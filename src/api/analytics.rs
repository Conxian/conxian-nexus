//! [NEXUS-ANALYTICS-01] Real-time on-chain analytics and metrics.
//! Inspired by top-tier systems like Glassnode for deep asset and network insight.

use crate::api::rest::AppState;
use axum::routing::get;
use axum::Router;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Deserialize)]
pub struct AnalyticsParams {
    pub asset: Option<String>,
    pub metric: String, // "tx_count" (alias: "tx_volume"), "active_senders", "whale_distribution"
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
    pub label: String,
    pub value: f64,
}

pub fn analytics_routes() -> Router<AppState> {
    Router::new().route("/metrics", get(get_analytics_metrics))
}

pub async fn get_analytics_metrics(
    State(state): State<AppState>,
    Query(params): Query<AnalyticsParams>,
) -> Result<Json<AnalyticsResponse>, StatusCode> {
    let asset = params
        .asset
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("STX")
        .to_uppercase();

    if asset != "STX" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let days = params.days.unwrap_or(7).clamp(1, 365);

    let mut values = Vec::new();

    let metric = match params.metric.as_str() {
        "tx_count" | "tx_volume" => {
            let rows = sqlx::query(
                "SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') as day, COUNT(*) as count
                 FROM stacks_transactions
                 WHERE created_at >= NOW() - INTERVAL '1 day' * $1::int
                 GROUP BY 1 ORDER BY 1 ASC",
            )
            .bind(days)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let day: String = row.get("day");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    label: day,
                    value: count as f64,
                });
            }

            "tx_count"
        }
        "active_senders" => {
            let rows = sqlx::query(
                "SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') as day, COUNT(DISTINCT sender) as count
                 FROM stacks_transactions
                 WHERE created_at >= NOW() - INTERVAL '1 day' * $1::int
                 GROUP BY 1 ORDER BY 1 ASC",
            )
            .bind(days)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let day: String = row.get("day");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    label: day,
                    value: count as f64,
                });
            }

            "active_senders"
        }
        "whale_distribution" => {
            let rows = sqlx::query(
                "SELECT
                    CASE
                        WHEN count >= 100 THEN 'Whale'
                        WHEN count >= 20 THEN 'Dolphin'
                        ELSE 'Shrimp'
                    END as tier,
                    COUNT(*) as entity_count
                 FROM (
                    SELECT sender, COUNT(*) as count
                    FROM stacks_transactions
                    WHERE created_at >= NOW() - INTERVAL '1 day' * $1::int
                    GROUP BY sender
                 ) as entity_stats
                 GROUP BY tier",
            )
            .bind(days)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let tier: String = row.get("tier");
                let count: i64 = row.get("entity_count");
                values.push(DataPoint {
                    label: tier,
                    value: count as f64,
                });
            }

            "whale_distribution"
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    Ok(Json(AnalyticsResponse {
        metric: metric.to_string(),
        asset,
        values,
    }))
}
