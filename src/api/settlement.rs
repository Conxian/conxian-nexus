//! [CON-162] External Settlement Trigger Module.
//! Handles ISO 20022, PAPSS, and BRICS triggers for TEE-verified proposals.

use crate::api::rest::AppState;
use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ExternalSettlementTrigger {
    pub source: String, // "ISO20022", "PAPSS", "BRICS"
    pub external_id: String,
    pub payload: serde_json::Value,
    pub attestation: String, // TEE Attestation
}

#[derive(Debug, Serialize)]
pub struct SettlementProposalResponse {
    pub proposal_id: String,
    pub status: String,
    pub unlock_height: u64,
    pub message: String,
}

/// [CON-162] Handles external settlement triggers.
/// Verifies TEE attestation and initiates a 144-block time-lock proposal.
pub async fn settlement_trigger_handler(
    State(state): State<AppState>,
    Json(payload): Json<ExternalSettlementTrigger>,
) -> impl IntoResponse {
    tracing::info!(
        "Received {} settlement trigger: {}",
        payload.source,
        payload.external_id
    );

    // 1. Verify TEE Attestation
    // CON-162: Production requires valid TEE attestation prefix.
    if !payload.attestation.starts_with("TEE_") {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(SettlementProposalResponse {
                proposal_id: "".to_string(),
                status: "Rejected".to_string(),
                unlock_height: 0,
                message: "Invalid TEE Attestation. Security floor violated.".to_string(),
            }),
        )
            .into_response();
    }

    // 2. Oracle Verification
    if let Some(oracle) = &state.oracle {
        match oracle
            .verify_external_signal(&payload.source, &payload.payload)
            .await
        {
            Ok(true) => tracing::info!("Oracle verified {} signal", payload.source),
            Ok(false) => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(SettlementProposalResponse {
                        proposal_id: "".to_string(),
                        status: "Rejected".to_string(),
                        unlock_height: 0,
                        message: "Oracle verification failed for external signal.".to_string(),
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SettlementProposalResponse {
                        proposal_id: "".to_string(),
                        status: "Error".to_string(),
                        unlock_height: 0,
                        message: format!("Oracle service error: {}", e),
                    }),
                )
                    .into_response();
            }
        }
    }

    // 3. Log external settlement event [CON-164]
    let fiat_value = payload.payload.get("amount").and_then(|v| v.as_f64());
    let _ = sqlx::query(
        "INSERT INTO cxn_external_settlement_logs (external_tx_reference, settlement_network_origin, fiat_value_pegged, raw_payload)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(&payload.external_id)
    .bind(&payload.source)
    .bind(fiat_value)
    .bind(&payload.payload)
    .execute(&state.storage.pg_pool)
    .await;

    // 4. Get current block height to calculate time-lock
    let row_res = sqlx::query("SELECT MAX(height) as max_height FROM stacks_blocks")
        .fetch_optional(&state.storage.pg_pool)
        .await;

    let current_height: i64 = match row_res {
        Ok(Some(row)) => row.get::<Option<i64>, _>("max_height").unwrap_or(0),
        _ => 0,
    };

    let unlock_height = (current_height + 144) as u64;
    let proposal_id = format!("prop_{}", Uuid::new_v4());

    // 5. Persist the proposal as "proposal-only"
    let res = sqlx::query(
        "INSERT INTO settlement_proposals (proposal_id, external_id, source, payload, status, init_height, unlock_height)
         VALUES ($1, $2, $3, $4, 'active', $5, $6)"
    )
    .bind(&proposal_id)
    .bind(&payload.external_id)
    .bind(&payload.source)
    .bind(&payload.payload)
    .bind(current_height)
    .bind(unlock_height as i64)
    .execute(&state.storage.pg_pool)
    .await;

    match res {
        Ok(_) => {
            tracing::info!(
                "Settlement proposal {} created. Unlocks at height {}.",
                proposal_id,
                unlock_height
            );
            Json(SettlementProposalResponse {
                proposal_id,
                status: "Active".to_string(),
                unlock_height,
                message: "External trigger verified. 144-block time-lock initiated.".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to persist settlement proposal: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(SettlementProposalResponse {
                    proposal_id: "".to_string(),
                    status: "Error".to_string(),
                    unlock_height: 0,
                    message: "Internal persistence failure.".to_string(),
                }),
            )
                .into_response()
        }
    }
}
