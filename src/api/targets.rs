use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::Value;

use super::{ApiError, ApiResult};
use crate::{
    auth::{Role, User},
    pull,
    state::{SharedState, now},
    views::{self, TargetRow},
};

const MASK: &str = "********";

#[derive(Deserialize, Default)]
pub struct SshBody {
    destination: String,
    port: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct TargetBody {
    id: Option<i64>,
    name: Option<String>,
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    // Missing = unchanged, null = direct connection.
    #[serde(deserialize_with = "double_option")]
    ssh: Option<Option<SshBody>>,
    interval: Option<i64>,
    tls_skip_verify: Option<bool>,
    enabled: Option<bool>,
}

fn double_option<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<SshBody>>, D::Error> {
    Ok(Some(Option::<SshBody>::deserialize(d)?))
}

fn apply(row: &mut TargetRow, b: TargetBody) -> ApiResult<()> {
    if let Some(n) = b.name.map(|n| n.trim().to_owned()) {
        if n.is_empty() {
            return Err(ApiError::bad_request("name must not be empty"));
        }
        row.name = n;
    }
    if let Some(u) = b.url.map(|u| u.trim().trim_end_matches('/').to_owned()) {
        let parsed = reqwest::Url::parse(&u).map_err(|_| ApiError::bad_request("invalid URL"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ApiError::bad_request(
                "URL must start with http:// or https://",
            ));
        }
        row.url = u;
    }
    if let Some(u) = b.username {
        row.username = Some(u.trim().to_owned()).filter(|u| !u.is_empty());
    }
    if let Some(p) = b.password
        && p != MASK
    {
        row.password = Some(p).filter(|p| !p.is_empty());
    }
    if let Some(ssh) = b.ssh {
        match ssh {
            Some(s) if !s.destination.trim().is_empty() => {
                if s.destination.trim().starts_with('-') {
                    return Err(ApiError::bad_request("invalid SSH destination"));
                }
                row.ssh_destination = Some(s.destination.trim().to_owned());
                row.ssh_port = s.port;
            }
            _ => {
                row.ssh_destination = None;
                row.ssh_port = None;
            }
        }
    }
    if let Some(i) = b.interval {
        row.interval = i.clamp(5, 86400);
    }
    if let Some(v) = b.tls_skip_verify {
        row.tls_skip_verify = v as i64;
    }
    if let Some(v) = b.enabled {
        row.enabled = v as i64;
    }
    if row.ssh_destination.is_some() && row.url.starts_with("https://") {
        return Err(ApiError::bad_request(
            "use an http:// URL with SSH (the tunnel already encrypts)",
        ));
    }
    Ok(())
}

fn blank() -> TargetRow {
    TargetRow {
        id: 0,
        name: String::new(),
        url: String::new(),
        username: None,
        password: None,
        ssh_destination: None,
        ssh_port: None,
        interval: 30,
        tls_skip_verify: 1,
        enabled: 1,
        managed: 0,
        host_id: None,
        last_status: None,
        last_polled_at: None,
        created_at: now(),
    }
}

async fn load(state: &SharedState, id: i64) -> ApiResult<TargetRow> {
    views::fetch_target(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("connection"))
}

pub async fn list(State(state): State<SharedState>, user: User) -> ApiResult<Json<Vec<Value>>> {
    user.require(Role::Admin)?;
    let rows: Vec<TargetRow> = sqlx::query_as("SELECT * FROM targets ORDER BY name COLLATE NOCASE")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(TargetRow::json).collect()))
}

async fn save(state: &SharedState, row: &TargetRow) -> ApiResult<i64> {
    let res = if row.id == 0 {
        sqlx::query(
            "INSERT INTO targets (name, url, username, password, ssh_destination, ssh_port, interval,
                tls_skip_verify, enabled, managed, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(&row.name)
        .bind(&row.url)
        .bind(&row.username)
        .bind(&row.password)
        .bind(&row.ssh_destination)
        .bind(row.ssh_port)
        .bind(row.interval)
        .bind(row.tls_skip_verify)
        .bind(row.enabled)
        .bind(row.created_at)
        .execute(&state.db)
        .await
        .map(|r| r.last_insert_rowid())
    } else {
        sqlx::query(
            "UPDATE targets SET name = ?, url = ?, username = ?, password = ?, ssh_destination = ?,
                ssh_port = ?, interval = ?, tls_skip_verify = ?, enabled = ?, last_polled_at = NULL WHERE id = ?",
        )
        .bind(&row.name)
        .bind(&row.url)
        .bind(&row.username)
        .bind(&row.password)
        .bind(&row.ssh_destination)
        .bind(row.ssh_port)
        .bind(row.interval)
        .bind(row.tls_skip_verify)
        .bind(row.enabled)
        .bind(row.id)
        .execute(&state.db)
        .await
        .map(|_| row.id)
    };
    match res {
        Ok(id) => Ok(id),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(ApiError::new(
            StatusCode::CONFLICT,
            "a connection with that name already exists",
        )),
        Err(e) => Err(e.into()),
    }
}

pub async fn create(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<TargetBody>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    if b.name.as_deref().is_none_or(|n| n.trim().is_empty()) || b.url.is_none() {
        return Err(ApiError::bad_request("name and url are required"));
    }
    let mut row = blank();
    apply(&mut row, b)?;
    let id = save(&state, &row).await?;
    let row = load(&state, id).await?;
    tracing::info!(target = %row.name, by = %user.username, "connection added");
    // First poll right away; the result shows up via the live stream.
    let s = state.clone();
    let r = row.clone();
    tokio::spawn(async move {
        let _ = pull::poll(&s, &r).await;
    });
    Ok(Json(row.json()))
}

pub async fn update(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
    Json(b): Json<TargetBody>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    let mut row = load(&state, id).await?;
    if row.managed != 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "this connection is managed by the configuration file",
        ));
    }
    apply(&mut row, b)?;
    save(&state, &row).await?;
    Ok(Json(load(&state, id).await?.json()))
}

pub async fn remove(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    let row = load(&state, id).await?;
    if row.managed != 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "this connection is managed by the configuration file",
        ));
    }
    sqlx::query("DELETE FROM targets WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test(
    State(state): State<SharedState>,
    user: User,
    Json(b): Json<TargetBody>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    let mut row = match b.id {
        Some(id) => load(&state, id).await?,
        None => blank(),
    };
    apply(&mut row, b)?;
    if row.url.is_empty() {
        return Err(ApiError::bad_request("url is required"));
    }
    Ok(Json(pull::test(&state, &row).await))
}

pub async fn poll_now(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Operator)?;
    let row = load(&state, id).await?;
    let _ = pull::poll(&state, &row).await;
    Ok(Json(load(&state, id).await?.json()))
}
