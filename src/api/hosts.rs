use std::cmp::Ordering;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{ApiError, ApiResult};
use crate::{
    auth::{Role, User},
    collector,
    monit::{client, model},
    state::{SharedState, now},
    views::{self, EventRow, HostRow},
};

pub async fn list(State(state): State<SharedState>, _user: User) -> ApiResult<Json<Vec<Value>>> {
    let mut out = Vec::new();
    for (h, services) in views::all_hosts(&state.db).await? {
        out.push(views::host_summary(&state.db, &h, &services).await?);
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct ServicesQuery {
    state: Option<String>,
    q: Option<String>,
    host: Option<i64>,
}

/// Every service of every host (the Services page), worst first.
pub async fn services_list(
    State(state): State<SharedState>,
    _user: User,
    Query(q): Query<ServicesQuery>,
) -> ApiResult<Json<Value>> {
    let needle =
        q.q.as_deref()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
    let wanted = q.state.as_deref().filter(|s| !s.is_empty());
    let rank = |s: &str| match s {
        "failed" => 0,
        "pending" => 1,
        "init" => 2,
        "unmonitored" => 3,
        _ => 4,
    };
    let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut rows: Vec<(i64, String, String, Value)> = Vec::new();
    for (h, services) in views::all_hosts(&state.db).await? {
        if q.host.is_some_and(|id| id != h.id) {
            continue;
        }
        let host_name = h.name().to_owned();
        let host_state = views::host_state(&h, &services);
        for s in &services {
            if let Some(n) = &needle
                && !s.name.to_lowercase().contains(n)
                && !host_name.to_lowercase().contains(n)
            {
                continue;
            }
            *counts.entry(s.state.clone()).or_default() += 1;
            if wanted.is_some_and(|w| w != s.state) {
                continue;
            }
            rows.push((
                rank(&s.state),
                host_name.to_lowercase(),
                s.name.to_lowercase(),
                json!({
                    "host_id": h.id,
                    "host": host_name,
                    "host_state": host_state,
                    "name": s.name,
                    "type": model::service_type_name(s.type_id),
                    "type_id": s.type_id,
                    "state": s.state,
                    "status_text": model::status_text(s.type_id, s.status, s.monitor, s.pending_action),
                    "state_since": s.state_since,
                    "pending_action": if s.pending_action != 0 { model::action_name(s.pending_action) } else { None },
                }),
            ));
        }
    }
    rows.sort_by(|a, b| (a.0, &a.1, &a.2).cmp(&(b.0, &b.1, &b.2)));
    Ok(Json(json!({
        "services": rows.into_iter().map(|r| r.3).collect::<Vec<_>>(),
        "counts": counts,
    })))
}

fn f(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn desc(a: Option<f64>, b: Option<f64>) -> Ordering {
    b.unwrap_or(f64::MIN)
        .partial_cmp(&a.unwrap_or(f64::MIN))
        .unwrap_or(Ordering::Equal)
}

pub async fn overview(State(state): State<SharedState>, _user: User) -> ApiResult<Json<Value>> {
    let hosts = views::all_hosts(&state.db).await?;
    let (mut online, mut offline, mut degraded) = (0, 0, 0);
    let (mut total, mut ok, mut failed, mut unmonitored, mut pending) = (0, 0, 0, 0, 0);
    let mut failing = Vec::new();
    let mut procs = Vec::new();
    let mut fss = Vec::new();
    for (h, services) in &hosts {
        match views::host_state(h, services) {
            "offline" => offline += 1,
            "degraded" => {
                online += 1;
                degraded += 1
            }
            _ => online += 1,
        }
        let c = views::counts(services);
        total += c.total;
        ok += c.ok;
        failed += c.failed;
        unmonitored += c.unmonitored;
        pending += c.pending;
        for s in services {
            let data: Value = serde_json::from_str(&s.data).unwrap_or_default();
            if s.state == "failed" {
                failing.push((
                    s.state_since,
                    json!({
                        "host_id": h.id, "host": h.name(), "service": s.name,
                        "type": model::service_type_name(s.type_id),
                        "status_text": model::status_text(s.type_id, s.status, s.monitor, s.pending_action),
                        "since": s.state_since,
                    }),
                ));
            }
            match s.type_id {
                3 if data["pid"].as_i64().unwrap_or(0) > 0 => procs.push(json!({
                    "host_id": h.id, "host": h.name(), "service": s.name,
                    "cpu": data["cpu"]["percent"], "mem_kb": data["memory"]["kb_total"].as_i64().or(data["memory"]["kb"].as_i64()),
                    "mem_percent": data["memory"]["percent_total"].as_f64().or(data["memory"]["percent"].as_f64()),
                })),
                0 if data["space"]["percent"].is_number() => fss.push(json!({
                    "host_id": h.id, "host": h.name(), "service": s.name,
                    "percent": data["space"]["percent"], "used_mb": data["space"]["used_mb"],
                    "total_mb": data["space"]["total_mb"],
                })),
                _ => {}
            }
        }
    }
    failing.sort_by_key(|(since, _)| since.unwrap_or(i64::MAX));
    let mut top_cpu = procs.clone();
    top_cpu.sort_by(|a, b| desc(f(&a["cpu"]), f(&b["cpu"])));
    top_cpu.truncate(8);
    let mut top_mem = procs;
    top_mem.sort_by(|a, b| desc(f(&a["mem_kb"]), f(&b["mem_kb"])));
    top_mem.truncate(8);
    fss.sort_by(|a, b| desc(f(&a["percent"]), f(&b["percent"])));
    fss.truncate(8);

    let ts = now();
    let since = (ts - 86400) as f64;
    let (events_24h,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE created_at >= ?")
        .bind(since)
        .fetch_one(&state.db)
        .await?;
    let recent: Vec<EventRow> = sqlx::query_as(&format!(
        "{} ORDER BY e.created_at DESC LIMIT 15",
        views::EVENT_SELECT
    ))
    .fetch_all(&state.db)
    .await?;
    let start = ts - ts.rem_euclid(3600) - 23 * 3600;
    let buckets: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT CAST(created_at AS INTEGER) / 3600 * 3600 AS b,
                SUM(CASE WHEN state = 0 THEN 1 ELSE 0 END), SUM(CASE WHEN state = 1 THEN 1 ELSE 0 END)
         FROM events WHERE created_at >= ? GROUP BY b",
    )
    .bind(start as f64)
    .fetch_all(&state.db)
    .await?;
    let activity: Vec<Value> = (0..24)
        .map(|i| {
            let b = start + i * 3600;
            let (ok, failed) = buckets
                .iter()
                .find(|(t, _, _)| *t == b)
                .map(|(_, o, f)| (*o, *f))
                .unwrap_or((0, 0));
            json!({"ts": b, "ok": ok, "failed": failed})
        })
        .collect();

    Ok(Json(json!({
        "hosts": {"total": hosts.len(), "online": online, "offline": offline, "degraded": degraded},
        "services": {"total": total, "ok": ok, "failed": failed, "unmonitored": unmonitored, "pending": pending},
        "events_24h": events_24h,
        "failing": failing.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "top_cpu": top_cpu,
        "top_mem": top_mem,
        "filesystems": fss,
        "recent_events": recent.iter().map(views::event_json).collect::<Vec<_>>(),
        "activity": activity,
    })))
}

async fn load_host(state: &SharedState, id: i64) -> ApiResult<HostRow> {
    views::fetch_host(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("host"))
}

async fn host_detail(state: &SharedState, h: &HostRow) -> ApiResult<Value> {
    let services = views::fetch_services(&state.db, h.id).await?;
    let mut v = views::host_summary(&state.db, h, &services).await?;
    let groups = h.servicegroups();
    let counts = views::event_counts(&state.db, h.id).await?;
    let extra = json!({
        "controlfile": h.controlfile,
        "incarnation": h.incarnation,
        "startdelay": h.startdelay,
        "httpd": if h.httpd_port.is_some() || h.httpd_unixsocket.is_some() {
            json!({"address": h.httpd_address, "port": h.httpd_port, "ssl": h.httpd_ssl != 0, "unixsocket": h.httpd_unixsocket})
        } else { Value::Null },
        "remote_addr": h.remote_addr,
        "monit_url": h.monit_url(),
        "override_url": h.override_url,
        "override_username": h.override_username,
        "has_override_password": h.override_password.as_deref().is_some_and(|p| !p.is_empty()),
        "has_reported_credentials": h.reported_username.is_some(),
        "tls_skip_verify": h.tls_skip_verify != 0,
        "servicegroups": groups,
        "services": services.iter()
            .map(|s| views::service_json(s, &groups, counts.get(&s.name).copied().unwrap_or(0)))
            .collect::<Vec<_>>(),
    });
    if let (Some(obj), Value::Object(extra)) = (v.as_object_mut(), extra) {
        obj.extend(extra);
    }
    Ok(v)
}

pub async fn detail(
    State(state): State<SharedState>,
    _user: User,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let h = load_host(&state, id).await?;
    Ok(Json(host_detail(&state, &h).await?))
}

fn opt_string(v: &Value) -> ApiResult<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s.trim().is_empty() => Ok(None),
        Value::String(s) => Ok(Some(s.trim().to_owned())),
        _ => Err(ApiError::bad_request("expected a string")),
    }
}

pub async fn update(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
    Json(body): Json<Map<String, Value>>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Admin)?;
    load_host(&state, id).await?;
    for (key, value) in &body {
        let column = match key.as_str() {
            "display_name" | "description" | "override_url" | "override_username"
            | "override_password" => key.as_str(),
            "tls_skip_verify" => {
                let b = value
                    .as_bool()
                    .ok_or_else(|| ApiError::bad_request("tls_skip_verify must be a boolean"))?;
                sqlx::query("UPDATE hosts SET tls_skip_verify = ? WHERE id = ?")
                    .bind(b as i64)
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                continue;
            }
            "muted_until" => {
                let v =
                    match value {
                        Value::Null => None,
                        v => Some(v.as_i64().ok_or_else(|| {
                            ApiError::bad_request("muted_until must be a timestamp")
                        })?),
                    };
                sqlx::query("UPDATE hosts SET muted_until = ? WHERE id = ?")
                    .bind(v)
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                continue;
            }
            other => return Err(ApiError::bad_request(format!("unknown field {other}"))),
        };
        let mut v = opt_string(value)?;
        if column == "override_url"
            && let Some(u) = &v
        {
            if !(u.starts_with("http://") || u.starts_with("https://")) {
                return Err(ApiError::bad_request(
                    "override_url must start with http:// or https://",
                ));
            }
            v = Some(u.trim_end_matches('/').to_owned());
        }
        sqlx::query(&format!("UPDATE hosts SET {column} = ? WHERE id = ?"))
            .bind(v)
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    let h = load_host(&state, id).await?;
    let detail = host_detail(&state, &h).await?;
    if let Some(summary) = views::host_summary_by_id(&state.db, id).await? {
        state.publish(json!({"type": "host", "host": summary}));
    }
    Ok(Json(detail))
}

pub async fn remove(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    let h = load_host(&state, id).await?;
    sqlx::query("DELETE FROM hosts WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    state
        .series
        .lock()
        .unwrap()
        .retain(|(host, _, _), _| *host != id);
    state.publish(json!({"type": "host_removed", "id": id}));
    tracing::info!(host = %h.hostname, by = %user.username, "host deleted");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    user.require(Role::Operator)?;
    let h = load_host(&state, id).await?;
    let Some(target) = views::action_target(&state.db, &state.config, &h).await? else {
        return Ok(Json(json!({
            "ok": false, "latency_ms": null,
            "message": "No Monit HTTP interface known. Enable `set httpd` in monitrc or set a URL override.",
        })));
    };
    Ok(Json(match client::probe(&target).await {
        Ok(d) => json!({"ok": true, "latency_ms": d.as_secs_f64() * 1000.0,
                        "message": format!("Connected to {}", target.describe())}),
        Err(e) => json!({"ok": false, "latency_ms": null, "message": format!("{e:#}")}),
    }))
}

#[derive(Deserialize)]
pub struct ActionBody {
    action: String,
    #[serde(default)]
    services: Vec<String>,
}

async fn run_action(
    state: &SharedState,
    user: &User,
    h: &HostRow,
    services: Vec<String>,
    action: &str,
) -> ApiResult<Json<Value>> {
    user.require(Role::Operator)?;
    if !model::USER_ACTIONS.contains(&action) {
        return Err(ApiError::bad_request(format!("invalid action {action}")));
    }
    if services.is_empty() {
        return Err(ApiError::bad_request("no services given"));
    }
    let Some(target) = views::action_target(&state.db, &state.config, h).await? else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "No Monit HTTP interface known for this host. Enable `set httpd` in monitrc or set a URL override.",
        ));
    };
    match client::do_action(&target, &services, action).await {
        Ok(()) => {
            tracing::info!(host = %h.hostname, ?services, action, by = %user.username, "service action");
            let id = sqlx::query(
                "INSERT INTO events (host_id, service, service_type, event_type, state, action, message, created_at, source)
                 VALUES (?, ?, NULL, 131072, 2, 0, ?, ?, 'monarch')",
            )
            .bind(h.id)
            .bind(if services.len() == 1 { Some(services[0].clone()) } else { None })
            .bind(format!("{action} requested by {} for {}", user.username, services.join(", ")))
            .bind(crate::state::now_f())
            .execute(&state.db)
            .await?
            .last_insert_rowid();
            collector::publish_events(state, &[id]).await?;
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => Err(ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}"))),
    }
}

pub async fn action(
    State(state): State<SharedState>,
    user: User,
    Path((id, name)): Path<(i64, String)>,
    Json(body): Json<ActionBody>,
) -> ApiResult<Json<Value>> {
    let h = load_host(&state, id).await?;
    run_action(&state, &user, &h, vec![name], &body.action).await
}

pub async fn bulk_action(
    State(state): State<SharedState>,
    user: User,
    Path(id): Path<i64>,
    Json(body): Json<ActionBody>,
) -> ApiResult<Json<Value>> {
    let h = load_host(&state, id).await?;
    run_action(&state, &user, &h, body.services, &body.action).await
}

/// Shared with the M/Monit compatibility layer.
pub async fn action_by_name(
    state: &SharedState,
    user: &User,
    host_id: i64,
    service: &str,
    action: &str,
) -> ApiResult<Json<Value>> {
    let h = load_host(state, host_id).await?;
    run_action(state, user, &h, vec![service.to_owned()], action).await
}

pub async fn service(
    State(state): State<SharedState>,
    _user: User,
    Path((id, name)): Path<(i64, String)>,
) -> ApiResult<Json<Value>> {
    let h = load_host(&state, id).await?;
    let services = views::fetch_services(&state.db, id).await?;
    let s = services
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| ApiError::not_found("service"))?;
    let counts = views::event_counts(&state.db, id).await?;
    let mut v = views::service_json(
        s,
        &h.servicegroups(),
        counts.get(&s.name).copied().unwrap_or(0),
    );
    let events: Vec<EventRow> = sqlx::query_as(&format!(
        "{} WHERE e.host_id = ? AND e.service = ? ORDER BY e.created_at DESC LIMIT 25",
        views::EVENT_SELECT
    ))
    .bind(id)
    .bind(&name)
    .fetch_all(&state.db)
    .await?;
    v["host"] = views::host_summary(&state.db, &h, &services).await?;
    v["recent_events"] = Value::Array(events.iter().map(views::event_json).collect());
    Ok(Json(v))
}
