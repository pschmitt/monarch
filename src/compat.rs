//! A subset of the M/Monit HTTP API, so existing clients (for example the
//! Home Assistant M/Monit integration) can talk to Monarch unchanged.

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    api::{ApiError, ApiResult},
    auth::{self, Role, User},
    monit::model,
    state::SharedState,
    views::{self, HostRow, ServiceRow},
};

pub fn router() -> Router<SharedState> {
    let mut r = Router::new()
        .route("/index.csp", get(|| async { Redirect::to("/") }))
        .route("/z_security_check", post(login))
        .route("/login/logout.csp", get(logout))
        .route("/status/hosts/detail", get(detail_redirect))
        .route(
            "/reports/events/",
            get(|| async { Redirect::to("/events") }),
        );
    for prefix in ["/api/2", ""] {
        r = r
            .route(&format!("{prefix}/status/hosts/list"), get(hosts_list))
            .route(&format!("{prefix}/status/hosts/get"), get(hosts_get))
            .route(&format!("{prefix}/action/service"), post(action));
    }
    r
}

#[derive(Deserialize)]
struct LoginForm {
    z_username: String,
    z_password: String,
}

async fn login(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Form(f): Form<LoginForm>,
) -> ApiResult<Response> {
    match auth::check_credentials(&state.db, f.z_username.trim(), &f.z_password).await? {
        Some(u) if u.role() != Role::Collector => {
            let token = auth::create_session(&state, u.id).await?;
            let cookie = auth::session_cookie(&state, auth::is_secure(&headers), &token);
            Ok(([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response())
        }
        _ => Ok(Redirect::to("/login?error=1").into_response()),
    }
}

async fn logout(State(state): State<SharedState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = auth::session_token(&headers) {
        auth::delete_session(&state.db, &token).await?;
    }
    Ok((
        [(header::SET_COOKIE, auth::clear_cookie())],
        Redirect::to("/login"),
    )
        .into_response())
}

#[derive(Deserialize)]
struct IdQuery {
    id: i64,
}

async fn detail_redirect(Query(q): Query<IdQuery>) -> Redirect {
    Redirect::to(&format!("/hosts/{}", q.id))
}

/// Like M/Monit, unauthenticated API calls are redirected to the login page
/// (clients detect the non-JSON answer and log in).
async fn session_user(
    state: &SharedState,
    headers: &HeaderMap,
) -> ApiResult<Result<User, Response>> {
    Ok(match auth::user_from_headers(state, headers).await? {
        Some(u) if u.role() != Role::Collector => Ok(u),
        _ => Err(Redirect::to("/login").into_response()),
    })
}

fn host_led(h: &HostRow, services: &[ServiceRow]) -> i64 {
    match views::host_state(h, services) {
        "offline" => 0,
        "degraded" => 1,
        _ => 2,
    }
}

fn host_status(h: &HostRow, services: &[ServiceRow]) -> String {
    if h.online == 0 {
        return "Monit is not responding".into();
    }
    let c = views::counts(services);
    let available = c.total - c.failed;
    if c.failed == 0 {
        format!("All {} services are available", c.total)
    } else {
        format!("{available} out of {} services are available", c.total)
    }
}

fn paged(records: Value, n: usize) -> Value {
    json!({
        "records": records, "pageSize": n.max(15), "recordsReturned": n, "totalRecords": n,
        "startIndex": 0, "sort": "led", "dir": "desc",
    })
}

async fn hosts_list(State(state): State<SharedState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Err(r) = session_user(&state, &headers).await? {
        return Ok(r);
    }
    let hosts = views::all_hosts(&state.db).await?;
    let events: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT host_id, COUNT(*) FROM events WHERE host_id IS NOT NULL GROUP BY host_id",
    )
    .fetch_all(&state.db)
    .await?;
    let mut records: Vec<Value> = hosts
        .iter()
        .map(|(h, services)| {
            let sys: Value = services
                .iter()
                .find(|s| s.type_id == 5)
                .and_then(|s| serde_json::from_str(&s.data).ok())
                .unwrap_or_default();
            json!({
                "id": h.id,
                "led": host_led(h, services),
                "hostname": h.name(),
                "events": events.iter().find(|(id, _)| *id == h.id).map(|(_, n)| *n).unwrap_or(0),
                "cpu": sys["cpu"]["total"].as_f64().map(round1),
                "mem": sys["memory"]["percent"],
                "status": host_status(h, services),
                "statusid": 0,
                "heartbeat": h.online,
            })
        })
        .collect();
    records.sort_by_key(|r| r["led"].as_i64().unwrap_or(0));
    let n = records.len();
    Ok(Json(paged(Value::Array(records), n)).into_response())
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn service_led(s: &ServiceRow) -> i64 {
    match s.state.as_str() {
        "failed" => 0,
        "ok" => 2,
        "unmonitored" => 3,
        _ => 1,
    }
}

/// M/Monit "statistics" entries (type id, descriptor, value).
fn statistics(s: &ServiceRow, d: &Value) -> Vec<(i64, String, Value)> {
    let mut out: Vec<(i64, String, Value)> = Vec::new();
    let mut push = |t: i64, v: &Value| {
        if !v.is_null() {
            out.push((t, String::new(), v.clone()));
        }
    };
    match s.type_id {
        5 => {
            push(0, &d["load"][0]);
            push(1, &d["cpu"]["user"]);
            push(2, &d["cpu"]["system"]);
            push(3, &d["cpu"]["wait"]);
            push(4, &d["memory"]["percent"]);
            push(5, &d["memory"]["kb"]);
            push(6, &d["swap"]["percent"]);
            push(7, &d["swap"]["kb"]);
        }
        3 => {
            push(8, &d["cpu"]["percent"]);
            push(9, &d["cpu"]["percent_total"]);
            push(10, &d["memory"]["percent"]);
            push(11, &d["memory"]["percent_total"]);
            push(12, &d["memory"]["kb"]);
            push(13, &d["memory"]["kb_total"]);
            push(14, &d["children"]);
            push(27, &d["uid"]);
            push(28, &d["gid"]);
            push(30, &d["pid"]);
            push(31, &d["ppid"]);
            push(34, &d["uptime"]);
            push(52, &d["threads"]);
        }
        0 => {
            push(18, &d["space"]["percent"]);
            push(19, &d["space"]["used_mb"]);
            push(20, &d["space"]["total_mb"]);
            push(21, &d["inodes"]["percent"]);
            push(22, &d["inodes"]["used"]);
            push(23, &d["inodes"]["total"]);
            push(25, &d["flags"]);
            push(26, &d["mode"]);
            push(27, &d["uid"]);
            push(28, &d["gid"]);
            push(53, &d["fstype"]);
        }
        1 | 2 | 6 => {
            push(26, &d["mode"]);
            push(27, &d["uid"]);
            push(28, &d["gid"]);
            push(29, &d["size"]);
        }
        7 => {
            push(24, &d["exit_status"]);
            push(35, &d["output"]);
        }
        8 => {
            push(36, &d["link"]["state"]);
            push(37, &d["link"]["speed"]);
            push(38, &d["link"]["duplex"]);
            push(39, &d["download"]["bytes"]);
            push(40, &d["download"]["bytes_total"]);
            push(41, &d["download"]["packets"]);
            push(42, &d["download"]["packets_total"]);
            push(43, &d["download"]["errors"]);
            push(44, &d["download"]["errors_total"]);
            push(45, &d["upload"]["bytes"]);
            push(46, &d["upload"]["bytes_total"]);
            push(47, &d["upload"]["packets"]);
            push(48, &d["upload"]["packets_total"]);
            push(49, &d["upload"]["errors"]);
            push(50, &d["upload"]["errors_total"]);
        }
        _ => {}
    }
    if let Some(icmp) = d["icmp"].as_array().and_then(|a| a.first())
        && let Some(ms) = icmp["response_ms"].as_f64()
    {
        out.push((17, String::new(), json!(ms / 1000.0)));
    }
    if let Some(ports) = d["ports"].as_array() {
        for p in ports {
            let desc = format!(
                "{}:{} [{}/{}]",
                p["hostname"].as_str().unwrap_or(""),
                p["port"].as_i64().unwrap_or(0),
                p["protocol"].as_str().unwrap_or(""),
                p["type"].as_str().unwrap_or("")
            );
            if let Some(ms) = p["response_ms"].as_f64() {
                out.push((15, desc.clone(), json!(ms / 1000.0)));
            }
            if let Some(days) = p["cert_valid_days"].as_i64() {
                out.push((75, desc, json!(days)));
            }
        }
    }
    out
}

async fn hosts_get(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<IdQuery>,
) -> ApiResult<Response> {
    if let Err(r) = session_user(&state, &headers).await? {
        return Ok(r);
    }
    let h = views::fetch_host(&state.db, q.id)
        .await?
        .ok_or_else(|| ApiError::not_found("host"))?;
    let services = views::fetch_services(&state.db, h.id).await?;
    let counts = views::event_counts(&state.db, h.id).await?;
    let groups = h.servicegroups();
    let uptime = services
        .iter()
        .find(|s| s.type_id == 5)
        .and_then(|s| serde_json::from_str::<Value>(&s.data).ok())
        .and_then(|d| d["uptime"].as_i64())
        .map(model::uptime_text);
    let svc: Vec<Value> = services
        .iter()
        .map(|s| {
            let d: Value = serde_json::from_str(&s.data).unwrap_or_default();
            let group = groups
                .iter()
                .find(|(_, m)| m.contains(&s.name))
                .map(|(g, _)| g.clone());
            let stats: Vec<Value> = statistics(s, &d)
                .into_iter()
                .enumerate()
                .map(|(i, (t, desc, v))| json!({"id": s.id * 100 + i as i64, "type": t, "descriptor": desc, "value": v}))
                .collect();
            json!({
                "id": s.id,
                "name": s.name,
                "nameid": s.id,
                "type": model::service_type_label(s.type_id),
                "typeid": s.type_id,
                "monitorstate": s.monitor,
                "monitormode": s.monitor_mode,
                "onreboot": s.onreboot,
                "status": model::status_text(s.type_id, s.status, s.monitor, s.pending_action),
                "led": service_led(s),
                "events": counts.get(&s.name).copied().unwrap_or(0),
                "every": s.every.clone().unwrap_or_else(|| "Every cycle".into()),
                "group": group,
                "pid": d["pid"],
                "ppid": d["ppid"],
                "statistics": stats,
            })
        })
        .collect();
    let host = json!({
        "id": h.id,
        "name": h.name(),
        "hostname": h.hostname,
        "led": host_led(&h, &services),
        "status": host_status(&h, &services),
        "statusid": 0,
        "heartbeat": h.online,
        "platform": {"name": h.os_name, "release": h.os_release, "version": h.os_version, "machine": h.machine},
        "cpu": {"count": h.cpu_count},
        "memory": {"size": h.mem_total_kb},
        "swap": {"size": h.swap_total_kb},
        "uptime": uptime,
        "monit": {
            "id": h.monit_id, "version": h.monit_version, "controlfile": h.controlfile,
            "poll": h.poll, "startdelay": h.startdelay, "uptime": h.monit_uptime.map(model::uptime_text),
        },
        "services": svc,
    });
    Ok(Json(paged(json!({"host": host}), 1)).into_response())
}

#[derive(Deserialize)]
struct ActionForm {
    hostid: i64,
    service: String,
    action: String,
}

async fn action(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Form(f): Form<ActionForm>,
) -> ApiResult<Response> {
    let user = match session_user(&state, &headers).await? {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    crate::api::hosts_action(&state, &user, f.hostid, &f.service, &f.action)
        .await
        .map(IntoResponse::into_response)
}
