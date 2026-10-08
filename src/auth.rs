use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, header, request::Parts},
};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};

use crate::{
    api::ApiError,
    state::{SharedState, now},
};

pub const COOKIE: &str = "monarch_session";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Collector,
    Viewer,
    Operator,
    Admin,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Self::Admin),
            "operator" => Some(Self::Operator),
            "viewer" => Some(Self::Viewer),
            "collector" => Some(Self::Collector),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: i64,
    pub last_login: Option<i64>,
    /// "local" or "oidc"
    pub auth_source: String,
    /// Has a single sign-on identity (SSO-created, or a local account linked to one).
    pub sso: bool,
    pub email: Option<String>,
}

impl User {
    pub fn role(&self) -> Role {
        Role::parse(&self.role).unwrap_or(Role::Viewer)
    }

    pub fn require(&self, min: Role) -> Result<(), ApiError> {
        let r = self.role();
        if r == Role::Collector || r < min {
            Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "insufficient privileges",
            ))
        } else {
            Ok(())
        }
    }
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Verify credentials; returns the user on success.
pub async fn check_credentials(
    db: &SqlitePool,
    username: &str,
    password: &str,
) -> anyhow::Result<Option<User>> {
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(db)
            .await?;
    let Some((id, hash)) = row else {
        // Spend comparable time to not leak which usernames exist.
        static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || {
            let dummy = DUMMY.get_or_init(|| hash_password("monarch-dummy").unwrap_or_default());
            verify_password(&password, dummy)
        })
        .await?;
        return Ok(None);
    };
    let ok = {
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || verify_password(&password, &hash)).await?
    };
    if !ok {
        return Ok(None);
    }
    Ok(sqlx::query_as(
        "SELECT id, username, role, created_at, last_login, auth_source, (oidc_subject IS NOT NULL) AS sso, email FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub async fn create_session(state: &SharedState, user_id: i64) -> anyhow::Result<String> {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    let token = hex::encode(raw);
    let ts = now();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(token_hash(&token))
    .bind(user_id)
    .bind(ts)
    .bind(ts + state.config.session_days * 86400)
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE users SET last_login = ? WHERE id = ?")
        .bind(ts)
        .bind(user_id)
        .execute(&state.db)
        .await?;
    Ok(token)
}

pub async fn delete_session(db: &SqlitePool, token: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(token_hash(token))
        .execute(db)
        .await?;
    Ok(())
}

pub fn session_cookie(state: &SharedState, secure: bool, token: &str) -> String {
    format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        state.config.session_days * 86400,
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie() -> String {
    format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

/// Whether the client talks to us over https (directly or via a proxy).
pub fn is_secure(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_owned())
}

pub const API_TOKEN_PREFIX: &str = "mnr_";

/// A new API token: `mnr_` + 32 random bytes (base64url). Only its hash is stored.
pub fn generate_api_token() -> String {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    format!(
        "{API_TOKEN_PREFIX}{}",
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw)
    )
}

pub fn api_token_hash(token: &str) -> String {
    token_hash(token)
}

pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| t.starts_with(API_TOKEN_PREFIX))
}

pub async fn user_from_headers(
    state: &SharedState,
    headers: &HeaderMap,
) -> anyhow::Result<Option<User>> {
    if let Some(token) = bearer_token(headers) {
        let ts = now();
        let hash = token_hash(token);
        let user: Option<User> = sqlx::query_as(
            "SELECT u.id, u.username, u.role, u.created_at, u.last_login, u.auth_source, (u.oidc_subject IS NOT NULL) AS sso, u.email FROM api_tokens t
             JOIN users u ON u.id = t.user_id WHERE t.token_hash = ? AND (t.expires_at IS NULL OR t.expires_at > ?)",
        )
        .bind(&hash)
        .bind(ts)
        .fetch_optional(&state.db)
        .await?;
        if user.is_some() {
            sqlx::query("UPDATE api_tokens SET last_used = ? WHERE token_hash = ? AND (last_used IS NULL OR last_used < ?)")
                .bind(ts)
                .bind(&hash)
                .bind(ts - 60)
                .execute(&state.db)
                .await?;
        }
        return Ok(user);
    }
    let Some(token) = session_token(headers) else {
        return Ok(None);
    };
    Ok(sqlx::query_as(
        "SELECT u.id, u.username, u.role, u.created_at, u.last_login, u.auth_source, (u.oidc_subject IS NOT NULL) AS sso, u.email FROM sessions s
         JOIN users u ON u.id = s.user_id WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(token_hash(&token))
    .bind(now())
    .fetch_optional(&state.db)
    .await?)
}

impl FromRequestParts<SharedState> for User {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        match user_from_headers(state, &parts.headers).await? {
            Some(u) if u.role() != Role::Collector => Ok(u),
            Some(_) => Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "collector accounts cannot sign in",
            )),
            None => Err(ApiError::new(StatusCode::UNAUTHORIZED, "not signed in")),
        }
    }
}

pub async fn user_count(db: &SqlitePool) -> anyhow::Result<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await?;
    Ok(n)
}

pub async fn create_user(
    db: &SqlitePool,
    username: &str,
    password: &str,
    role: Role,
) -> anyhow::Result<User> {
    let hash = {
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || hash_password(&password)).await??
    };
    let role = match role {
        Role::Admin => "admin",
        Role::Operator => "operator",
        Role::Viewer => "viewer",
        Role::Collector => "collector",
    };
    let id = sqlx::query(
        "INSERT INTO users (username, password_hash, role, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(username)
    .bind(hash)
    .bind(role)
    .bind(now())
    .execute(db)
    .await?
    .last_insert_rowid();
    Ok(sqlx::query_as(
        "SELECT id, username, role, created_at, last_login, auth_source, (oidc_subject IS NOT NULL) AS sso, email FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await?)
}
