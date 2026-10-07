use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Static configuration, from an optional TOML file and the environment.
/// Everything that can be changed at runtime lives in [`Settings`] instead.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Address to listen on.
    pub listen: SocketAddr,
    /// Path of the SQLite database.
    pub database: PathBuf,
    /// Externally reachable base URL; seeds the corresponding setting.
    pub public_url: Option<String>,
    /// Create this admin user on startup if no user exists yet.
    pub initial_admin_user: Option<String>,
    /// File containing the password for `initial_admin_user`.
    pub initial_admin_password_file: Option<PathBuf>,
    /// Accept collector posts without (valid) credentials.
    pub collector_allow_anonymous: bool,
    /// Session lifetime in days.
    pub session_days: i64,
    /// Users that are created (or whose password and role are reset) on startup.
    pub ensure_users: Vec<EnsureUser>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnsureUser {
    pub username: String,
    #[serde(default = "default_role")]
    pub role: String,
    pub password_file: PathBuf,
}

fn default_role() -> String {
    "collector".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8080".parse().unwrap(),
            database: PathBuf::from("monarch.db"),
            public_url: None,
            initial_admin_user: None,
            initial_admin_password_file: None,
            collector_allow_anonymous: false,
            session_days: 30,
            ensure_users: Vec::new(),
        }
    }
}

impl Config {
    pub fn load(path: Option<&PathBuf>) -> Result<Self> {
        let mut cfg = match path {
            Some(p) => {
                let raw = std::fs::read_to_string(p)
                    .with_context(|| format!("reading config {}", p.display()))?;
                toml::from_str(&raw).with_context(|| format!("parsing config {}", p.display()))?
            }
            None => Config::default(),
        };
        if let Ok(v) = std::env::var("MONARCH_LISTEN") {
            cfg.listen = v.parse().context("MONARCH_LISTEN")?;
        }
        if let Ok(v) = std::env::var("MONARCH_DATABASE") {
            cfg.database = v.into();
        }
        if let Ok(v) = std::env::var("MONARCH_PUBLIC_URL") {
            cfg.public_url = Some(v);
        }
        if let Ok(v) = std::env::var("MONARCH_INITIAL_ADMIN_USER") {
            cfg.initial_admin_user = Some(v);
        }
        if let Ok(v) = std::env::var("MONARCH_INITIAL_ADMIN_PASSWORD_FILE") {
            cfg.initial_admin_password_file = Some(v.into());
        }
        Ok(cfg)
    }
}

/// Runtime settings, editable from the UI and persisted in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub public_url: String,
    pub retention: Retention,
    /// Multiples of a host's poll interval without report before it is offline.
    pub heartbeat_grace: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Retention {
    pub raw_hours: i64,
    pub rollup_5m_days: i64,
    pub rollup_1h_days: i64,
    pub events_days: i64,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            raw_hours: 48,
            rollup_5m_days: 30,
            rollup_1h_days: 400,
            events_days: 180,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            public_url: "http://localhost:8080".into(),
            retention: Retention::default(),
            heartbeat_grace: 3.0,
        }
    }
}
