//! Alert notifications (webhook, ntfy, gotify, slack, discord, telegram, email).

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    state::{SharedState, now},
    views::{self, EventRow},
};

pub const KINDS: [&str; 7] = ["webhook", "ntfy", "gotify", "slack", "discord", "telegram", "email"];

/// Config keys holding secrets; never sent back to the browser.
pub const SECRET_KEYS: [&str; 3] = ["token", "smtp_url", "headers"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub hosts: Option<String>,
    pub services: Option<String>,
    pub states: Vec<String>,
    pub include_heartbeat: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            hosts: None,
            services: None,
            states: vec!["failed".into(), "succeeded".into()],
            include_heartbeat: true,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ChannelRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub config: String,
    pub filter: String,
    pub enabled: i64,
    pub created_at: i64,
    pub last_status: Option<String>,
    pub last_sent_at: Option<i64>,
}

impl ChannelRow {
    pub fn config_map(&self) -> HashMap<String, String> {
        serde_json::from_str(&self.config).unwrap_or_default()
    }
    pub fn filter(&self) -> Filter {
        serde_json::from_str(&self.filter).unwrap_or_default()
    }
}

fn matches(re: &Option<String>, value: &str) -> bool {
    match re.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        None => true,
        Some(r) => Regex::new(&format!("(?i){r}")).map(|re| re.is_match(value)).unwrap_or(false),
    }
}

impl Filter {
    fn accepts(&self, e: &EventRow) -> bool {
        if e.source == "monarch" && !self.include_heartbeat {
            return false;
        }
        let state = crate::monit::model::event_state_name(e.state);
        if !self.states.is_empty() && !self.states.iter().any(|s| s == state) {
            return false;
        }
        matches(&self.hosts, e.host.as_deref().unwrap_or(""))
            && matches(&self.services, e.service.as_deref().unwrap_or(""))
    }
}

pub struct Notification {
    pub title: String,
    pub text: String,
    pub url: String,
    pub failed: bool,
    pub event: Value,
}

pub fn render(public_url: &str, e: &EventRow) -> Notification {
    let ev = views::event_json(e);
    let failed = e.state == 1;
    let icon = match e.state {
        0 => "🟢",
        1 => "🔴",
        _ => "🔵",
    };
    let host = e.host.as_deref().unwrap_or("monarch");
    let label = ev["kind_label"].as_str().unwrap_or("Event");
    let title = match &e.service {
        Some(s) => format!("{icon} {host} · {s}: {label}"),
        None => format!("{icon} {host}: {label}"),
    };
    let base = public_url.trim_end_matches('/');
    let url = match (e.host_id, &e.service) {
        (Some(h), Some(s)) => format!("{base}/hosts/{h}/services/{}", urlencode(s)),
        (Some(h), None) => format!("{base}/hosts/{h}"),
        _ => format!("{base}/events"),
    };
    Notification {
        title,
        text: e.message.clone(),
        url,
        failed,
        event: ev,
    }
}

fn urlencode(s: &str) -> String {
    serde_urlencoded::to_string([("", s)])
        .map(|v| v.trim_start_matches('=').replace('+', "%20"))
        .unwrap_or_default()
}

/// Fire and forget: deliver an event to all matching channels.
pub fn dispatch(state: SharedState, e: EventRow) {
    tokio::spawn(async move {
        if let Err(err) = dispatch_inner(&state, &e).await {
            tracing::error!("notification dispatch failed: {err:#}");
        }
    });
}

async fn dispatch_inner(state: &SharedState, e: &EventRow) -> Result<()> {
    if let Some(host_id) = e.host_id {
        let muted: Option<(Option<i64>,)> = sqlx::query_as("SELECT muted_until FROM hosts WHERE id = ?")
            .bind(host_id)
            .fetch_optional(&state.db)
            .await?;
        if let Some((Some(until),)) = muted
            && until > now()
        {
            return Ok(());
        }
    }
    let channels: Vec<ChannelRow> = sqlx::query_as("SELECT * FROM channels WHERE enabled = 1")
        .fetch_all(&state.db)
        .await?;
    let public_url = state.settings.read().await.public_url.clone();
    let n = render(&public_url, e);
    for c in channels.into_iter().filter(|c| c.filter().accepts(e)) {
        let result = send(state, &c, &n).await;
        let status = match &result {
            Ok(()) => "ok".to_owned(),
            Err(err) => {
                tracing::warn!(channel = %c.name, "notification failed: {err:#}");
                format!("error: {err:#}")
            }
        };
        sqlx::query("UPDATE channels SET last_status = ?, last_sent_at = ? WHERE id = ?")
            .bind(status)
            .bind(now())
            .bind(c.id)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

fn cfg<'a>(c: &'a HashMap<String, String>, key: &str) -> Result<&'a str> {
    c.get(key)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing `{key}`"))
}

async fn check(res: reqwest::Response) -> Result<()> {
    let status = res.status();
    if status.is_success() {
        return Ok(());
    }
    let body = res.text().await.unwrap_or_default();
    bail!("HTTP {status}: {}", body.chars().take(200).collect::<String>())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub async fn send(state: &SharedState, c: &ChannelRow, n: &Notification) -> Result<()> {
    let conf = c.config_map();
    let http = &state.http;
    match c.kind.as_str() {
        "webhook" => {
            let method = conf.get("method").map(|m| m.to_uppercase()).unwrap_or_else(|| "POST".into());
            let mut req = http.request(method.parse().unwrap_or(reqwest::Method::POST), cfg(&conf, "url")?);
            for line in conf.get("headers").map(String::as_str).unwrap_or("").lines() {
                if let Some((k, v)) = line.split_once(':') {
                    req = req.header(k.trim(), v.trim());
                }
            }
            let body = json!({"title": n.title, "text": n.text, "url": n.url, "failed": n.failed, "event": n.event});
            check(req.json(&body).send().await?).await
        }
        "ntfy" => {
            let mut req = http
                .post(cfg(&conf, "url")?)
                .header("Title", n.title.clone())
                .header("Click", n.url.clone())
                .header("Priority", if n.failed { "4" } else { "3" })
                .header("Tags", if n.failed { "rotating_light" } else { "white_check_mark" });
            if let Ok(token) = cfg(&conf, "token") {
                req = req.bearer_auth(token);
            }
            check(req.body(n.text.clone()).send().await?).await
        }
        "gotify" => {
            let url = format!("{}/message", cfg(&conf, "url")?.trim_end_matches('/'));
            let body = json!({
                "title": n.title, "message": n.text, "priority": if n.failed { 8 } else { 4 },
                "extras": {"client::notification": {"click": {"url": n.url}}},
            });
            check(
                http.post(url)
                    .header("X-Gotify-Key", cfg(&conf, "token")?)
                    .json(&body)
                    .send()
                    .await?,
            )
            .await
        }
        "slack" => {
            let body = json!({"text": format!("*<{}|{}>*\n{}", n.url, n.title, n.text)});
            check(http.post(cfg(&conf, "url")?).json(&body).send().await?).await
        }
        "discord" => {
            let body = json!({"embeds": [{
                "title": n.title.chars().take(256).collect::<String>(),
                "description": n.text.chars().take(4000).collect::<String>(),
                "url": n.url,
                "color": if n.failed { 0xf43f5e } else { 0x10b981 },
            }]});
            check(http.post(cfg(&conf, "url")?).json(&body).send().await?).await
        }
        "telegram" => {
            let url = format!("https://api.telegram.org/bot{}/sendMessage", cfg(&conf, "token")?);
            let text = format!(
                "<b><a href=\"{}\">{}</a></b>\n{}",
                html_escape(&n.url),
                html_escape(&n.title),
                html_escape(&n.text)
            );
            let body = json!({"chat_id": cfg(&conf, "chat_id")?, "text": text, "parse_mode": "HTML", "disable_web_page_preview": true});
            check(http.post(url).json(&body).send().await?).await
        }
        "email" => {
            let mailer = AsyncSmtpTransport::<Tokio1Executor>::from_url(cfg(&conf, "smtp_url")?)
                .context("invalid smtp_url")?
                .build();
            let mut msg = Message::builder()
                .from(cfg(&conf, "from")?.parse().context("invalid from address")?)
                .subject(&n.title)
                .header(ContentType::TEXT_PLAIN);
            for to in cfg(&conf, "to")?.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                msg = msg.to(to.parse().with_context(|| format!("invalid address {to}"))?);
            }
            let msg = msg.body(format!("{}\n\n{}\n", n.text, n.url))?;
            mailer.send(msg).await?;
            Ok(())
        }
        other => bail!("unknown channel kind {other}"),
    }
}
