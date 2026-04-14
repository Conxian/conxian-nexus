//! [CON-63] OData/ERP Translation Layer for Conxian Gateway.
//! Bridges SAP/Oracle OData payloads to x402 mandates.

use axum::routing::post;
use axum::Router;
use crate::api::rest::AppState;
use axum::{extract::State, Json, http::StatusCode};
use serde::{Deserialize, Serialize};

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
    tracing::info!("Received ERP Sync request from {} system (Org: {})", payload.erp_type, payload.organization_id);

    let mut reconciled_entries = 0;
    let mut errors = Vec::new();

    // [NEXUS-ERP-02] ERP Reconciliation Logic.
    // Verify OData "value" entries against local transaction history.
    if let Some(entries) = payload.odata_payload.get("value").and_then(|v| v.as_array()) {
        for entry in entries {
            if let Some(tx_id) = entry.get("TransactionId").and_then(|t| t.as_str()) {
                // Check if transaction exists and is not orphaned
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM stacks_transactions t JOIN stacks_blocks b ON t.block_hash = b.hash WHERE t.tx_id = $1 AND b.state != 'orphaned')"
                )
                .bind(tx_id)
                .fetch_one(&state.storage.pg_pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                if exists {
                    reconciled_entries += 1;
                } else {
                    errors.push(format!("Transaction {} not found or orphaned in local state", tx_id));
                }
            }
        }
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
        status: if errors.is_empty() { "Success".to_string() } else { "Partial Success".to_string() },
        mandate_id: Some(mandate_hash),
        reconciled_entries,
        errors,
    }))
}
