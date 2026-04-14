//! [NEXUS-ANALYTICS-01] Real-time on-chain analytics and metrics.
//! Inspired by top-tier systems like Glassnode for deep asset and network insight.

use axum::routing::get;
use axum::Router;
use crate::api::rest::AppState;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Deserialize)]
pub struct AnalyticsParams {
    pub asset: Option<String>,
    pub metric: String, // "tx_volume", "active_senders", "block_density"
    pub days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AnalyticsResponse {
    pub metric: String,
    pub asset: String,
    pub values: Vec<DataPoint>,
}

#[derive(Debug, Serialize)]
pub struct DataPoint {
    pub timestamp: i64,
    pub value: f64,
}

pub fn analytics_routes() -> Router<AppState> {
    Router::new().route("/metrics", get(get_analytics_metrics))
}

pub async fn get_analytics_metrics(
    State(state): State<AppState>,
    Query(params): Query<AnalyticsParams>,
) -> Result<Json<AnalyticsResponse>, StatusCode> {
    let asset = params.asset.unwrap_or_else(|| "STX".to_string());
    let days = params.days.unwrap_or(7);

    let mut values = Vec::new();

    match params.metric.as_str() {
        "tx_volume" => {
            let rows = sqlx::query(
                "SELECT DATE_TRUNC('day', created_at) as day, COUNT(*) as count
                 FROM stacks_transactions
                 WHERE created_at >= NOW() - INTERVAL '1 day' * $1
                 GROUP BY day ORDER BY day ASC"
            )
            .bind(days)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let day: chrono::DateTime<chrono::Utc> = row.get("day");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    timestamp: day.timestamp(),
                    value: count as f64,
                });
            }
        },
        "active_senders" => {
            let rows = sqlx::query(
                "SELECT DATE_TRUNC('day', created_at) as day, COUNT(DISTINCT sender) as count
                 FROM stacks_transactions
                 WHERE created_at >= NOW() - INTERVAL '1 day' * $1
                 GROUP BY day ORDER BY day ASC"
            )
            .bind(days)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let day: chrono::DateTime<chrono::Utc> = row.get("day");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    timestamp: day.timestamp(),
                    value: count as f64,
                });
            }
        },
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    Ok(Json(AnalyticsResponse {
        metric: params.metric,
        asset,
        values,
    }))
}
