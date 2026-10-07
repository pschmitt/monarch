//! Background jobs: offline detection and data retention.

use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use crate::{
    collector,
    monit::model,
    state::{SharedState, now},
    views,
};

pub fn spawn(state: SharedState) {
    let s = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            if let Err(e) = heartbeat(&s).await {
                tracing::error!("heartbeat check failed: {e:#}");
            }
        }
    });
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = retention(&state).await {
                tracing::error!("retention job failed: {e:#}");
            }
        }
    });
}

/// Mark hosts offline that have not reported for `heartbeat_grace` poll cycles.
async fn heartbeat(state: &SharedState) -> Result<()> {
    let grace = state.settings.read().await.heartbeat_grace;
    let ts = now();
    let stale: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT id, COALESCE(NULLIF(display_name, ''), hostname), last_seen, poll FROM hosts WHERE online = 1",
    )
    .fetch_all(&state.db)
    .await?;
    for (id, name, last_seen, poll) in stale {
        let limit = ((poll.max(1) as f64) * grace).max(60.0) as i64 + 15;
        if ts - last_seen <= limit {
            continue;
        }
        let changed = sqlx::query("UPDATE hosts SET online = 0 WHERE id = ? AND online = 1")
            .bind(id)
            .execute(&state.db)
            .await?
            .rows_affected();
        if changed == 0 {
            continue;
        }
        tracing::warn!(host = %name, silent_for = ts - last_seen, "host stopped reporting");
        let ev = sqlx::query(
            "INSERT INTO events (host_id, service, service_type, event_type, state, action, message, created_at, source)
             VALUES (?, NULL, NULL, ?, 1, 1, ?, ?, 'monarch')",
        )
        .bind(id)
        .bind(model::EVENT_HEARTBEAT)
        .bind(format!("No report from {name} for {}", human(ts - last_seen)))
        .bind(ts as f64)
        .execute(&state.db)
        .await?
        .last_insert_rowid();
        if let Some(summary) = views::host_summary_by_id(&state.db, id).await? {
            state.publish(json!({"type": "host", "host": summary}));
        }
        collector::publish_events(state, &[ev]).await?;
    }
    Ok(())
}

fn human(secs: i64) -> String {
    match secs {
        s if s < 120 => format!("{s} seconds"),
        s if s < 7200 => format!("{} minutes", s / 60),
        s => format!("{} hours", s / 3600),
    }
}

async fn retention(state: &SharedState) -> Result<()> {
    let r = state.settings.read().await.retention.clone();
    let ts = now();
    let db = &state.db;
    let raw = sqlx::query("DELETE FROM samples WHERE ts < ?")
        .bind(ts - r.raw_hours * 3600)
        .execute(db)
        .await?
        .rows_affected();
    let r5 = sqlx::query("DELETE FROM rollup_5m WHERE bucket < ?")
        .bind(ts - r.rollup_5m_days * 86400)
        .execute(db)
        .await?
        .rows_affected();
    let r1 = sqlx::query("DELETE FROM rollup_1h WHERE bucket < ?")
        .bind(ts - r.rollup_1h_days * 86400)
        .execute(db)
        .await?
        .rows_affected();
    let ev = sqlx::query("DELETE FROM events WHERE created_at < ?")
        .bind((ts - r.events_days * 86400) as f64)
        .execute(db)
        .await?
        .rows_affected();
    sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
        .bind(ts)
        .execute(db)
        .await?;
    // Series of services that no longer exist and have no data left.
    sqlx::query(
        "DELETE FROM series WHERE NOT EXISTS (SELECT 1 FROM services s WHERE s.host_id = series.host_id AND s.name = series.service)
           AND NOT EXISTS (SELECT 1 FROM rollup_1h r WHERE r.series_id = series.id)",
    )
    .execute(db)
    .await?;
    state.series.lock().unwrap().clear();
    sqlx::query("PRAGMA optimize").execute(db).await?;
    tracing::debug!(raw, r5, r1, ev, "retention pruned rows");
    Ok(())
}
