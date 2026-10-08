//! Browser notifications: Web Push (RFC 8030) with VAPID (RFC 8292) and
//! message encryption (RFC 8291), via the pure-Rust `web-push-native` crate.

use anyhow::{Context, Result, bail};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use web_push_native::{
    Auth, WebPushBuilder,
    jwt_simple::algorithms::{ECDSAP256PublicKeyLike, ES256KeyPair},
    p256::PublicKey,
};

use crate::{
    api::{ApiError, ApiResult},
    auth::User,
    notify::Notification,
    state::{SharedState, now},
};

const VAPID_KEY: &str = "vapid_private_key";

#[derive(Debug, FromRow)]
struct Subscription {
    id: i64,
    endpoint: String,
    p256dh: String,
    auth: String,
}

/// The server's VAPID key pair; generated on first use and kept in the database.
async fn vapid(state: &SharedState) -> Result<ES256KeyPair> {
    let load = || async {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(VAPID_KEY)
            .fetch_optional(&state.db)
            .await?;
        anyhow::Ok(row)
    };
    if let Some((v,)) = load().await? {
        return ES256KeyPair::from_bytes(&URL_SAFE_NO_PAD.decode(v)?).context("stored VAPID key");
    }
    let kp = ES256KeyPair::generate();
    sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
        .bind(VAPID_KEY)
        .bind(URL_SAFE_NO_PAD.encode(kp.to_bytes()))
        .execute(&state.db)
        .await?;
    // A concurrent request may have won the insert; use whatever is stored.
    let (v,) = load().await?.context("VAPID key missing after insert")?;
    ES256KeyPair::from_bytes(&URL_SAFE_NO_PAD.decode(v)?).context("stored VAPID key")
}

fn public_key(kp: &ES256KeyPair) -> String {
    URL_SAFE_NO_PAD.encode(kp.public_key().public_key().to_bytes_uncompressed())
}

async fn push_one(
    state: &SharedState,
    kp: &ES256KeyPair,
    contact: &str,
    sub: &Subscription,
    payload: &[u8],
) -> Result<()> {
    let p256dh = URL_SAFE_NO_PAD.decode(&sub.p256dh)?;
    let auth = URL_SAFE_NO_PAD.decode(&sub.auth)?;
    if auth.len() != 16 {
        bail!("subscription has an invalid auth secret");
    }
    let request = WebPushBuilder::new(
        sub.endpoint.parse().context("invalid push endpoint")?,
        PublicKey::from_sec1_bytes(&p256dh).context("invalid p256dh key")?,
        Auth::clone_from_slice(&auth),
    )
    .with_vapid(kp, contact)
    .build(payload.to_vec())
    .context("encrypting push message")?;
    let (parts, body) = request.into_parts();
    let mut req = state.http.post(parts.uri.to_string()).body(body);
    for (name, value) in &parts.headers {
        req = req.header(name.as_str(), value.as_bytes());
    }
    let res = req.send().await?;
    let status = res.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        // The browser dropped this subscription.
        sqlx::query("DELETE FROM push_subscriptions WHERE id = ?")
            .bind(sub.id)
            .execute(&state.db)
            .await?;
        bail!("subscription expired (HTTP {status}), removed");
    }
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        bail!(
            "HTTP {status}: {}",
            text.chars().take(200).collect::<String>()
        );
    }
    sqlx::query("UPDATE push_subscriptions SET last_ok = ? WHERE id = ?")
        .bind(now())
        .bind(sub.id)
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn push_all(
    state: &SharedState,
    subs: Vec<Subscription>,
    payload: Value,
) -> Result<(usize, Vec<String>)> {
    let kp = vapid(state).await?;
    let contact = state.settings.read().await.public_url.clone();
    let payload = serde_json::to_vec(&payload)?;
    let mut ok = 0;
    let mut errors = Vec::new();
    for sub in &subs {
        match push_one(state, &kp, &contact, sub, &payload).await {
            Ok(()) => ok += 1,
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    Ok((ok, errors))
}

/// Deliver a notification to all subscribed browsers (optionally only those of
/// the listed users). Used by the "webpush" channel kind.
pub async fn send_channel(
    state: &SharedState,
    n: &Notification,
    users: Option<&str>,
) -> Result<()> {
    let wanted: Vec<String> = users
        .unwrap_or("")
        .split(',')
        .map(|u| u.trim().to_lowercase())
        .filter(|u| !u.is_empty())
        .collect();
    let subs: Vec<Subscription> = sqlx::query_as(
        "SELECT s.id, s.endpoint, s.p256dh, s.auth FROM push_subscriptions s
         JOIN users u ON u.id = s.user_id WHERE (? = 0 OR lower(u.username) IN (SELECT value FROM json_each(?)))",
    )
    .bind(!wanted.is_empty() as i64)
    .bind(serde_json::to_string(&wanted)?)
    .fetch_all(&state.db)
    .await?;
    if subs.is_empty() {
        bail!("no browser is subscribed");
    }
    let payload = json!({"title": n.title, "text": n.text, "url": n.url, "failed": n.failed});
    let (ok, errors) = push_all(state, subs, payload).await?;
    if ok == 0 {
        bail!("{}", errors.join("; "));
    }
    Ok(())
}

// ---------------------------------------------------------------- API

pub async fn key(State(state): State<SharedState>, _user: User) -> ApiResult<Json<Value>> {
    let kp = vapid(&state).await?;
    Ok(Json(json!({"public_key": public_key(&kp)})))
}

#[derive(Serialize, FromRow)]
pub struct SubscriptionInfo {
    id: i64,
    endpoint: String,
    user_agent: Option<String>,
    created_at: i64,
    last_ok: Option<i64>,
}

pub async fn list(
    State(state): State<SharedState>,
    user: User,
) -> ApiResult<Json<Vec<SubscriptionInfo>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT id, endpoint, user_agent, created_at, last_ok FROM push_subscriptions
             WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(user.id)
        .fetch_all(&state.db)
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct Keys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
pub struct NewSubscription {
    endpoint: String,
    keys: Keys,
    user_agent: Option<String>,
}

pub async fn subscribe(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<NewSubscription>,
) -> ApiResult<Json<Value>> {
    if !b.endpoint.starts_with("https://") {
        return Err(ApiError::bad_request("endpoint must be an https URL"));
    }
    let valid = URL_SAFE_NO_PAD
        .decode(&b.keys.p256dh)
        .is_ok_and(|k| PublicKey::from_sec1_bytes(&k).is_ok())
        && URL_SAFE_NO_PAD
            .decode(&b.keys.auth)
            .is_ok_and(|a| a.len() == 16);
    if !valid {
        return Err(ApiError::bad_request("invalid subscription keys"));
    }
    // The same browser re-subscribing (possibly as another user) replaces the row.
    let id = sqlx::query(
        "INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth, user_agent, created_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (endpoint) DO UPDATE SET user_id = excluded.user_id, p256dh = excluded.p256dh,
           auth = excluded.auth, user_agent = excluded.user_agent",
    )
    .bind(user.id)
    .bind(&b.endpoint)
    .bind(&b.keys.p256dh)
    .bind(&b.keys.auth)
    .bind(
        b.user_agent
            .as_deref()
            .map(|s| s.chars().take(200).collect::<String>()),
    )
    .bind(now())
    .execute(&state.db)
    .await?
    .last_insert_rowid();
    Ok(Json(json!({"id": id})))
}

pub async fn unsubscribe(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let n = sqlx::query("DELETE FROM push_subscriptions WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found("subscription"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Send a test notification to the caller's own browsers.
pub async fn test(State(state): State<SharedState>, user: User) -> ApiResult<Json<Value>> {
    let subs: Vec<Subscription> = sqlx::query_as(
        "SELECT id, endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    if subs.is_empty() {
        return Err(ApiError::bad_request(
            "this account has no subscribed browser",
        ));
    }
    let url = state.settings.read().await.public_url.clone();
    let payload = json!({
        "title": "Monarch test notification",
        "text": "Browser notifications work.",
        "url": url,
        "failed": false,
    });
    let (ok, errors) = push_all(&state, subs, payload).await?;
    Ok(Json(json!({"ok": ok > 0, "sent": ok, "errors": errors})))
}
