use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, ApiResult};
use crate::{
    auth::User,
    state::{SharedState, now},
};

#[derive(Deserialize)]
pub struct MetricsQuery {
    host: i64,
    service: String,
    metrics: String,
    from: Option<i64>,
    to: Option<i64>,
}

pub async fn query(State(state): State<SharedState>, _user: User, Query(q): Query<MetricsQuery>) -> ApiResult<Json<Value>> {
    let to = q.to.unwrap_or_else(now);
    let from = q.from.unwrap_or(to - 6 * 3600);
    if from >= to {
        return Err(ApiError::bad_request("from must be before to"));
    }
    let span = to - from;
    let (table, resolution) = match span {
        s if s <= 6 * 3600 => ("samples", 0),
        s if s <= 7 * 86400 => ("rollup_5m", 300),
        _ => ("rollup_1h", 3600),
    };
    let mut series = Vec::new();
    for metric in q.metrics.split(',').map(str::trim).filter(|m| !m.is_empty()).take(16) {
        let points: Vec<Value> = if resolution == 0 {
            sqlx::query_as::<_, (i64, f64)>(
                "SELECT s.ts, s.value FROM samples s JOIN series r ON r.id = s.series_id
                 WHERE r.host_id = ? AND r.service = ? AND r.metric = ? AND s.ts BETWEEN ? AND ?
                 ORDER BY s.ts",
            )
            .bind(q.host)
            .bind(&q.service)
            .bind(metric)
            .bind(from)
            .bind(to)
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|(ts, v)| json!([ts, v, null, null]))
            .collect()
        } else {
            sqlx::query_as::<_, (i64, f64, i64, f64, f64)>(&format!(
                "SELECT b.bucket, b.sum, b.count, b.min, b.max FROM {table} b JOIN series r ON r.id = b.series_id
                 WHERE r.host_id = ? AND r.service = ? AND r.metric = ? AND b.bucket BETWEEN ? AND ?
                 ORDER BY b.bucket"
            ))
            .bind(q.host)
            .bind(&q.service)
            .bind(metric)
            .bind(from - resolution)
            .bind(to)
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|(ts, sum, count, min, max)| json!([ts, sum / count.max(1) as f64, min, max]))
            .collect()
        };
        series.push(json!({"metric": metric, "points": points}));
    }
    Ok(Json(json!({"resolution": resolution, "series": series})))
}
