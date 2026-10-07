//! Talks to a Monit agent's embedded HTTP server to trigger service actions.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rand::distr::{Alphanumeric, SampleString};

#[derive(Debug, Clone)]
pub struct Target {
    pub base_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_skip_verify: bool,
}

fn client(t: &Target) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .danger_accept_invalid_certs(t.tls_skip_verify)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("monarch/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn auth(req: reqwest::RequestBuilder, t: &Target) -> reqwest::RequestBuilder {
    match &t.username {
        Some(u) => req.basic_auth(u, t.password.as_deref()),
        None => req,
    }
}

/// Run `action` on the given services. Monit protects POSTs with a
/// double-submit CSRF cookie, so we simply pick a token and send it both as
/// cookie and form parameter.
pub async fn do_action(t: &Target, services: &[String], action: &str) -> Result<()> {
    let token = Alphanumeric.sample_string(&mut rand::rng(), 32);
    let mut form: Vec<(&str, &str)> = vec![("securitytoken", &token), ("action", action)];
    for s in services {
        form.push(("service", s));
    }
    let url = format!("{}/_doaction", t.base_url.trim_end_matches('/'));
    let res = auth(client(t)?.post(&url), t)
        .header("Cookie", format!("securitytoken={token}"))
        .form(&form)
        .send()
        .await
        .with_context(|| format!("cannot reach monit at {}", t.base_url))?;
    let status = res.status();
    if status.is_success() || status.is_redirection() {
        return Ok(());
    }
    let body = res.text().await.unwrap_or_default();
    bail!("monit answered {status}: {}", extract_error(&body))
}

/// Check that the agent is reachable and the credentials are accepted.
pub async fn probe(t: &Target) -> Result<Duration> {
    let started = Instant::now();
    let url = format!("{}/_status?format=xml", t.base_url.trim_end_matches('/'));
    let res = auth(client(t)?.get(&url), t)
        .send()
        .await
        .with_context(|| format!("cannot reach monit at {}", t.base_url))?;
    let status = res.status();
    if !status.is_success() {
        bail!("monit answered {status}");
    }
    Ok(started.elapsed())
}

/// Monit's error pages are HTML; pull out something readable.
fn extract_error(body: &str) -> String {
    let text = regex::Regex::new(r"(?s)<[^>]*>")
        .map(|re| re.replace_all(body, " ").to_string())
        .unwrap_or_default();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(300).collect()
}
