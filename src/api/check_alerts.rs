//! Per-check overrides of the generic notification settings.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::FromRow;

use super::{ApiError, ApiResult};
use crate::{
    auth::{Role, User},
    state::{SharedState, now},
};

#[derive(Debug, FromRow)]
pub struct CheckAlert {
    pub host_id: i64,
    pub service: String,
    pub muted: i64,
    pub events: Option<String>,
    pub channels: Option<String>,
    pub updated_at: i64,
}

impl CheckAlert {
    /// Event kinds that notify for this check; `None` follows the global settings.
    pub fn events(&self) -> Option<Vec<String>> {
        self.events
            .as_deref()
            .and_then(|e| serde_json::from_str(e).ok())
    }
    /// Channel ids that receive this check's events; `None` uses normal routing.
    pub fn channels(&self) -> Option<Vec<i64>> {
        self.channels
            .as_deref()
            .and_then(|c| serde_json::from_str(c).ok())
    }
}

pub async fn fetch(
    state: &SharedState,
    host_id: i64,
    service: &str,
) -> anyhow::Result<Option<CheckAlert>> {
    Ok(
        sqlx::query_as("SELECT * FROM check_alerts WHERE host_id = ? AND service = ?")
            .bind(host_id)
            .bind(service)
            .fetch_optional(&state.db)
            .await?,
    )
}

fn json_of(a: &CheckAlert, host: Option<&str>) -> Value {
    json!({
        "host_id": a.host_id,
        "host": host,
        "service": a.service,
        "muted": a.muted != 0,
        "events": a.events(),
        "channels": a.channels(),
        "updated_at": a.updated_at,
    })
}

fn default_json(host_id: i64, service: &str) -> Value {
    json!({
        "host_id": host_id, "service": service, "muted": false,
        "events": null, "channels": null, "updated_at": null,
    })
}

#[derive(FromRow)]
struct ListRow {
    host_id: i64,
    service: String,
    muted: i64,
    events: Option<String>,
    channels: Option<String>,
    updated_at: i64,
    display_name: Option<String>,
    hostname: String,
}

/// Every check that has its own settings (for the settings overview).
pub async fn list(State(state): State<SharedState>, user: User) -> ApiResult<Json<Vec<Value>>> {
    user.require(Role::Admin)?;
    let rows: Vec<ListRow> = sqlx::query_as(
        "SELECT a.host_id, a.service, a.muted, a.events, a.channels, a.updated_at, h.display_name, h.hostname
         FROM check_alerts a JOIN hosts h ON h.id = a.host_id ORDER BY h.hostname, a.service",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| {
                let host = r.display_name.clone().unwrap_or_else(|| r.hostname.clone());
                json_of(
                    &CheckAlert {
                        host_id: r.host_id,
                        service: r.service,
                        muted: r.muted,
                        events: r.events,
                        channels: r.channels,
                        updated_at: r.updated_at,
                    },
                    Some(&host),
                )
            })
            .collect(),
    ))
}

pub async fn get(
    State(state): State<SharedState>,
    _user: User,
    Path((host_id, service)): Path<(i64, String)>,
) -> ApiResult<Json<Value>> {
    Ok(Json(match fetch(&state, host_id, &service).await? {
        Some(a) => json_of(&a, None),
        None => default_json(host_id, &service),
    }))
}

#[derive(Deserialize)]
pub struct Put {
    #[serde(default)]
    muted: bool,
    events: Option<Vec<String>>,
    channels: Option<Vec<i64>>,
}

pub async fn put(
    State(state): State<SharedState>,
    user: User,
    Path((host_id, service)): Path<(i64, String)>,
    Json(b): Json<Put>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    let known: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM services WHERE host_id = ? AND name = ?")
            .bind(host_id)
            .bind(&service)
            .fetch_optional(&state.db)
            .await?;
    if known.is_none() {
        return Err(ApiError::not_found("service"));
    }
    for k in b.events.iter().flatten() {
        if !crate::monit::model::EVENTS.iter().any(|e| e.1 == k) {
            return Err(ApiError::bad_request(format!("unknown event kind {k}")));
        }
    }
    for id in b.channels.iter().flatten() {
        let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM channels WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
        if exists.is_none() {
            return Err(ApiError::bad_request(format!("unknown channel {id}")));
        }
    }
    // Nothing overridden: drop the row so the overview only lists real overrides.
    if !b.muted && b.events.is_none() && b.channels.is_none() {
        sqlx::query("DELETE FROM check_alerts WHERE host_id = ? AND service = ?")
            .bind(host_id)
            .bind(&service)
            .execute(&state.db)
            .await?;
        return Ok(Json(default_json(host_id, &service)));
    }
    sqlx::query(
        "INSERT INTO check_alerts (host_id, service, muted, events, channels, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (host_id, service) DO UPDATE SET muted = excluded.muted,
           events = excluded.events, channels = excluded.channels, updated_at = excluded.updated_at",
    )
    .bind(host_id)
    .bind(&service)
    .bind(b.muted as i64)
    .bind(
        b.events
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(anyhow::Error::from)?,
    )
    .bind(
        b.channels
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(anyhow::Error::from)?,
    )
    .bind(now())
    .execute(&state.db)
    .await?;
    let a = fetch(&state, host_id, &service)
        .await?
        .ok_or_else(|| ApiError::not_found("check alert"))?;
    Ok(Json(json_of(&a, None)))
}

pub async fn delete(
    State(state): State<SharedState>,
    user: User,
    Path((host_id, service)): Path<(i64, String)>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    sqlx::query("DELETE FROM check_alerts WHERE host_id = ? AND service = ?")
        .bind(host_id)
        .bind(&service)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
