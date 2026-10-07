//! Talks to a Monit agent's embedded HTTP server: fetch its status and
//! trigger service actions, either directly or tunnelled through SSH.

use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use base64::Engine;
use rand::distr::{Alphanumeric, SampleString};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct Target {
    pub base_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_skip_verify: bool,
    pub ssh: Option<Ssh>,
}

/// Reach the Monit HTTP interface through `ssh -W host:port destination`.
#[derive(Debug, Clone)]
pub struct Ssh {
    pub destination: String,
    pub port: Option<i64>,
    pub binary: String,
    pub identity_file: Option<PathBuf>,
    pub known_hosts_file: Option<PathBuf>,
}

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Target {
    pub fn describe(&self) -> String {
        match &self.ssh {
            Some(s) => format!("{} via ssh {}", self.base_url, s.destination),
            None => self.base_url.clone(),
        }
    }
}

async fn request(
    t: &Target,
    method: &str,
    path: &str,
    form: Option<String>,
    cookie: Option<String>,
) -> Result<Response> {
    match &t.ssh {
        None => direct(t, method, path, form, cookie).await,
        Some(ssh) => tokio::time::timeout(TIMEOUT, via_ssh(t, ssh, method, path, form, cookie))
            .await
            .with_context(|| format!("timeout talking to {}", t.describe()))?,
    }
}

async fn direct(
    t: &Target,
    method: &str,
    path: &str,
    form: Option<String>,
    cookie: Option<String>,
) -> Result<Response> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .danger_accept_invalid_certs(t.tls_skip_verify)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("monarch/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let url = format!("{}{path}", t.base_url.trim_end_matches('/'));
    let mut req = client.request(method.parse()?, &url);
    if let Some(u) = &t.username {
        req = req.basic_auth(u, t.password.as_deref());
    }
    if let Some(c) = cookie {
        req = req.header("Cookie", c);
    }
    if let Some(body) = form {
        req = req
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body);
    }
    let res = req
        .send()
        .await
        .with_context(|| format!("cannot reach monit at {}", t.base_url))?;
    Ok(Response {
        status: res.status().as_u16(),
        body: res.bytes().await?.to_vec(),
    })
}

async fn via_ssh(
    t: &Target,
    ssh: &Ssh,
    method: &str,
    path: &str,
    form: Option<String>,
    cookie: Option<String>,
) -> Result<Response> {
    let url = reqwest::Url::parse(&t.base_url).context("invalid monit URL")?;
    if url.scheme() != "http" {
        bail!("only http:// monit URLs can be used through SSH (the tunnel already encrypts)");
    }
    let host = url.host_str().context("monit URL has no host")?.to_owned();
    let port = url.port().unwrap_or(80);
    let mut cmd = tokio::process::Command::new(&ssh.binary);
    cmd.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ServerAliveInterval=10",
    ]);
    if let Some(k) = &ssh.known_hosts_file {
        cmd.arg("-o")
            .arg(format!("UserKnownHostsFile={}", k.display()));
    }
    if let Some(i) = &ssh.identity_file {
        cmd.arg("-o").arg("IdentitiesOnly=yes").arg("-i").arg(i);
    }
    if let Some(p) = ssh.port {
        cmd.arg("-p").arg(p.to_string());
    }
    let host_port = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    cmd.arg("-W")
        .arg(&host_port)
        .arg("--")
        .arg(&ssh.destination);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .with_context(|| format!("cannot run {}", ssh.binary))?;

    let mut head = format!(
        "{method} {path} HTTP/1.0\r\nHost: {host_port}\r\nConnection: close\r\nUser-Agent: monarch/{}\r\n",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(u) = &t.username {
        let creds = format!("{u}:{}", t.password.as_deref().unwrap_or(""));
        head.push_str(&format!(
            "Authorization: Basic {}\r\n",
            base64::engine::general_purpose::STANDARD.encode(creds)
        ));
    }
    if let Some(c) = cookie {
        head.push_str(&format!("Cookie: {c}\r\n"));
    }
    let body = form.unwrap_or_default();
    if !body.is_empty() {
        head.push_str(&format!(
            "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    head.push_str("\r\n");
    head.push_str(&body);

    let mut stdin = child.stdin.take().context("ssh stdin")?;
    let mut stdout = child.stdout.take().context("ssh stdout")?;
    let mut stderr = child.stderr.take().context("ssh stderr")?;
    stdin.write_all(head.as_bytes()).await?;
    stdin.flush().await?;
    // Keep stdin open: closing it makes `ssh -W` tear the channel down early.
    let mut raw = Vec::new();
    stdout.read_to_end(&mut raw).await?;
    drop(stdin);
    let mut err = String::new();
    let _ = stderr.read_to_string(&mut err).await;
    let _ = child.wait().await;
    if raw.is_empty() {
        let err = err.trim();
        bail!(
            "ssh to {} failed: {}",
            ssh.destination,
            if err.is_empty() { "no response" } else { err }
        );
    }
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Result<Response> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("malformed HTTP response")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .context("malformed HTTP status line")?;
    Ok(Response {
        status,
        body: raw[split + 4..].to_vec(),
    })
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
    let body = serde_urlencoded::to_string(&form)?;
    let res = request(
        t,
        "POST",
        "/_doaction",
        Some(body),
        Some(format!("securitytoken={token}")),
    )
    .await?;
    if (200..400).contains(&res.status) {
        return Ok(());
    }
    bail!(
        "monit answered {}: {}",
        res.status,
        extract_error(&String::from_utf8_lossy(&res.body))
    )
}

/// Fetch the full status document (same format Monit posts to its collector).
pub async fn fetch_status(t: &Target) -> Result<(Vec<u8>, Duration)> {
    let started = Instant::now();
    let res = request(t, "GET", "/_status2?format=xml&level=full", None, None).await?;
    match res.status {
        200 => Ok((res.body, started.elapsed())),
        401 => bail!("monit rejected the credentials (401)"),
        s => bail!(
            "monit answered {s}: {}",
            extract_error(&String::from_utf8_lossy(&res.body))
        ),
    }
}

/// Check that the agent is reachable and the credentials are accepted.
pub async fn probe(t: &Target) -> Result<Duration> {
    fetch_status(t).await.map(|(_, d)| d)
}

/// Monit's error pages are HTML; pull out something readable.
fn extract_error(body: &str) -> String {
    let text = regex::Regex::new(r"(?s)<[^>]*>")
        .map(|re| re.replace_all(body, " ").to_string())
        .unwrap_or_default();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_response() {
        let r =
            parse_response(b"HTTP/1.0 200 OK\r\nContent-Type: text/xml\r\n\r\n<monit/>").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, b"<monit/>");
    }
}
