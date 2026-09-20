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
pub struct DataPoint {
    pub timestamp: String,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct AnalyticsResponse {
    pub asset: String,
    pub metric: String,
    pub data: Vec<DataPoint>,
}

pub fn analytics_routes() -> Router<AppState> {
    Router::new().route("/metrics", get(get_metrics_handler))
}

pub async fn get_metrics_handler(
    State(state): State<AppState>,
    Query(params): Query<AnalyticsParams>,
) -> Result<Json<AnalyticsResponse>, StatusCode> {
    let asset = params.asset.unwrap_or_else(|| "STX".to_string());

    if asset != "STX" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let days = params.days.unwrap_or(7).clamp(1, 365);

    let mut values = Vec::new();

    match params.metric.as_str() {
        "tx_count" | "tx_volume" => {
            let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
                "SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') as day, COUNT(*) as count
                 FROM stacks_transactions
                 WHERE created_at >= NOW() - INTERVAL '1 day' * $1::int
                 GROUP BY 1 ORDER BY 1 ASC",
            )
            .bind(days as i32)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let day: String = row.get("day");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    timestamp: day,
                    value: count as f64,
                });
            }
        }
        "active_senders" => {
            let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
                "SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') as day, COUNT(DISTINCT sender) as count
                 FROM stacks_transactions
                 WHERE created_at >= NOW() - INTERVAL '1 day' * $1::int
                 GROUP BY 1 ORDER BY 1 ASC",
            )
            .bind(days as i32)
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let day: String = row.get("day");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    timestamp: day,
                    value: count as f64,
                });
            }
        }
        "whale_distribution" => {
            let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
                "SELECT
                    CASE
                        WHEN count >= 100 THEN 'Whale'
                        WHEN count >= 10 THEN 'Shark'
                        ELSE 'Shrimp'
                    END as tier,
                    COUNT(*) as count
                 FROM (
                    SELECT sender, COUNT(*) as count
                    FROM stacks_transactions
                    GROUP BY sender
                 ) as counts
                 GROUP BY 1",
            )
            .fetch_all(&state.storage.pg_pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            for row in rows {
                let tier: String = row.get("tier");
                let count: i64 = row.get("count");
                values.push(DataPoint {
                    timestamp: tier,
                    value: count as f64,
                });
            }
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    Ok(Json(AnalyticsResponse {
        asset,
        metric: params.metric,
        data: values,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rest::AppState;
    use crate::config::Config;
    use crate::executor::NexusExecutor;
    use crate::state::NexusState;
    use crate::storage::tableland::TablelandAdapter;
    use crate::storage::Storage;
    use axum::extract::{Query, State};
    use std::collections::HashSet;
    use std::sync::Arc;

    fn mock_app_state() -> AppState {
        let config = Arc::new(Config::default_test());
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let nexus_state = Arc::new(NexusState::new());
        let executor = Arc::new(NexusExecutor::new(
            storage.clone(),
            crate::executor::rgb::RGBRolloutMode::Disabled,
            HashSet::new(),
        ));
        let tableland = Arc::new(TablelandAdapter::new(storage.clone(), "test".to_string()));

        AppState {
            config,
            storage,
            nexus_state,
            executor,
            oracle: None,
            tableland,
            kwil: None,
            nostr: None,
            gateway_url: None,
            http_client: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn test_get_metrics_rejects_non_stx_asset() {
        let state = mock_app_state();
        let params = AnalyticsParams {
            asset: Some("BTC".to_string()),
            metric: "tx_count".to_string(),
            days: Some(7),
        };

        let result = get_metrics_handler(State(state), Query(params)).await;
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_metrics_rejects_invalid_metric() {
        let state = mock_app_state();
        let params = AnalyticsParams {
            asset: Some("STX".to_string()),
            metric: "invalid_metric_name".to_string(),
            days: Some(7),
        };

        let result = get_metrics_handler(State(state), Query(params)).await;
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }
}
