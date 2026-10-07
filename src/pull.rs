//! Pull mode: poll Monit agents' HTTP interfaces (directly or through SSH).

use std::{collections::HashSet, sync::Mutex, time::Duration};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::{
    collector,
    monit::{client, xml},
    state::{SharedState, now},
    views::{self, TargetRow},
};

static IN_FLIGHT: Mutex<Option<HashSet<i64>>> = Mutex::new(None);

pub fn spawn(state: SharedState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            if let Err(e) = schedule(&state).await {
                tracing::error!("pull scheduler: {e:#}");
            }
        }
    });
}

async fn schedule(state: &SharedState) -> Result<()> {
    let ts = now();
    let due: Vec<TargetRow> = sqlx::query_as(
        "SELECT * FROM targets WHERE enabled = 1 AND (last_polled_at IS NULL OR last_polled_at + interval <= ?)",
    )
    .bind(ts)
    .fetch_all(&state.db)
    .await?;
    for t in due {
        {
            let mut guard = IN_FLIGHT.lock().unwrap();
            let set = guard.get_or_insert_with(HashSet::new);
            if !set.insert(t.id) {
                continue;
            }
        }
        let state = state.clone();
        tokio::spawn(async move {
            let id = t.id;
            if let Err(e) = poll(&state, &t).await {
                tracing::error!(target = %t.name, "poll failed: {e:#}");
            }
            if let Some(set) = IN_FLIGHT.lock().unwrap().as_mut() {
                set.remove(&id);
            }
        });
    }
    Ok(())
}

pub struct Fetched {
    pub doc: xml::Monit,
    pub latency: Duration,
}

pub async fn fetch(state: &SharedState, t: &TargetRow) -> Result<Fetched> {
    let target = t.client_target(&state.config);
    let (body, latency) = client::fetch_status(&target).await?;
    let doc = xml::parse(&body).context("cannot parse monit status")?;
    Ok(Fetched { doc, latency })
}

/// Poll a target once and store the result.
pub async fn poll(state: &SharedState, t: &TargetRow) -> Result<()> {
    let ts = now();
    let result = async {
        let f = fetch(state, t).await?;
        let remote = t
            .ssh_destination
            .clone()
            .filter(|d| !d.is_empty())
            .map(|d| d.rsplit('@').next().unwrap_or(&d).to_owned())
            .or_else(|| {
                reqwest::Url::parse(&t.url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
            })
            .unwrap_or_default();
        collector::ingest(state, f.doc, &remote, Some(t.id)).await
    }
    .await;
    let status = match &result {
        Ok(_) => "ok".to_owned(),
        Err(e) => format!("{e:#}"),
    };
    sqlx::query("UPDATE targets SET last_status = ?, last_polled_at = ? WHERE id = ?")
        .bind(&status)
        .bind(ts)
        .bind(t.id)
        .execute(&state.db)
        .await?;
    if let Some(row) = views::fetch_target(&state.db, t.id).await? {
        state.publish(json!({"type": "target", "target": row.json()}));
    }
    result.map(|_| ())
}

/// One-off connection test without storing anything.
pub async fn test(state: &SharedState, t: &TargetRow) -> Value {
    match fetch(state, t).await {
        Ok(f) => {
            let services = f.doc.services.as_ref().map(|s| s.services.len());
            json!({
                "ok": true,
                "message": format!("Connected to {}", t.client_target(&state.config).describe()),
                "hostname": f.doc.server.localhostname,
                "monit_version": f.doc.version.or(f.doc.server.version),
                "services": services,
                "latency_ms": f.latency.as_secs_f64() * 1000.0,
            })
        }
        Err(e) => json!({
            "ok": false, "message": format!("{e:#}"),
            "hostname": null, "monit_version": null, "services": null, "latency_ms": null,
        }),
    }
}

/// Sync targets declared in the config file (managed = 1).
pub async fn sync_managed(state: &SharedState) -> Result<()> {
    let mut names = Vec::new();
    for t in &state.config.targets {
        let password = match &t.password_file {
            Some(p) => Some(
                std::fs::read_to_string(p)
                    .with_context(|| format!("reading {}", p.display()))?
                    .trim_end_matches(['\n', '\r'])
                    .to_owned(),
            ),
            None => None,
        };
        sqlx::query(
            "INSERT INTO targets (name, url, username, password, ssh_destination, ssh_port, interval,
                tls_skip_verify, enabled, managed, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, 1, ?)
             ON CONFLICT (name) DO UPDATE SET url = excluded.url, username = excluded.username,
                password = excluded.password, ssh_destination = excluded.ssh_destination,
                ssh_port = excluded.ssh_port, interval = excluded.interval,
                tls_skip_verify = excluded.tls_skip_verify, enabled = 1, managed = 1",
        )
        .bind(&t.name)
        .bind(t.url.trim_end_matches('/'))
        .bind(&t.username)
        .bind(password)
        .bind(&t.ssh_destination)
        .bind(t.ssh_port)
        .bind(t.interval.max(5))
        .bind(t.tls_skip_verify as i64)
        .bind(now())
        .execute(&state.db)
        .await?;
        names.push(t.name.clone());
    }
    // Managed targets that were removed from the config.
    let managed: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM targets WHERE managed = 1")
            .fetch_all(&state.db)
            .await?;
    for (id, name) in managed {
        if !names.contains(&name) {
            sqlx::query("DELETE FROM targets WHERE id = ?")
                .bind(id)
                .execute(&state.db)
                .await?;
            tracing::info!(target = %name, "removed managed connection no longer in config");
        }
    }
    Ok(())
}
