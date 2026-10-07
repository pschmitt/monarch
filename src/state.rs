use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::{RwLock, broadcast, watch};

use crate::config::{Config, Settings};

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub db: SqlitePool,
    pub config: Config,
    pub settings: RwLock<Settings>,
    /// Live updates for `/api/stream` (pre-serialized JSON messages).
    pub stream: broadcast::Sender<Arc<str>>,
    /// (host_id, service, metric) -> series id
    pub series: Mutex<HashMap<(i64, String, String), i64>>,
    /// sha256(user:pass) -> verified at; avoids an argon2 run per collector post.
    pub collector_auth: Mutex<HashMap<[u8; 32], Instant>>,
    pub http: reqwest::Client,
    /// Flipped to true on shutdown so long-lived streams end.
    pub shutdown: watch::Sender<bool>,
    /// Serializes ingestion (SQLite has a single writer anyway); avoids lock
    /// upgrade conflicts between concurrent push and pull reports.
    pub ingest_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(db: SqlitePool, config: Config, settings: Settings) -> Result<SharedState> {
        let (stream, _) = broadcast::channel(1024);
        let (shutdown, _) = watch::channel(false);
        Ok(Arc::new(Self {
            db,
            config,
            settings: RwLock::new(settings),
            stream,
            series: Mutex::new(HashMap::new()),
            collector_auth: Mutex::new(HashMap::new()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("monarch/", env!("CARGO_PKG_VERSION")))
                .build()?,
            shutdown,
            ingest_lock: tokio::sync::Mutex::new(()),
        }))
    }

    pub fn publish(&self, msg: Value) {
        // No receivers is fine.
        let _ = self.stream.send(Arc::from(msg.to_string()));
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn now_f() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
