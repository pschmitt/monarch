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
    /// SSH client used for connections with an SSH destination.
    pub ssh_binary: String,
    /// Private key for SSH connections (default: ssh's own defaults).
    pub ssh_identity_file: Option<PathBuf>,
    /// known_hosts file for SSH connections (default: next to the database).
    pub ssh_known_hosts_file: Option<PathBuf>,
    /// Monit agents to poll, managed declaratively (read only in the UI).
    pub targets: Vec<TargetConfig>,
    /// Single sign-on via OpenID Connect (e.g. Authelia, Authentik, Keycloak).
    pub oidc: Option<OidcConfig>,
    /// Only allow OIDC sign-in in the UI (collector credentials still work).
    pub disable_password_login: bool,
    /// Allow admins to create "exec" notification channels, which run a command
    /// on the server. Off by default: it turns an admin login into code execution.
    pub allow_exec_channels: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret_file: Option<PathBuf>,
    /// Label of the sign-in button.
    #[serde(default = "default_oidc_name")]
    pub display_name: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    #[serde(default = "default_username_claim")]
    pub username_claim: String,
    #[serde(default = "default_groups_claim")]
    pub groups_claim: String,
    #[serde(default)]
    pub admin_groups: Vec<String>,
    #[serde(default)]
    pub operator_groups: Vec<String>,
    /// Role for users in none of the groups above; "none" denies access.
    #[serde(default = "default_oidc_role")]
    pub default_role: String,
    /// Link a first sign-in to the existing local account with the same
    /// username (keeping its role and password) instead of creating a separate
    /// SSO account. Only enable this when the identity provider controls who
    /// can hold a given username.
    #[serde(default)]
    pub link_local_users: bool,
}

fn default_oidc_name() -> String {
    "Single sign-on".into()
}
fn default_scopes() -> Vec<String> {
    ["openid", "profile", "email", "groups"]
        .map(String::from)
        .to_vec()
}
fn default_username_claim() -> String {
    "preferred_username".into()
}
fn default_groups_claim() -> String {
    "groups".into()
}
fn default_oidc_role() -> String {
    "viewer".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    pub name: String,
    pub url: String,
    pub username: Option<String>,
    pub password_file: Option<PathBuf>,
    pub ssh_destination: Option<String>,
    pub ssh_port: Option<i64>,
    #[serde(default = "default_interval")]
    pub interval: i64,
    #[serde(default = "default_true")]
    pub tls_skip_verify: bool,
}

fn default_interval() -> i64 {
    30
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnsureUser {
    pub username: Option<String>,
    /// Alternative to `username`, e.g. when the name is kept in a secret store.
    pub username_file: Option<PathBuf>,
    #[serde(default = "default_role")]
    pub role: String,
    pub password_file: PathBuf,
    pub email: Option<String>,
    /// Alternative to `email`, e.g. when the address is kept in a secret store.
    pub email_file: Option<PathBuf>,
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
            ssh_binary: "ssh".into(),
            ssh_identity_file: None,
            ssh_known_hosts_file: None,
            targets: Vec::new(),
            oidc: None,
            disable_password_login: false,
            allow_exec_channels: false,
        }
    }
}

impl Config {
    pub fn known_hosts_file(&self) -> PathBuf {
        self.ssh_known_hosts_file.clone().unwrap_or_else(|| {
            self.database
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.join("known_hosts"))
                .unwrap_or_else(|| PathBuf::from("known_hosts"))
        })
    }

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
    /// Event kinds (see `GET /api/events/kinds`) that never trigger notifications.
    pub disabled_events: Vec<String>,
    /// How long notifications for a channel are collected into one message
    /// (minutes); 0 sends each event immediately. Channels can override it.
    pub group_minutes: u32,
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
            disabled_events: Vec::new(),
            group_minutes: 10,
        }
    }
}
