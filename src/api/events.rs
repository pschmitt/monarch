use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, ApiResult};
use crate::{
    auth::{Role, User},
    monit::model,
    state::{SharedState, now},
    views::{self, EventRow},
};

#[derive(Deserialize)]
pub struct EventQuery {
    host: Option<i64>,
    service: Option<String>,
    state: Option<String>,
    q: Option<String>,
    unacked: Option<String>,
    limit: Option<i64>,
    before: Option<i64>,
}

pub async fn list(
    State(state): State<SharedState>,
    _user: User,
    Query(q): Query<EventQuery>,
) -> ApiResult<Json<Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let mut sql = format!("{} WHERE 1 = 1", views::EVENT_SELECT);
    let mut binds: Vec<Value> = Vec::new();
    if let Some(h) = q.host {
        sql.push_str(" AND e.host_id = ?");
        binds.push(json!(h));
    }
    if let Some(s) = q.service.as_deref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND e.service = ?");
        binds.push(json!(s));
    }
    if let Some(st) = q.state.as_deref().filter(|s| !s.is_empty()) {
        let v = model::event_state_from_name(st)
            .ok_or_else(|| ApiError::bad_request("invalid state"))?;
        if v == 3 {
            sql.push_str(" AND e.state IN (3, 4)");
        } else {
            sql.push_str(" AND e.state = ?");
            binds.push(json!(v));
        }
    }
    if let Some(text) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(" AND (e.message LIKE ? OR e.service LIKE ? OR h.hostname LIKE ? OR h.display_name LIKE ?)");
        let like = format!("%{text}%");
        for _ in 0..4 {
            binds.push(json!(like));
        }
    }
    if matches!(q.unacked.as_deref(), Some("1" | "true")) {
        sql.push_str(" AND e.acked_at IS NULL AND e.state = 1");
    }
    if let Some(b) = q.before {
        sql.push_str(" AND e.id < ?");
        binds.push(json!(b));
    }
    sql.push_str(" ORDER BY e.id DESC LIMIT ?");
    binds.push(json!(limit + 1));

    let mut query = sqlx::query_as::<_, EventRow>(&sql);
    for b in &binds {
        query = match b {
            Value::Number(n) => query.bind(n.as_i64()),
            Value::String(s) => query.bind(s.clone()),
            _ => query,
        };
    }
    let mut rows = query.fetch_all(&state.db).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    Ok(Json(json!({
        "events": rows.iter().map(views::event_json).collect::<Vec<_>>(),
        "has_more": has_more,
    })))
}

/// The event kinds that can be toggled for notifications.
pub async fn kinds(_user: User) -> Json<Value> {
    Json(Value::Array(
        crate::monit::model::EVENTS
            .iter()
            .map(|(_, kind, failed, ok)| json!({"kind": kind, "failed": failed, "succeeded": ok}))
            .collect(),
    ))
}

pub async fn ack(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Operator)?;
    sqlx::query("UPDATE events SET acked_by = ?, acked_at = ? WHERE id = ? AND acked_at IS NULL")
        .bind(&user.username)
        .bind(now())
        .bind(id)
        .execute(&state.db)
        .await?;
    let e = views::fetch_event(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("event"))?;
    Ok(Json(views::event_json(&e)))
}

#[derive(Deserialize)]
pub struct AckMany {
    ids: Vec<i64>,
}

pub async fn ack_many(
    State(state): State<SharedState>,
    user: User,
    Json(body): Json<AckMany>,
) -> ApiResult<StatusCode> {
    user.require(Role::Operator)?;
    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;
    for id in body.ids.iter().take(1000) {
        sqlx::query(
            "UPDATE events SET acked_by = ?, acked_at = ? WHERE id = ? AND acked_at IS NULL",
        )
        .bind(&user.username)
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
