use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, ApiResult, auth::validate_parts};
use crate::{
    auth::{self, Role, User},
    config::Retention,
    db,
    notify::{self, ChannelRow, Filter},
    state::{SharedState, now},
    views::EventRow,
};

// ---------------------------------------------------------------- users

const USER_COLS: &str = "SELECT id, username, role, created_at, last_login, auth_source, (oidc_subject IS NOT NULL) AS sso, email FROM users";

pub async fn users(State(state): State<SharedState>, user: User) -> ApiResult<Json<Vec<User>>> {
    user.require(Role::Admin)?;
    Ok(Json(
        sqlx::query_as(&format!("{USER_COLS} ORDER BY username"))
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Deserialize)]
pub struct NewUser {
    username: String,
    password: String,
    role: String,
    email: Option<String>,
}

pub async fn create_user(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<NewUser>,
) -> ApiResult<Json<User>> {
    user.require(Role::Admin)?;
    validate_parts(&b.username, &b.password)?;
    let role = Role::parse(&b.role).ok_or_else(|| ApiError::bad_request("invalid role"))?;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
        .bind(b.username.trim())
        .fetch_optional(&state.db)
        .await?;
    if exists.is_some() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "username already taken",
        ));
    }
    let email = normalize_email(b.email.as_deref())?;
    let created = auth::create_user(&state.db, b.username.trim(), &b.password, role).await?;
    if let Some(e) = &email {
        sqlx::query("UPDATE users SET email = ? WHERE id = ?")
            .bind(e)
            .bind(created.id)
            .execute(&state.db)
            .await?;
    }
    Ok(Json(fetch_user(&state, created.id).await?))
}

#[derive(Deserialize)]
pub struct UserPatch {
    username: Option<String>,
    /// An empty string clears the address.
    email: Option<String>,
    password: Option<String>,
    role: Option<String>,
    current_password: Option<String>,
}

async fn set_password(state: &SharedState, id: i64, password: &str) -> ApiResult<()> {
    if password.len() < 8 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }
    let hash = {
        let p = password.to_owned();
        tokio::task::spawn_blocking(move || auth::hash_password(&p))
            .await
            .map_err(anyhow::Error::from)??
    };
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id)
        .execute(&state.db)
        .await?;
    state.collector_auth.lock().unwrap().clear();
    Ok(())
}

async fn admin_count(state: &SharedState) -> ApiResult<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&state.db)
        .await?;
    Ok(n)
}

async fn fetch_user(state: &SharedState, id: i64) -> ApiResult<User> {
    sqlx::query_as(&format!("{USER_COLS} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("user"))
}

/// `None`/empty clears; otherwise a plausible address.
fn normalize_email(email: Option<&str>) -> ApiResult<Option<String>> {
    let e = email.map(str::trim).unwrap_or("");
    if e.is_empty() {
        return Ok(None);
    }
    let ok = e.len() <= 254
        && e.split_once('@')
            .is_some_and(|(l, d)| !l.is_empty() && d.contains('.') && !d.starts_with('.'))
        && !e.contains(char::is_whitespace);
    if !ok {
        return Err(ApiError::bad_request("invalid email address"));
    }
    Ok(Some(e.to_owned()))
}

async fn set_email(state: &SharedState, target: &User, email: &str) -> ApiResult<()> {
    if target.sso {
        return Err(ApiError::bad_request(
            "the email address of a single sign-on account comes from the identity provider",
        ));
    }
    sqlx::query("UPDATE users SET email = ? WHERE id = ?")
        .bind(normalize_email(Some(email))?)
        .bind(target.id)
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn rename_user(state: &SharedState, target: &User, new_name: &str) -> ApiResult<()> {
    let name = new_name.trim();
    if name.is_empty() || name.contains(':') || name.len() > 64 {
        return Err(ApiError::bad_request(
            "username must be 1-64 characters and must not contain ':'",
        ));
    }
    if target.sso {
        return Err(ApiError::bad_request(
            "the username of a single sign-on account comes from the identity provider",
        ));
    }
    if name == target.username {
        return Ok(());
    }
    let taken: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = ? AND id != ?")
            .bind(name)
            .bind(target.id)
            .fetch_optional(&state.db)
            .await?;
    if taken.is_some() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "username already taken",
        ));
    }
    sqlx::query("UPDATE users SET username = ? WHERE id = ?")
        .bind(name)
        .bind(target.id)
        .execute(&state.db)
        .await?;
    // Collector credentials are cached by username.
    state.collector_auth.lock().unwrap().clear();
    Ok(())
}

pub async fn update_user(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
    Json(b): Json<UserPatch>,
) -> ApiResult<Json<User>> {
    user.require(Role::Admin)?;
    let target = fetch_user(&state, id).await?;
    if let Some(r) = &b.role {
        let role = Role::parse(r).ok_or_else(|| ApiError::bad_request("invalid role"))?;
        if target.role() == Role::Admin && role != Role::Admin && admin_count(&state).await? <= 1 {
            return Err(ApiError::bad_request("cannot demote the last admin"));
        }
        sqlx::query("UPDATE users SET role = ? WHERE id = ?")
            .bind(r)
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    if let Some(n) = &b.username {
        rename_user(&state, &target, n).await?;
    }
    if let Some(e) = &b.email {
        set_email(&state, &target, e).await?;
    }
    if let Some(p) = &b.password {
        set_password(&state, id, p).await?;
    }
    Ok(Json(fetch_user(&state, id).await?))
}

pub async fn update_me(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<UserPatch>,
) -> ApiResult<Json<User>> {
    if b.password.is_none() && b.username.is_none() && b.email.is_none() {
        return Err(ApiError::bad_request("nothing to change"));
    }
    if let Some(p) = &b.password {
        if user.auth_source == "oidc" {
            return Err(ApiError::bad_request(
                "single sign-on accounts have no local password",
            ));
        }
        let current = b.current_password.as_deref().unwrap_or("");
        if auth::check_credentials(&state.db, &user.username, current)
            .await?
            .is_none()
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "current password is wrong",
            ));
        }
        if p.len() < 8 {
            return Err(ApiError::bad_request(
                "password must be at least 8 characters",
            ));
        }
    }
    if let Some(n) = &b.username {
        rename_user(&state, &user, n).await?;
    }
    if let Some(e) = &b.email {
        set_email(&state, &user, e).await?;
    }
    if let Some(p) = &b.password {
        set_password(&state, user.id, p).await?;
    }
    Ok(Json(fetch_user(&state, user.id).await?))
}

pub async fn delete_user(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    if id == user.id {
        return Err(ApiError::bad_request("you cannot delete yourself"));
    }
    let target = fetch_user(&state, id).await?;
    if target.role() == Role::Admin && admin_count(&state).await? <= 1 {
        return Err(ApiError::bad_request("cannot delete the last admin"));
    }
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    state.collector_auth.lock().unwrap().clear();
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- channels

const MASK: &str = "********";

fn channel_json(c: &ChannelRow) -> Value {
    let mut config = c.config_map();
    for k in notify::SECRET_KEYS {
        if let Some(v) = config.get_mut(k)
            && !v.is_empty()
        {
            *v = MASK.to_owned();
        }
    }
    json!({
        "id": c.id, "name": c.name, "kind": c.kind, "enabled": c.enabled != 0,
        "default": c.is_default != 0,
        "config": config, "filter": c.filter(), "last_status": c.last_status,
        "last_sent_at": c.last_sent_at, "created_at": c.created_at,
    })
}

async fn fetch_channel(state: &SharedState, id: i64) -> ApiResult<ChannelRow> {
    sqlx::query_as("SELECT * FROM channels WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::not_found("channel"))
}

pub async fn channels(State(state): State<SharedState>, user: User) -> ApiResult<Json<Vec<Value>>> {
    user.require(Role::Admin)?;
    let rows: Vec<ChannelRow> = sqlx::query_as("SELECT * FROM channels ORDER BY name")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(channel_json).collect()))
}

#[derive(Deserialize)]
pub struct ChannelBody {
    name: Option<String>,
    kind: Option<String>,
    enabled: Option<bool>,
    /// Make this the (single) default channel.
    default: Option<bool>,
    config: Option<HashMap<String, String>>,
    filter: Option<Filter>,
}

/// There is at most one default channel: making one the default clears the others.
async fn set_default(state: &SharedState, id: i64, default: bool) -> ApiResult<()> {
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    if default {
        sqlx::query("UPDATE channels SET is_default = 0 WHERE id != ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE channels SET is_default = ? WHERE id = ?")
        .bind(default as i64)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn validate_event_kinds(kinds: &[String]) -> ApiResult<()> {
    for k in kinds {
        if !crate::monit::model::EVENTS.iter().any(|e| e.1 == k) {
            return Err(ApiError::bad_request(format!("unknown event kind {k}")));
        }
    }
    Ok(())
}

fn validate_kind(state: &SharedState, kind: &str) -> ApiResult<()> {
    if !notify::KINDS.contains(&kind) {
        return Err(ApiError::bad_request("invalid kind"));
    }
    if kind == "exec" && !state.config.allow_exec_channels {
        return Err(ApiError::bad_request(
            "exec channels are disabled; the server must set allow_exec_channels",
        ));
    }
    Ok(())
}

fn validate_filter(f: &Filter) -> ApiResult<()> {
    validate_event_kinds(f.events.as_deref().unwrap_or_default())?;
    for re in [&f.hosts, &f.services].into_iter().flatten() {
        if !re.trim().is_empty() {
            regex::Regex::new(re)
                .map_err(|e| ApiError::bad_request(format!("invalid regex: {e}")))?;
        }
    }
    Ok(())
}

pub async fn create_channel(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<ChannelBody>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    let name = b
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::bad_request("name required"))?;
    let kind = b
        .kind
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("invalid kind"))?;
    validate_kind(&state, kind)?;
    let filter = b.filter.unwrap_or_default();
    validate_filter(&filter)?;
    let id = sqlx::query("INSERT INTO channels (name, kind, config, filter, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(name)
        .bind(kind)
        .bind(serde_json::to_string(&b.config.unwrap_or_default()).map_err(anyhow::Error::from)?)
        .bind(serde_json::to_string(&filter).map_err(anyhow::Error::from)?)
        .bind(b.enabled.unwrap_or(true) as i64)
        .bind(now())
        .execute(&state.db)
        .await?
        .last_insert_rowid();
    if let Some(d) = b.default {
        set_default(&state, id, d).await?;
    }
    Ok(Json(channel_json(&fetch_channel(&state, id).await?)))
}

pub async fn update_channel(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
    Json(b): Json<ChannelBody>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    let mut c = fetch_channel(&state, id).await?;
    if let Some(n) = b.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        c.name = n.to_owned();
    }
    if let Some(k) = b.kind {
        validate_kind(&state, &k)?;
        c.kind = k;
    }
    if let Some(e) = b.enabled {
        c.enabled = e as i64;
    }
    if let Some(mut config) = b.config {
        // Masked secrets mean "unchanged".
        let old = c.config_map();
        for (k, v) in config.iter_mut() {
            if v == MASK
                && let Some(prev) = old.get(k)
            {
                *v = prev.clone();
            }
        }
        c.config = serde_json::to_string(&config).map_err(anyhow::Error::from)?;
    }
    if let Some(f) = b.filter {
        validate_filter(&f)?;
        c.filter = serde_json::to_string(&f).map_err(anyhow::Error::from)?;
    }
    sqlx::query(
        "UPDATE channels SET name = ?, kind = ?, config = ?, filter = ?, enabled = ? WHERE id = ?",
    )
    .bind(&c.name)
    .bind(&c.kind)
    .bind(&c.config)
    .bind(&c.filter)
    .bind(c.enabled)
    .bind(id)
    .execute(&state.db)
    .await?;
    if let Some(d) = b.default {
        set_default(&state, id, d).await?;
    }
    Ok(Json(channel_json(&fetch_channel(&state, id).await?)))
}

pub async fn delete_channel(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test_channel(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    let c = fetch_channel(&state, id).await?;
    // Use a real host and check when there is one, so email previews show the real design.
    let sample: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT h.id, COALESCE(h.display_name, h.hostname), s.name FROM hosts h
         JOIN services s ON s.host_id = h.id ORDER BY h.id, s.type DESC, s.name LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?;
    let (host_id, host, service) = match sample {
        Some((id, host, service)) => (Some(id), host, service),
        None => (None, "monarch".to_owned(), "test".to_owned()),
    };
    let e = EventRow {
        id: 0,
        host_id,
        host: Some(host),
        service: Some(service),
        service_type: None,
        event_type: 0x200000,
        state: 1,
        action: 1,
        message: format!(
            "This is a test notification sent by {} from Monarch.",
            user.username
        ),
        created_at: crate::state::now_f(),
        source: "monarch".into(),
        acked_by: None,
        acked_at: None,
    };
    let public_url = state.settings.read().await.public_url.clone();
    let n = notify::render(&public_url, &e);
    let (ok, message) = match notify::send(&state, &c, &n).await {
        Ok(()) => (true, "Test notification sent".to_owned()),
        Err(e) => (false, format!("{e:#}")),
    };
    sqlx::query("UPDATE channels SET last_status = ?, last_sent_at = ? WHERE id = ?")
        .bind(if ok {
            "ok".to_owned()
        } else {
            format!("error: {message}")
        })
        .bind(now())
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({"ok": ok, "message": message})))
}

// ---------------------------------------------------------------- settings

async fn settings_json(state: &SharedState) -> Value {
    let s = state.settings.read().await;
    json!({
        "public_url": s.public_url,
        "retention": s.retention,
        "heartbeat_grace": s.heartbeat_grace,
        "disabled_events": s.disabled_events,
        "group_minutes": s.group_minutes,
        "collector_url": format!("{}/collector", s.public_url.trim_end_matches('/')),
    })
}

pub async fn settings(State(state): State<SharedState>, user: User) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    Ok(Json(settings_json(&state).await))
}

#[derive(Deserialize)]
pub struct SettingsPatch {
    public_url: Option<String>,
    retention: Option<Retention>,
    heartbeat_grace: Option<f64>,
    disabled_events: Option<Vec<String>>,
    group_minutes: Option<u32>,
}

pub async fn update_settings(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<SettingsPatch>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    {
        let mut s = state.settings.write().await;
        if let Some(u) = b.public_url {
            let u = u.trim().trim_end_matches('/').to_owned();
            if !(u.starts_with("http://") || u.starts_with("https://")) {
                return Err(ApiError::bad_request(
                    "public_url must start with http:// or https://",
                ));
            }
            s.public_url = u;
        }
        if let Some(r) = b.retention {
            if r.raw_hours < 1 || r.rollup_5m_days < 1 || r.rollup_1h_days < 1 || r.events_days < 1
            {
                return Err(ApiError::bad_request("retention values must be positive"));
            }
            s.retention = r;
        }
        if let Some(g) = b.heartbeat_grace {
            if !(1.0..=100.0).contains(&g) {
                return Err(ApiError::bad_request(
                    "heartbeat_grace must be between 1 and 100",
                ));
            }
            s.heartbeat_grace = g;
        }
        if let Some(m) = b.group_minutes {
            if m > 1440 {
                return Err(ApiError::bad_request(
                    "group_minutes must be between 0 and 1440",
                ));
            }
            s.group_minutes = m;
        }
        if let Some(mut kinds) = b.disabled_events {
            validate_event_kinds(&kinds)?;
            kinds.sort();
            kinds.dedup();
            s.disabled_events = kinds;
        }
        db::save_settings(&state.db, &s).await?;
    }
    Ok(Json(settings_json(&state).await))
}
