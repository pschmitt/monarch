//! Database rows and their JSON representations (see `docs/API.md`).

use std::collections::HashMap;

use anyhow::Result;
use serde_json::{Value, json};
use sqlx::{FromRow, SqlitePool};

use crate::monit::{client::Target, model};

#[derive(Debug, Clone, FromRow)]
pub struct HostRow {
    pub id: i64,
    pub monit_id: String,
    pub hostname: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub monit_version: Option<String>,
    pub incarnation: Option<i64>,
    pub monit_uptime: Option<i64>,
    pub poll: i64,
    pub startdelay: Option<i64>,
    pub controlfile: Option<String>,
    pub os_name: Option<String>,
    pub os_release: Option<String>,
    pub os_version: Option<String>,
    pub machine: Option<String>,
    pub cpu_count: Option<i64>,
    pub mem_total_kb: Option<i64>,
    pub swap_total_kb: Option<i64>,
    pub httpd_address: Option<String>,
    pub httpd_port: Option<i64>,
    pub httpd_ssl: i64,
    pub httpd_unixsocket: Option<String>,
    pub reported_username: Option<String>,
    pub reported_password: Option<String>,
    pub override_url: Option<String>,
    pub override_username: Option<String>,
    pub override_password: Option<String>,
    pub tls_skip_verify: i64,
    pub hostgroups: String,
    pub servicegroups: String,
    pub remote_addr: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub online: i64,
    pub muted_until: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ServiceRow {
    pub id: i64,
    pub host_id: i64,
    pub name: String,
    #[sqlx(rename = "type")]
    pub type_id: i64,
    pub status: i64,
    pub monitor: i64,
    pub monitor_mode: i64,
    pub onreboot: i64,
    pub pending_action: i64,
    pub every: Option<String>,
    pub collected_at: f64,
    pub data: String,
    pub state: String,
    pub state_since: Option<i64>,
}

impl HostRow {
    pub fn name(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.hostname)
    }

    pub fn monit_url(&self) -> Option<String> {
        if let Some(u) = self.override_url.as_deref().filter(|s| !s.is_empty()) {
            return Some(u.trim_end_matches('/').to_owned());
        }
        let port = self.httpd_port.filter(|p| *p > 0)?;
        let addr = self
            .httpd_address
            .as_deref()
            .filter(|a| !a.is_empty() && *a != "0.0.0.0" && *a != "::")
            .or(self.remote_addr.as_deref())?;
        let addr = if addr.contains(':') && !addr.starts_with('[') {
            format!("[{addr}]")
        } else {
            addr.to_owned()
        };
        let scheme = if self.httpd_ssl != 0 { "https" } else { "http" };
        Some(format!("{scheme}://{addr}:{port}"))
    }

    pub fn target(&self) -> Option<Target> {
        let (username, password) = match self.override_username.as_deref().filter(|s| !s.is_empty()) {
            Some(u) => (Some(u.to_owned()), self.override_password.clone()),
            None => (self.reported_username.clone(), self.reported_password.clone()),
        };
        Some(Target {
            base_url: self.monit_url()?,
            username,
            password,
            tls_skip_verify: self.tls_skip_verify != 0,
        })
    }

    pub fn hostgroups(&self) -> Vec<String> {
        serde_json::from_str(&self.hostgroups).unwrap_or_default()
    }

    pub fn servicegroups(&self) -> HashMap<String, Vec<String>> {
        serde_json::from_str(&self.servicegroups).unwrap_or_default()
    }
}

pub struct Counts {
    pub total: i64,
    pub ok: i64,
    pub failed: i64,
    pub unmonitored: i64,
    pub pending: i64,
}

pub fn counts(services: &[ServiceRow]) -> Counts {
    let mut c = Counts {
        total: services.len() as i64,
        ok: 0,
        failed: 0,
        unmonitored: 0,
        pending: 0,
    };
    for s in services {
        match s.state.as_str() {
            "ok" => c.ok += 1,
            "failed" => c.failed += 1,
            "unmonitored" => c.unmonitored += 1,
            _ => c.pending += 1,
        }
    }
    c
}

pub fn host_state(h: &HostRow, services: &[ServiceRow]) -> &'static str {
    if h.online == 0 {
        "offline"
    } else if services.iter().any(|s| s.state == "failed") {
        "degraded"
    } else {
        "ok"
    }
}

fn system_json(services: &[ServiceRow]) -> Value {
    let Some(sys) = services.iter().find(|s| s.type_id == 5) else {
        return json!({
            "uptime": null, "cpu": null, "cpu_user": null, "cpu_system": null, "cpu_wait": null,
            "mem_percent": null, "mem_kb": null, "swap_percent": null, "swap_kb": null, "load": null,
        });
    };
    let d: Value = serde_json::from_str(&sys.data).unwrap_or_default();
    json!({
        "uptime": d["uptime"],
        "cpu": d["cpu"]["total"],
        "cpu_user": d["cpu"]["user"],
        "cpu_system": d["cpu"]["system"],
        "cpu_wait": d["cpu"]["wait"],
        "mem_percent": d["memory"]["percent"],
        "mem_kb": d["memory"]["kb"],
        "swap_percent": d["swap"]["percent"],
        "swap_kb": d["swap"]["kb"],
        "load": d["load"],
    })
}

pub fn system_service_name(services: &[ServiceRow]) -> Option<&str> {
    services
        .iter()
        .find(|s| s.type_id == 5)
        .map(|s| s.name.as_str())
}

async fn sparkline(db: &SqlitePool, host_id: i64, service: &str, metric: &str) -> Result<Vec<f64>> {
    let rows: Vec<(f64,)> = sqlx::query_as(
        "SELECT value FROM (
            SELECT s.ts, s.value FROM samples s JOIN series r ON r.id = s.series_id
            WHERE r.host_id = ? AND r.service = ? AND r.metric = ?
            ORDER BY s.ts DESC LIMIT 60
         ) ORDER BY ts ASC",
    )
    .bind(host_id)
    .bind(service)
    .bind(metric)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(v,)| v).collect())
}

pub async fn host_summary(db: &SqlitePool, h: &HostRow, services: &[ServiceRow]) -> Result<Value> {
    let c = counts(services);
    let (cpu, mem) = match system_service_name(services) {
        Some(name) => (
            sparkline(db, h.id, name, "cpu").await?,
            sparkline(db, h.id, name, "mem_percent").await?,
        ),
        None => (vec![], vec![]),
    };
    let failing: Vec<&str> = services
        .iter()
        .filter(|s| s.state == "failed")
        .map(|s| s.name.as_str())
        .collect();
    Ok(json!({
        "id": h.id,
        "monit_id": h.monit_id,
        "hostname": h.hostname,
        "display_name": h.display_name,
        "description": h.description,
        "state": host_state(h, services),
        "online": h.online != 0,
        "last_seen": h.last_seen,
        "first_seen": h.first_seen,
        "poll": h.poll,
        "monit_version": h.monit_version,
        "monit_uptime": h.monit_uptime,
        "os": {"name": h.os_name, "release": h.os_release, "version": h.os_version, "machine": h.machine},
        "cpu_count": h.cpu_count,
        "mem_total_kb": h.mem_total_kb,
        "swap_total_kb": h.swap_total_kb,
        "hostgroups": h.hostgroups(),
        "services": {"total": c.total, "ok": c.ok, "failed": c.failed, "unmonitored": c.unmonitored, "pending": c.pending},
        "system": system_json(services),
        "sparkline": {"cpu": cpu, "mem": mem},
        "failing": failing,
        "muted_until": h.muted_until,
        "can_act": h.monit_url().is_some(),
    }))
}

pub fn service_json(s: &ServiceRow, groups: &HashMap<String, Vec<String>>, events: i64) -> Value {
    let mut in_groups: Vec<&str> = groups
        .iter()
        .filter(|(_, members)| members.iter().any(|m| *m == s.name))
        .map(|(g, _)| g.as_str())
        .collect();
    in_groups.sort();
    json!({
        "id": s.id,
        "host_id": s.host_id,
        "name": s.name,
        "type": model::service_type_name(s.type_id),
        "type_id": s.type_id,
        "state": s.state,
        "status_text": model::status_text(s.type_id, s.status, s.monitor, s.pending_action),
        "status": s.status,
        "monitor": s.monitor,
        "monitor_mode": if s.monitor_mode == 1 { "passive" } else { "active" },
        "pending_action": if s.pending_action != 0 { model::action_name(s.pending_action) } else { None },
        "every": s.every,
        "collected_at": s.collected_at,
        "state_since": s.state_since,
        "groups": in_groups,
        "events": events,
        "data": serde_json::from_str::<Value>(&s.data).unwrap_or_default(),
    })
}

pub async fn fetch_host(db: &SqlitePool, id: i64) -> Result<Option<HostRow>> {
    Ok(sqlx::query_as("SELECT * FROM hosts WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?)
}

pub async fn fetch_services(db: &SqlitePool, host_id: i64) -> Result<Vec<ServiceRow>> {
    Ok(sqlx::query_as("SELECT * FROM services WHERE host_id = ? ORDER BY type DESC, name")
        .bind(host_id)
        .fetch_all(db)
        .await?)
}

pub async fn all_hosts(db: &SqlitePool) -> Result<Vec<(HostRow, Vec<ServiceRow>)>> {
    let hosts: Vec<HostRow> = sqlx::query_as("SELECT * FROM hosts ORDER BY hostname COLLATE NOCASE")
        .fetch_all(db)
        .await?;
    let services: Vec<ServiceRow> = sqlx::query_as("SELECT * FROM services ORDER BY type DESC, name")
        .fetch_all(db)
        .await?;
    let mut by_host: HashMap<i64, Vec<ServiceRow>> = HashMap::new();
    for s in services {
        by_host.entry(s.host_id).or_default().push(s);
    }
    Ok(hosts
        .into_iter()
        .map(|h| {
            let s = by_host.remove(&h.id).unwrap_or_default();
            (h, s)
        })
        .collect())
}

pub async fn host_summary_by_id(db: &SqlitePool, id: i64) -> Result<Option<Value>> {
    let Some(h) = fetch_host(db, id).await? else {
        return Ok(None);
    };
    let services = fetch_services(db, id).await?;
    Ok(Some(host_summary(db, &h, &services).await?))
}

pub async fn event_counts(db: &SqlitePool, host_id: i64) -> Result<HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT service, COUNT(*) FROM events WHERE host_id = ? AND service IS NOT NULL GROUP BY service",
    )
    .bind(host_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub id: i64,
    pub host_id: Option<i64>,
    pub host: Option<String>,
    pub service: Option<String>,
    pub service_type: Option<i64>,
    pub event_type: i64,
    pub state: i64,
    pub action: i64,
    pub message: String,
    pub created_at: f64,
    pub source: String,
    pub acked_by: Option<String>,
    pub acked_at: Option<i64>,
}

pub const EVENT_SELECT: &str = "SELECT e.id, e.host_id, COALESCE(NULLIF(h.display_name, ''), h.hostname) AS host,
    e.service, e.service_type, e.event_type, e.state, e.action, e.message, e.created_at, e.source,
    e.acked_by, e.acked_at FROM events e LEFT JOIN hosts h ON h.id = e.host_id";

pub fn event_json(e: &EventRow) -> Value {
    json!({
        "id": e.id,
        "host_id": e.host_id,
        "host": e.host,
        "service": e.service,
        "service_type": e.service_type.map(model::service_type_name),
        "kind": model::event_kind(e.event_type),
        "kind_label": model::event_label(e.event_type, e.state),
        "state": model::event_state_name(e.state),
        "action": model::action_name(e.action).filter(|a| *a != "ignore"),
        "message": e.message,
        "created_at": e.created_at,
        "source": e.source,
        "acked_by": e.acked_by,
        "acked_at": e.acked_at,
    })
}

pub async fn fetch_event(db: &SqlitePool, id: i64) -> Result<Option<EventRow>> {
    Ok(sqlx::query_as(&format!("{EVENT_SELECT} WHERE e.id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await?)
}
