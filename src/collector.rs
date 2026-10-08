//! `POST /collector`: the endpoint Monit agents report to
//! (`set mmonit http://user:pass@monarch:8080/collector`).

use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, Transaction};

use crate::{
    auth,
    monit::{model, xml},
    notify,
    state::{SharedState, now},
    views,
};

const AUTH_CACHE_TTL: Duration = Duration::from_secs(600);

pub async fn collector(
    State(state): State<SharedState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match authorize(&state, &headers).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(%peer, "collector: rejected credentials");
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"monarch\"")],
                "unauthorized",
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("collector auth: {e:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    let doc = match xml::parse(&body) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(%peer, "collector: cannot parse status document: {e}");
            return (
                StatusCode::BAD_REQUEST,
                format!("invalid status document: {e}"),
            )
                .into_response();
        }
    };

    let remote = client_ip(&headers).unwrap_or_else(|| peer.ip().to_string());
    match ingest(&state, doc, &remote, None).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!(%peer, "collector: ingest failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "ingest failed").into_response()
        }
    }
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    let v = headers.get("x-forwarded-for")?.to_str().ok()?;
    v.split(',')
        .next()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

async fn authorize(state: &SharedState, headers: &HeaderMap) -> Result<bool> {
    let creds = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| {
            base64::engine::general_purpose::STANDARD
                .decode(v.trim())
                .ok()
        })
        .and_then(|v| String::from_utf8(v).ok());
    let Some(creds) = creds else {
        return Ok(state.config.collector_allow_anonymous);
    };
    let key: [u8; 32] = Sha256::digest(creds.as_bytes()).into();
    {
        let mut cache = state.collector_auth.lock().unwrap();
        cache.retain(|_, t| t.elapsed() < AUTH_CACHE_TTL);
        if cache.contains_key(&key) {
            return Ok(true);
        }
    }
    let (user, pass) = creds.split_once(':').unwrap_or((&creds, ""));
    // Monit URL-encodes nothing, but be lenient with users that did.
    let user = percent_decode(user);
    let pass = percent_decode(pass);
    if auth::check_credentials(&state.db, &user, &pass)
        .await?
        .is_some()
    {
        state
            .collector_auth
            .lock()
            .unwrap()
            .insert(key, Instant::now());
        return Ok(true);
    }
    Ok(state.config.collector_allow_anonymous)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct Existing {
    state: String,
    state_since: Option<i64>,
    status: i64,
}

async fn series_id(
    state: &SharedState,
    tx: &mut Transaction<'_, Sqlite>,
    host_id: i64,
    service: &str,
    metric: &str,
) -> Result<i64> {
    let key = (host_id, service.to_owned(), metric.to_owned());
    if let Some(id) = state.series.lock().unwrap().get(&key) {
        return Ok(*id);
    }
    sqlx::query(
        "INSERT INTO series (host_id, service, metric) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(host_id)
    .bind(service)
    .bind(metric)
    .execute(&mut **tx)
    .await?;
    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM series WHERE host_id = ? AND service = ? AND metric = ?")
            .bind(host_id)
            .bind(service)
            .bind(metric)
            .fetch_one(&mut **tx)
            .await?;
    state.series.lock().unwrap().insert(key, id);
    Ok(id)
}

async fn record_sample(
    state: &SharedState,
    tx: &mut Transaction<'_, Sqlite>,
    host_id: i64,
    service: &str,
    metric: &str,
    ts: i64,
    value: f64,
) -> Result<()> {
    let sid = series_id(state, tx, host_id, service, metric).await?;
    let inserted =
        sqlx::query("INSERT OR IGNORE INTO samples (series_id, ts, value) VALUES (?, ?, ?)")
            .bind(sid)
            .bind(ts)
            .bind(value)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    if inserted == 0 {
        return Ok(());
    }
    for (table, width) in [("rollup_5m", 300), ("rollup_1h", 3600)] {
        sqlx::query(&format!(
            "INSERT INTO {table} (series_id, bucket, sum, count, min, max) VALUES (?, ?, ?, 1, ?, ?)
             ON CONFLICT (series_id, bucket) DO UPDATE SET
               sum = sum + excluded.sum, count = count + 1,
               min = MIN(min, excluded.min), max = MAX(max, excluded.max)"
        ))
        .bind(sid)
        .bind(ts - ts.rem_euclid(width))
        .bind(value)
        .bind(value)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Store a status document. `target` is set for documents Monarch pulled
/// itself: those carry no events, so events are derived from state changes.
pub async fn ingest(
    state: &SharedState,
    doc: xml::Monit,
    remote: &str,
    target: Option<i64>,
) -> Result<i64> {
    let _guard = state.ingest_lock.lock().await;
    // Series ids cached during a transaction that never commits (error, or the
    // request future being dropped) would point at rows that do not exist.
    let mut cache_guard = SeriesCacheGuard { state, armed: true };
    let result = ingest_locked(state, doc, remote, target).await;
    cache_guard.armed = result.is_err();
    result
}

struct SeriesCacheGuard<'a> {
    state: &'a SharedState,
    armed: bool,
}

impl Drop for SeriesCacheGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.state.series.lock().unwrap().clear();
        }
    }
}

async fn ingest_locked(
    state: &SharedState,
    doc: xml::Monit,
    remote: &str,
    target: Option<i64>,
) -> Result<i64> {
    let srv = &doc.server;
    let monit_id = doc
        .id
        .clone()
        .or_else(|| srv.id.clone())
        .filter(|s| !s.is_empty())
        .context("status document has no monit id")?;
    let hostname = srv
        .localhostname
        .clone()
        .unwrap_or_else(|| monit_id.clone());
    let ts = now();
    let platform = doc.platform.as_ref();
    let httpd = srv.httpd.as_ref();
    let creds = srv.credentials.as_ref();
    let hostgroups: Vec<String> = doc
        .hostgroups
        .as_ref()
        .map(|g| {
            g.names
                .iter()
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let servicegroups: HashMap<String, Vec<String>> = doc
        .servicegroups
        .as_ref()
        .map(|g| {
            g.groups
                .iter()
                .map(|g| (g.name.clone(), g.services.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut tx = state.db.begin_with("BEGIN IMMEDIATE").await?;

    let prev: Option<(i64, i64)> =
        sqlx::query_as("SELECT id, online FROM hosts WHERE monit_id = ?")
            .bind(&monit_id)
            .fetch_optional(&mut *tx)
            .await?;

    let (host_id,): (i64,) = sqlx::query_as(
        "INSERT INTO hosts (monit_id, hostname, monit_version, incarnation, monit_uptime, poll, startdelay,
            controlfile, os_name, os_release, os_version, machine, cpu_count, mem_total_kb, swap_total_kb,
            httpd_address, httpd_port, httpd_ssl, httpd_unixsocket, reported_username, reported_password,
            hostgroups, servicegroups, remote_addr, first_seen, last_seen, online)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
         ON CONFLICT (monit_id) DO UPDATE SET
            hostname = excluded.hostname, monit_version = excluded.monit_version,
            incarnation = excluded.incarnation, monit_uptime = excluded.monit_uptime,
            poll = excluded.poll, startdelay = excluded.startdelay, controlfile = excluded.controlfile,
            os_name = excluded.os_name, os_release = excluded.os_release, os_version = excluded.os_version,
            machine = excluded.machine, cpu_count = excluded.cpu_count, mem_total_kb = excluded.mem_total_kb,
            swap_total_kb = excluded.swap_total_kb, httpd_address = excluded.httpd_address,
            httpd_port = excluded.httpd_port, httpd_ssl = excluded.httpd_ssl,
            httpd_unixsocket = excluded.httpd_unixsocket, reported_username = excluded.reported_username,
            reported_password = excluded.reported_password, hostgroups = excluded.hostgroups,
            servicegroups = excluded.servicegroups, remote_addr = excluded.remote_addr,
            last_seen = excluded.last_seen, online = 1
         RETURNING id",
    )
    .bind(&monit_id)
    .bind(&hostname)
    .bind(doc.version.clone().or_else(|| srv.version.clone()))
    .bind(xml::int(&doc.incarnation).or(xml::int(&srv.incarnation)))
    .bind(xml::int(&srv.uptime))
    .bind(xml::int(&srv.poll).unwrap_or(30).max(1))
    .bind(xml::int(&srv.startdelay))
    .bind(&srv.controlfile)
    .bind(platform.and_then(|p| p.name.clone()))
    .bind(platform.and_then(|p| p.release.clone()))
    .bind(platform.and_then(|p| p.version.clone()))
    .bind(platform.and_then(|p| p.machine.clone()))
    .bind(platform.and_then(|p| xml::int(&p.cpu)))
    .bind(platform.and_then(|p| xml::int(&p.memory)))
    .bind(platform.and_then(|p| xml::int(&p.swap)))
    .bind(httpd.and_then(|h| h.address.clone()))
    .bind(httpd.and_then(|h| xml::int(&h.port)))
    .bind(httpd.and_then(|h| xml::int(&h.ssl)).unwrap_or(0))
    .bind(httpd.and_then(|h| h.unixsocket.clone()))
    .bind(creds.and_then(|c| c.username.clone()))
    .bind(creds.and_then(|c| c.password.clone()))
    .bind(serde_json::to_string(&hostgroups)?)
    .bind(serde_json::to_string(&servicegroups)?)
    .bind(remote)
    .bind(ts)
    .bind(ts)
    .fetch_one(&mut *tx)
    .await?;

    // Hosts that also push send real events; only derive events for pull-only hosts.
    let pushing = match target {
        None => {
            sqlx::query("UPDATE hosts SET last_push = ? WHERE id = ?")
                .bind(ts)
                .bind(host_id)
                .execute(&mut *tx)
                .await?;
            true
        }
        Some(_) => {
            let (last_push, poll): (Option<i64>, i64) =
                sqlx::query_as("SELECT last_push, poll FROM hosts WHERE id = ?")
                    .bind(host_id)
                    .fetch_one(&mut *tx)
                    .await?;
            last_push.is_some_and(|p| ts - p <= (poll * 3).max(180))
        }
    };

    if let Some(t) = target {
        sqlx::query("UPDATE hosts SET target_id = ? WHERE id = ?")
            .bind(t)
            .bind(host_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE targets SET host_id = ? WHERE id = ?")
            .bind(host_id)
            .bind(t)
            .execute(&mut *tx)
            .await?;
    }

    let existing: HashMap<String, Existing> =
        sqlx::query_as::<_, (String, String, Option<i64>, i64)>(
            "SELECT name, state, state_since, status FROM services WHERE host_id = ?",
        )
        .bind(host_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|(name, state, state_since, status)| {
            (
                name,
                Existing {
                    state,
                    state_since,
                    status,
                },
            )
        })
        .collect();

    // Monit needs one cycle to compute deltas; its first report says 0% cpu.
    let warming_up = xml::int(&srv.uptime).unwrap_or(i64::MAX)
        <= xml::int(&srv.startdelay).unwrap_or(0) + xml::int(&srv.poll).unwrap_or(30);

    let mut transitions: Vec<(String, i64, i64, i64, String)> = Vec::new();

    let services = doc
        .services
        .as_ref()
        .map(|s| s.services.as_slice())
        .unwrap_or_default();
    for s in services {
        let name = s.name();
        if name.is_empty() {
            continue;
        }
        let status = xml::int(&s.status).unwrap_or(0);
        let monitor = xml::int(&s.monitor).unwrap_or(0);
        let pending = xml::int(&s.pendingaction).unwrap_or(0);
        let svc_state = model::service_state(status, monitor, pending);
        let collected = xml::num(&s.collected_sec).unwrap_or(ts as f64)
            + xml::num(&s.collected_usec).unwrap_or(0.0) / 1e6;
        if !pushing
            && let Some(prev) = existing.get(&name)
            && prev.state != svc_state
            && (svc_state == "failed" || (prev.state == "failed" && svc_state == "ok"))
        {
            let failed = svc_state == "failed";
            let bits = if failed {
                status & !prev.status
            } else {
                prev.status
            };
            let bits = if bits == 0 {
                status.max(prev.status)
            } else {
                bits
            };
            transitions.push((
                name.clone(),
                s.type_id(),
                bits,
                if failed { 1 } else { 0 },
                model::status_text(s.type_id(), status, monitor, pending),
            ));
        }
        let since = match existing.get(&name) {
            Some(e) if e.state == svc_state => e.state_since.or(Some(ts)),
            _ => Some(ts),
        };
        let data = model::service_data(s);
        sqlx::query(
            "INSERT INTO services (host_id, name, type, status, status_hint, monitor, monitor_mode, onreboot,
                pending_action, every, collected_at, data, state, state_since, last_seen)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (host_id, name) DO UPDATE SET
                type = excluded.type, status = excluded.status, status_hint = excluded.status_hint,
                monitor = excluded.monitor, monitor_mode = excluded.monitor_mode, onreboot = excluded.onreboot,
                pending_action = excluded.pending_action, every = excluded.every,
                collected_at = excluded.collected_at, data = excluded.data, state = excluded.state,
                state_since = excluded.state_since, last_seen = excluded.last_seen",
        )
        .bind(host_id)
        .bind(&name)
        .bind(s.type_id())
        .bind(status)
        .bind(xml::int(&s.status_hint).unwrap_or(0))
        .bind(monitor)
        .bind(xml::int(&s.monitormode).unwrap_or(0))
        .bind(xml::int(&s.onreboot).unwrap_or(0))
        .bind(pending)
        .bind(model::every_text(s))
        .bind(collected)
        .bind(data.to_string())
        .bind(svc_state)
        .bind(since)
        .bind(ts)
        .execute(&mut *tx)
        .await?;

        // Only record metrics for monitored services with fresh data.
        if !warming_up && monitor & 1 != 0 && status_is_measurable(svc_state) {
            let sample_ts = xml::int(&s.collected_sec).filter(|t| *t > 0).unwrap_or(ts);
            for (metric, value) in model::metrics(s) {
                record_sample(state, &mut tx, host_id, &name, metric, sample_ts, value).await?;
            }
        }
    }

    // Services that are no longer part of the monit configuration.
    if !services.is_empty() {
        let removed = sqlx::query("DELETE FROM services WHERE host_id = ? AND last_seen < ?")
            .bind(host_id)
            .bind(ts)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if removed > 0 {
            tracing::info!(host = %hostname, removed, "removed services no longer reported");
        }
    }
    let mut new_events: Vec<i64> = Vec::new();

    if let Some((_, 0)) = prev {
        let id = sqlx::query(
            "INSERT INTO events (host_id, service, service_type, event_type, state, action, message, created_at, source)
             VALUES (?, NULL, NULL, ?, 0, 1, ?, ?, 'monarch')",
        )
        .bind(host_id)
        .bind(model::EVENT_HEARTBEAT)
        .bind(format!("{hostname} is reporting again"))
        .bind(ts as f64)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        new_events.push(id);
    }

    if let Some(ev) = &doc.event {
        // An agent with a wrong clock reports events from the future (rofl-12 once
        // jumped 33 days ahead); never let them sort or group in the future.
        let created = (xml::num(&ev.collected_sec).unwrap_or(ts as f64)
            + xml::num(&ev.collected_usec).unwrap_or(0.0) / 1e6)
            .min(ts as f64 + 60.0);
        let service = ev.service.clone().filter(|s| !s.is_empty());
        let event_type = xml::int(&ev.id).unwrap_or(0);
        let ev_state = xml::int(&ev.state).unwrap_or(0);
        let dupe: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM events WHERE host_id = ? AND service IS ? AND event_type = ? AND state = ?
               AND abs(created_at - ?) < 0.001",
        )
        .bind(host_id)
        .bind(&service)
        .bind(event_type)
        .bind(ev_state)
        .bind(created)
        .fetch_optional(&mut *tx)
        .await?;
        if dupe.is_none() {
            let id = sqlx::query(
                "INSERT INTO events (host_id, service, service_type, event_type, state, action, message, created_at, source)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'monit')",
            )
            .bind(host_id)
            .bind(&service)
            .bind(xml::int(&ev.service_type))
            .bind(event_type)
            .bind(ev_state)
            .bind(xml::int(&ev.action).unwrap_or(0))
            .bind(ev.message.clone().unwrap_or_default().trim().to_owned())
            .bind(created)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            new_events.push(id);
        }
    }

    for (service, type_id, bits, ev_state, text) in transitions {
        let id = sqlx::query(
            "INSERT INTO events (host_id, service, service_type, event_type, state, action, message, created_at, source)
             VALUES (?, ?, ?, ?, ?, 1, ?, ?, 'monarch')",
        )
        .bind(host_id)
        .bind(service)
        .bind(type_id)
        .bind(bits)
        .bind(ev_state)
        .bind(text)
        .bind(ts as f64)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        new_events.push(id);
    }

    tx.commit().await?;

    if let Some(summary) = views::host_summary_by_id(&state.db, host_id).await? {
        state.publish(json!({"type": "host", "host": summary}));
    }
    publish_events(state, &new_events).await?;
    if prev.is_none() {
        tracing::info!(host = %hostname, %remote, "new host registered");
    }
    Ok(host_id)
}

fn status_is_measurable(state: &str) -> bool {
    state != "init" && state != "unmonitored"
}

/// Publish freshly stored events on the live stream and hand them to the notifier.
pub async fn publish_events(state: &SharedState, ids: &[i64]) -> Result<()> {
    for id in ids {
        if let Some(e) = views::fetch_event(&state.db, *id).await? {
            state.publish(json!({"type": "event", "event": views::event_json(&e)}));
            notify::dispatch(state.clone(), e);
        }
    }
    Ok(())
}
