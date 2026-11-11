//! A small client for the Monarch HTTP API.
//!
//! Methods return raw JSON so `--json` can print exactly what the server sent;
//! callers decode into the types of [`crate::types`] with [`decode`].

use std::pin::Pin;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use reqwest::{Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::config::{Resolved, Secret};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("unauthorized: the server rejected the token (expired or revoked?)")]
    Unauthorized,
    #[error(
        "permission denied: {0} (actions need an operator or admin token, viewer tokens are read-only)"
    )]
    Forbidden(String),
    #[error("server returned {status}: {message}")]
    Status { status: u16, message: String },
    #[error("request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("unexpected response: {0}")]
    Decode(String),
    #[error("invalid server URL: {0}")]
    Url(String),
}

impl ApiError {
    pub fn is_forbidden(&self) -> bool {
        matches!(self, Self::Forbidden(_))
    }
}

pub type Result<T, E = ApiError> = std::result::Result<T, E>;

pub fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|err| ApiError::Decode(err.to_string()))
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    token: Secret,
    timeout: Duration,
}

impl Client {
    pub fn new(config: &Resolved, token: Secret) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("monarchctl/", env!("CARGO_PKG_VERSION")))
            .danger_accept_invalid_certs(config.tls_skip_verify)
            .connect_timeout(config.timeout)
            .build()?;
        let base = Url::parse(&config.url).map_err(|err| ApiError::Url(err.to_string()))?;
        if base.cannot_be_a_base() {
            return Err(ApiError::Url(config.url.clone()));
        }
        Ok(Self {
            http,
            base,
            token,
            timeout: config.timeout,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    /// Builds `<base>/seg/seg/...`, percent-encoding every segment.
    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        url
    }

    fn builder(&self, method: Method, segments: &[&str]) -> reqwest::RequestBuilder {
        self.http
            .request(method, self.url(segments))
            .bearer_auth(self.token.expose())
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        let response = request.timeout(self.timeout).send().await?;
        let status = response.status();
        if status.is_success() {
            let bytes = response.bytes().await?;
            if bytes.is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_slice(&bytes).map_err(|err| ApiError::Decode(err.to_string()));
        }
        let body = response.text().await.unwrap_or_default();
        Err(error_for(status, &body))
    }

    pub async fn me(&self) -> Result<Value> {
        self.send(self.builder(Method::GET, &["api", "auth", "me"]))
            .await
    }

    pub async fn overview(&self) -> Result<Value> {
        self.send(self.builder(Method::GET, &["api", "overview"]))
            .await
    }

    pub async fn hosts(&self) -> Result<Value> {
        self.send(self.builder(Method::GET, &["api", "hosts"]))
            .await
    }

    pub async fn host(&self, id: u64) -> Result<Value> {
        self.send(self.builder(Method::GET, &["api", "hosts", &id.to_string()]))
            .await
    }

    pub async fn service(&self, host: u64, name: &str) -> Result<Value> {
        let request = self.builder(
            Method::GET,
            &["api", "hosts", &host.to_string(), "services", name],
        );
        self.send(request).await
    }

    /// Metric history of one service between two unix timestamps.
    pub async fn metrics(
        &self,
        host: u64,
        service: &str,
        keys: &[&str],
        from: f64,
        to: f64,
    ) -> Result<Value> {
        let query = [
            ("host", host.to_string()),
            ("service", service.to_owned()),
            ("metrics", keys.join(",")),
            ("from", (from as i64).to_string()),
            ("to", (to as i64).to_string()),
        ];
        self.send(self.builder(Method::GET, &["api", "metrics"]).query(&query))
            .await
    }

    pub async fn services(&self, state: Option<&str>, q: Option<&str>) -> Result<Value> {
        let mut query = Vec::new();
        if let Some(state) = state {
            query.push(("state", state));
        }
        if let Some(q) = q {
            query.push(("q", q));
        }
        self.send(
            self.builder(Method::GET, &["api", "services"])
                .query(&query),
        )
        .await
    }

    pub async fn events(&self, host: Option<u64>, unacked: bool, limit: usize) -> Result<Value> {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(host) = host {
            query.push(("host", host.to_string()));
        }
        if unacked {
            query.push(("unacked", "1".to_owned()));
        }
        self.send(self.builder(Method::GET, &["api", "events"]).query(&query))
            .await
    }

    pub async fn ack(&self, ids: &[u64]) -> Result<Value> {
        self.send(
            self.builder(Method::POST, &["api", "events", "ack"])
                .json(&json!({ "ids": ids })),
        )
        .await
    }

    pub async fn service_action(&self, host: u64, service: &str, action: &str) -> Result<Value> {
        let request = self
            .builder(
                Method::POST,
                &[
                    "api",
                    "hosts",
                    &host.to_string(),
                    "services",
                    service,
                    "action",
                ],
            )
            .json(&json!({ "action": action }));
        self.send(request).await
    }

    pub async fn bulk_action(&self, host: u64, services: &[String], action: &str) -> Result<Value> {
        let request = self
            .builder(Method::POST, &["api", "hosts", &host.to_string(), "action"])
            .json(&json!({ "action": action, "services": services }));
        self.send(request).await
    }

    /// `muted_until`: a unix timestamp, `"forever"` or `null` to unmute.
    pub async fn set_muted(&self, host: u64, muted_until: Value) -> Result<Value> {
        let request = self
            .builder(Method::PATCH, &["api", "hosts", &host.to_string()])
            .json(&json!({ "muted_until": muted_until }));
        self.send(request).await
    }

    pub async fn targets(&self) -> Result<Value> {
        self.send(self.builder(Method::GET, &["api", "targets"]))
            .await
    }

    pub async fn poll_target(&self, id: u64) -> Result<Value> {
        self.send(self.builder(Method::POST, &["api", "targets", &id.to_string(), "poll"]))
            .await
    }

    /// Opens the Server-Sent Events stream. It has no overall timeout.
    pub async fn stream(&self) -> Result<EventStream> {
        let response = self
            .builder(Method::GET, &["api", "stream"])
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(error_for(status, &body));
        }
        Ok(EventStream {
            body: Box::pin(response.bytes_stream()),
            parser: SseParser::default(),
            queue: Vec::new(),
        })
    }
}

fn error_for(status: StatusCode, body: &str) -> ApiError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| body.trim().chars().take(200).collect());
    let message = if message.is_empty() {
        status.canonical_reason().unwrap_or("error").to_owned()
    } else {
        message
    };
    match status {
        StatusCode::UNAUTHORIZED => ApiError::Unauthorized,
        StatusCode::FORBIDDEN => ApiError::Forbidden(message),
        _ => ApiError::Status {
            status: status.as_u16(),
            message,
        },
    }
}

type ByteStream = Pin<Box<dyn Stream<Item = reqwest::Result<bytes::Bytes>> + Send>>;

/// A parsed Server-Sent Events connection yielding one JSON value per message.
pub struct EventStream {
    body: ByteStream,
    parser: SseParser,
    queue: Vec<String>,
}

/// How long the stream may stay silent; the server pings well within this.
const STREAM_IDLE: Duration = Duration::from_secs(120);

impl EventStream {
    /// The next message, `Ok(None)` when the server closed the stream.
    pub async fn next(&mut self) -> Result<Option<Value>> {
        loop {
            if !self.queue.is_empty() {
                let data = self.queue.remove(0);
                match serde_json::from_str(&data) {
                    Ok(value) => return Ok(Some(value)),
                    Err(_) => continue, // not JSON (a comment-like payload): skip it
                }
            }
            match tokio::time::timeout(STREAM_IDLE, self.body.next()).await {
                Err(_) => return Err(ApiError::Decode("the event stream went silent".into())),
                Ok(None) => return Ok(None),
                Ok(Some(chunk)) => self.queue = self.parser.push(&chunk?),
            }
        }
    }
}

/// Incremental parser for `text/event-stream`: returns the `data` of each complete message.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut messages = Vec::new();
        while let Some((end, skip)) = find_message_end(&self.buffer) {
            let raw: Vec<u8> = self.buffer.drain(..end + skip).take(end).collect();
            let text = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
            let data: Vec<&str> = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
                .collect();
            if !data.is_empty() {
                messages.push(data.join("\n"));
            }
        }
        messages
    }
}

/// Position of the first blank line: (end of the message, length of the separator).
fn find_message_end(buffer: &[u8]) -> Option<(usize, usize)> {
    (0..buffer.len()).find_map(|i| {
        if buffer[i..].starts_with(b"\n\n") {
            Some((i, 2))
        } else if buffer[i..].starts_with(b"\r\n\r\n") {
            Some((i, 4))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_messages_across_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"data: {\"type\":\"pi").is_empty());
        let out = parser.push(b"ng\"}\n\n: keepalive\n\ndata: {\"a\":1}\r\n\r\nda");
        assert_eq!(out, vec![r#"{"type":"ping"}"#, r#"{"a":1}"#]);
        assert_eq!(parser.push(b"ta:x\ndata:y\n\n"), vec!["x\ny"]);
    }

    #[test]
    fn error_messages() {
        assert!(matches!(
            error_for(StatusCode::UNAUTHORIZED, ""),
            ApiError::Unauthorized
        ));
        let forbidden = error_for(
            StatusCode::FORBIDDEN,
            r#"{"error":"operator role required"}"#,
        );
        assert!(forbidden.is_forbidden());
        assert!(forbidden.to_string().contains("operator role required"));
        let bad = error_for(StatusCode::BAD_GATEWAY, r#"{"error":"monit unreachable"}"#);
        assert_eq!(bad.to_string(), "server returned 502: monit unreachable");
        let plain = error_for(StatusCode::NOT_FOUND, "");
        assert!(plain.to_string().contains("Not Found"));
    }

    #[test]
    fn urls_encode_segments_and_keep_a_path_prefix() {
        let config = Resolved {
            url: "https://example.com/monarch/".into(),
            token: crate::config::TokenSource::Literal(Secret::new("t")),
            tls_skip_verify: false,
            timeout: Duration::from_secs(1),
        };
        let client = Client::new(&config, Secret::new("t")).unwrap();
        assert_eq!(
            client
                .url(&["api", "hosts", "1", "services", "a/b c", "action"])
                .as_str(),
            "https://example.com/monarch/api/hosts/1/services/a%2Fb%20c/action"
        );
    }
}
