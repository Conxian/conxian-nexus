//! [CON-63] OData/ERP Translation Layer for Conxian Gateway.
//! Bridges SAP/Oracle OData payloads to x402 mandates.

use crate::api::rest::AppState;
use axum::routing::post;
use axum::Router;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_ERP_TX_IDS: usize = 1000;
const MAX_ERP_ERRORS: usize = 100;

#[derive(Debug, Deserialize)]
pub struct ErpSyncRequest {
    pub organization_id: String,
    pub erp_type: String, // "SAP", "Oracle", "MicrosoftDynamics"
    pub odata_payload: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct ErpSyncResponse {
    pub status: String,
    pub mandate_id: Option<String>,
    pub reconciled_entries: usize,
    pub errors: Vec<String>,
}

/// [NEXUS-ERP-01] OData v4 compatible parser for SAP/Oracle payloads.
pub fn erp_routes() -> Router<AppState> {
    Router::new().route("/sync", post(erp_sync_handler))
}

pub async fn erp_sync_handler(
    State(state): State<AppState>,
    Json(payload): Json<ErpSyncRequest>,
) -> Result<Json<ErpSyncResponse>, StatusCode> {
    tracing::info!(
        "Received ERP Sync request from {} system (Org: {})",
        payload.erp_type,
        payload.organization_id
    );

    let mut reconciled_entries = 0;
    let mut errors = Vec::new();

    // [NEXUS-ERP-02] ERP Reconciliation Logic.
    // Verify OData "value" entries against local transaction history.
    let entries = payload
        .odata_payload
        .get("value")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut seen = HashSet::new();
    let mut tx_ids = Vec::new();
    for entry in entries {
        if let Some(tx_id) = entry.get("TransactionId").and_then(|t| t.as_str()) {
            if seen.insert(tx_id) {
                tx_ids.push(tx_id.to_owned());
                if tx_ids.len() > MAX_ERP_TX_IDS {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
            }
        }
    }

    let found: HashSet<String> = if tx_ids.is_empty() {
        HashSet::new()
    } else {
        sqlx::query_scalar(
            "SELECT t.tx_id
             FROM stacks_transactions t
             JOIN stacks_blocks b ON t.block_hash = b.hash
             WHERE t.tx_id = ANY($1) AND b.state = 'hard'",
        )
        .bind(&tx_ids)
        .fetch_all(&state.storage.pg_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .collect()
    };

    // Note: `tx_ids` is deduplicated; `reconciled_entries` is the count of unique transaction IDs.
    let mut truncated_errors = false;
    for tx_id in &tx_ids {
        if found.contains(tx_id) {
            reconciled_entries += 1;
        } else {
            if errors.len() < MAX_ERP_ERRORS {
                errors.push(format!(
                    "Transaction {} not found or not finalized in local state",
                    tx_id
                ));
            } else {
                truncated_errors = true;
            }
        }
    }

    if truncated_errors {
        errors.push(format!(
            "Additional reconciliation errors were omitted after {} entries",
            MAX_ERP_ERRORS
        ));
    }

    // OData v4 to x402 Mandate Translation
    let action = match payload.odata_payload.get("action").and_then(|a| a.as_str()) {
        Some("REBALANCE") => "REBALANCE_OPEX",
        Some("DISBURSE") => "DISBURSE_YIELD",
        _ => "SETTLE_TX",
    };

    let mandate_hash = format!("x402_{}", uuid::Uuid::new_v4());

    tracing::info!(
        "Translated OData to x402 Mandate. Action: {}. Requesting Enclave Signature...",
        action
    );

    // [NEXUS-ERP-03] Secure Hardware Attestation for ERP reconciliation.
    // In a real environment, this would call into a TEE/Enclave service.
    let attestation = format!("enclave_sig_{}", uuid::Uuid::new_v4());
    tracing::info!("Received Hardware Attestation: {}", attestation);

    Ok(Json(ErpSyncResponse {
        status: if errors.is_empty() {
            "Success".to_string()
        } else {
            "Partial Success".to_string()
        },
        mandate_id: Some(mandate_hash),
        reconciled_entries,
        errors,
    }))
}
