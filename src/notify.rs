//! Alert notifications (webhook, ntfy, gotify, slack, discord, telegram, email,
//! apprise, browser push, command).

use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, bail};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{MultiPart, header::ContentType},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;

use crate::{
    state::{SharedState, now},
    views::{self, EventRow},
};

pub const KINDS: [&str; 10] = [
    "webhook", "ntfy", "gotify", "slack", "discord", "telegram", "email", "apprise", "webpush",
    "exec",
];

/// Config keys holding secrets; never sent back to the browser.
pub const SECRET_KEYS: [&str; 4] = ["token", "smtp_url", "headers", "apprise_url"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub hosts: Option<String>,
    pub services: Option<String>,
    pub states: Vec<String>,
    pub include_heartbeat: bool,
    /// Event kinds to notify about (see `model::EVENTS`); `None` = all, an empty list = none.
    pub events: Option<Vec<String>>,
    /// Collection window in minutes for this channel; `None` = the global setting.
    pub group_minutes: Option<u32>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            hosts: None,
            services: None,
            states: vec!["failed".into(), "succeeded".into()],
            include_heartbeat: true,
            events: None,
            group_minutes: None,
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
    /// Receives the events no other channel's routing claims.
    pub is_default: i64,
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
        Some(r) => Regex::new(&format!("(?i){r}"))
            .map(|re| re.is_match(value))
            .unwrap_or(false),
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
        if let Some(events) = &self.events
            && !events
                .iter()
                .any(|k| k == crate::monit::model::event_kind(e.event_type))
        {
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

/// The channels an event goes to: every enabled channel whose routing accepts it,
/// except the default channel, which only gets what no other channel claims.
pub fn route<'a>(channels: &'a [ChannelRow], e: &EventRow) -> Vec<&'a ChannelRow> {
    let accepted: Vec<&ChannelRow> = channels
        .iter()
        .filter(|c| c.enabled != 0 && c.filter().accepts(e))
        .collect();
    let routed: Vec<&ChannelRow> = accepted
        .iter()
        .copied()
        .filter(|c| c.is_default == 0)
        .collect();
    if routed.is_empty() {
        accepted.into_iter().filter(|c| c.is_default != 0).collect()
    } else {
        routed
    }
}

async fn dispatch_inner(state: &SharedState, e: &EventRow) -> Result<()> {
    if let Some(host_id) = e.host_id {
        let muted: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT muted_until FROM hosts WHERE id = ?")
                .bind(host_id)
                .fetch_optional(&state.db)
                .await?;
        if let Some((Some(until),)) = muted
            && until > now()
        {
            return Ok(());
        }
    }
    let kind = crate::monit::model::event_kind(e.event_type);
    // Settings of this very check beat the generic ones.
    let check = match (e.host_id, e.service.as_deref()) {
        (Some(host_id), Some(service)) => {
            crate::api::check_alerts::fetch(state, host_id, service).await?
        }
        _ => None,
    };
    if check.as_ref().is_some_and(|c| c.muted != 0) {
        return Ok(());
    }
    let kind_enabled = match check.as_ref().and_then(|c| c.events()) {
        Some(kinds) => kinds.iter().any(|k| k == kind),
        None => !state
            .settings
            .read()
            .await
            .disabled_events
            .iter()
            .any(|k| k == kind),
    };
    if !kind_enabled {
        return Ok(());
    }
    let channels: Vec<ChannelRow> = sqlx::query_as("SELECT * FROM channels WHERE enabled = 1")
        .fetch_all(&state.db)
        .await?;
    let public_url = state.settings.read().await.public_url.clone();
    let n = render(&public_url, e);
    let default_window = state.settings.read().await.group_minutes;
    let targets: Vec<&ChannelRow> = match check.as_ref().and_then(|c| c.channels()) {
        Some(ids) => channels.iter().filter(|c| ids.contains(&c.id)).collect(),
        None => route(&channels, e),
    };
    for c in targets {
        let window = c.filter().group_minutes.unwrap_or(default_window);
        if window == 0 {
            deliver(state, c, &n).await?;
        } else {
            enqueue(state, c, e, Duration::from_secs(u64::from(window) * 60));
        }
    }
    Ok(())
}

/// Send a notification and record the outcome on the channel.
async fn deliver(state: &SharedState, c: &ChannelRow, n: &Notification) -> Result<()> {
    let result = send(state, c, n).await;
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
    Ok(())
}

/// Collect an event for a channel; one message goes out when its window closes.
fn enqueue(state: &SharedState, c: &ChannelRow, e: &EventRow, window: Duration) {
    let first = {
        let mut pending = state.pending.lock().unwrap();
        let queue = pending.entry(c.id).or_default();
        queue.push(e.clone());
        queue.len() == 1
    };
    if first {
        let state = state.clone();
        let id = c.id;
        tokio::spawn(async move {
            tokio::time::sleep(window).await;
            if let Err(err) = flush_channel(&state, id).await {
                tracing::error!("flushing notifications of channel {id} failed: {err:#}");
            }
        });
    }
}

async fn flush_channel(state: &SharedState, id: i64) -> Result<()> {
    let events = state
        .pending
        .lock()
        .unwrap()
        .remove(&id)
        .unwrap_or_default();
    if events.is_empty() {
        return Ok(());
    }
    let channel: Option<ChannelRow> = sqlx::query_as("SELECT * FROM channels WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let Some(c) = channel.filter(|c| c.enabled != 0) else {
        return Ok(());
    };
    let public_url = state.settings.read().await.public_url.clone();
    deliver(state, &c, &render_group(&public_url, &events)).await
}

/// Send everything still collected (used on shutdown).
pub async fn flush_all(state: &SharedState) {
    let ids: Vec<i64> = state.pending.lock().unwrap().keys().copied().collect();
    for id in ids {
        if let Err(err) = flush_channel(state, id).await {
            tracing::error!("flushing notifications of channel {id} failed: {err:#}");
        }
    }
}

/// One message for several events: a summary title and one line per event.
pub fn render_group(public_url: &str, events: &[EventRow]) -> Notification {
    if let [e] = events {
        return render(public_url, e);
    }
    let notes: Vec<Notification> = events.iter().map(|e| render(public_url, e)).collect();
    let failed = notes.iter().any(|n| n.failed);
    let mut hosts: Vec<&str> = events.iter().filter_map(|e| e.host.as_deref()).collect();
    hosts.sort_unstable();
    hosts.dedup();
    let shown = hosts.iter().take(3).copied().collect::<Vec<_>>().join(", ");
    let more = hosts.len().saturating_sub(3);
    let title = format!(
        "{} {} notifications{}{}",
        if failed { "🔴" } else { "🟢" },
        events.len(),
        if shown.is_empty() {
            String::new()
        } else {
            format!(" · {shown}")
        },
        if more > 0 {
            format!(" +{more}")
        } else {
            String::new()
        },
    );
    const MAX_LINES: usize = 25;
    let mut lines: Vec<String> = notes
        .iter()
        .take(MAX_LINES)
        .map(|n| match n.text.lines().next() {
            Some(first) if !first.trim().is_empty() => format!("{} — {}", n.title, first.trim()),
            _ => n.title.clone(),
        })
        .collect();
    if notes.len() > MAX_LINES {
        lines.push(format!("… and {} more", notes.len() - MAX_LINES));
    }
    Notification {
        title,
        text: lines.join("\n"),
        url: format!("{}/events", public_url.trim_end_matches('/')),
        failed,
        event: json!({"count": events.len(), "events": notes.iter().map(|n| n.event.clone()).collect::<Vec<_>>()}),
    }
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
    bail!(
        "HTTP {status}: {}",
        body.chars().take(200).collect::<String>()
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Run a user-configured command; the notification is passed in the environment
/// (MONARCH_*) and as JSON on stdin.
async fn run_command(command: &str, n: &Notification) -> Result<()> {
    use std::{process::Stdio, time::Duration};
    use tokio::{io::AsyncWriteExt, process::Command};

    let field = |k: &str| n.event[k].as_str().unwrap_or("").to_owned();
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("MONARCH_TITLE", &n.title)
        .env("MONARCH_TEXT", &n.text)
        .env("MONARCH_URL", &n.url)
        .env("MONARCH_FAILED", if n.failed { "1" } else { "" })
        .env("MONARCH_HOST", field("host"))
        .env("MONARCH_SERVICE", field("service"))
        .env("MONARCH_KIND", field("kind"))
        .env("MONARCH_EVENT_JSON", n.event.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("starting /bin/sh")?;
    if let Some(mut stdin) = child.stdin.take() {
        // The command may not read stdin; a closed pipe is fine.
        let _ = stdin.write_all(n.event.to_string().as_bytes()).await;
    }
    let out = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .context("command timed out after 30s")??;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!(
            "{}: {}",
            out.status,
            err.trim().chars().take(200).collect::<String>()
        );
    }
    Ok(())
}

pub async fn send(state: &SharedState, c: &ChannelRow, n: &Notification) -> Result<()> {
    let conf = c.config_map();
    let http = &state.http;
    match c.kind.as_str() {
        "webhook" => {
            let method = conf
                .get("method")
                .map(|m| m.to_uppercase())
                .unwrap_or_else(|| "POST".into());
            let mut req = http.request(
                method.parse().unwrap_or(reqwest::Method::POST),
                cfg(&conf, "url")?,
            );
            for line in conf
                .get("headers")
                .map(String::as_str)
                .unwrap_or("")
                .lines()
            {
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
                .header(
                    "Tags",
                    if n.failed {
                        "rotating_light"
                    } else {
                        "white_check_mark"
                    },
                );
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
            let url = format!(
                "https://api.telegram.org/bot{}/sendMessage",
                cfg(&conf, "token")?
            );
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
                .from(
                    cfg(&conf, "from")?
                        .parse()
                        .context("invalid from address")?,
                )
                .subject(&n.title);
            let mut recipients: Vec<String> = conf
                .get("to")
                .map(String::as_str)
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            // Also everyone with one of the listed roles who has an email address.
            let roles: Vec<String> = conf
                .get("to_roles")
                .map(String::as_str)
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            if !roles.is_empty() {
                let rows: Vec<(String,)> = sqlx::query_as(
                    "SELECT email FROM users WHERE email IS NOT NULL AND email != ''
                       AND role IN (SELECT value FROM json_each(?))",
                )
                .bind(serde_json::to_string(&roles)?)
                .fetch_all(&state.db)
                .await?;
                recipients.extend(rows.into_iter().map(|(e,)| e));
            }
            recipients.sort();
            recipients.dedup();
            if recipients.is_empty() {
                bail!(
                    "no recipients: set `to` or `to_roles` (and give those users an email address)"
                );
            }
            for to in &recipients {
                msg = msg.to(to
                    .parse()
                    .with_context(|| format!("invalid address {to}"))?);
            }
            let plain = format!("{}\n\n{}\n", n.text, n.url);
            let msg = if conf.get("format").map(String::as_str) == Some("text") {
                msg.header(ContentType::TEXT_PLAIN).body(plain)?
            } else {
                // HTML with a plain-text alternative; digests list their events,
                // single events add the host's current state and graphs.
                let host = match n.event["host_id"].as_i64() {
                    Some(id) if !n.event["events"].is_array() => {
                        views::host_summary_by_id(&state.db, id).await?
                    }
                    _ => None,
                };
                let html = crate::mail_html::render(n, host.as_ref(), &c.name);
                msg.multipart(MultiPart::alternative_plain_html(plain, html))?
            };
            mailer.send(msg).await?;
            Ok(())
        }
        "apprise" => {
            let mut body = json!({
                "title": n.title,
                "body": format!("{}\n{}", n.text, n.url),
                "type": if n.failed { "failure" } else { "success" },
                "format": "text",
            });
            if let Ok(tag) = cfg(&conf, "tag") {
                body["tag"] = json!(tag);
            }
            check(
                http.post(cfg(&conf, "apprise_url")?)
                    .json(&body)
                    .send()
                    .await?,
            )
            .await
        }
        "webpush" => {
            crate::push::send_channel(state, n, conf.get("users").map(String::as_str)).await
        }
        "exec" => {
            if !state.config.allow_exec_channels {
                bail!("exec channels are disabled (set allow_exec_channels)");
            }
            run_command(cfg(&conf, "command")?, n).await
        }
        other => bail!("unknown channel kind {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(id: i64, is_default: bool, events: Option<&[&str]>) -> ChannelRow {
        let filter = Filter {
            events: events.map(|e| e.iter().map(|s| s.to_string()).collect()),
            ..Filter::default()
        };
        ChannelRow {
            id,
            name: format!("c{id}"),
            kind: "webhook".into(),
            config: "{}".into(),
            filter: serde_json::to_string(&filter).unwrap(),
            enabled: 1,
            is_default: is_default as i64,
            created_at: 0,
            last_status: None,
            last_sent_at: None,
        }
    }

    fn event(event_type: i64) -> EventRow {
        EventRow {
            id: 0,
            host_id: None,
            host: Some("h".into()),
            service: Some("s".into()),
            service_type: None,
            event_type,
            state: 1,
            action: 1,
            message: String::new(),
            created_at: 0.0,
            source: "monit".into(),
            acked_by: None,
            acked_at: None,
        }
    }

    fn ids(v: Vec<&ChannelRow>) -> Vec<i64> {
        v.into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn groups_events_into_one_message() {
        let events = vec![event(0x200000), event(0x40000000), event(0x200000)];
        let n = render_group("https://m.example", &events);
        assert!(n.title.contains("3 notifications"), "{}", n.title);
        assert!(n.title.contains("· h"), "{}", n.title);
        assert_eq!(n.text.lines().count(), 3);
        assert_eq!(n.url, "https://m.example/events");
        assert_eq!(n.event["count"], 3);
        // A single event is rendered like an ungrouped one.
        assert_eq!(
            render_group("https://m.example", &events[..1]).title,
            render("https://m.example", &events[0]).title
        );
    }

    #[test]
    fn routed_events_skip_the_default_channel() {
        // 0x200000 = "status", 0x40000000 = "exist"
        let channels = vec![
            channel(1, true, None),
            channel(2, false, Some(&["status"])),
            channel(3, false, Some(&[])),
        ];
        assert_eq!(ids(route(&channels, &event(0x200000))), vec![2]);
        // Nothing claims "exist": it falls back to the default channel.
        assert_eq!(ids(route(&channels, &event(0x40000000))), vec![1]);
    }

    #[test]
    fn catch_all_channels_keep_receiving_everything() {
        let channels = vec![channel(1, true, None), channel(2, false, None)];
        assert_eq!(ids(route(&channels, &event(0x40000000))), vec![2]);
    }

    #[test]
    fn disabled_channels_do_not_claim_events() {
        let mut routed = channel(2, false, Some(&["status"]));
        routed.enabled = 0;
        let channels = vec![channel(1, true, None), routed];
        assert_eq!(ids(route(&channels, &event(0x200000))), vec![1]);
    }

    #[test]
    fn no_default_means_unrouted_events_are_dropped() {
        let channels = vec![channel(2, false, Some(&["status"]))];
        assert!(route(&channels, &event(0x40000000)).is_empty());
    }
}
