//! [CON-62/72] Bitcoin DLC Bond Orchestrator.
//! Finalizes lifecycle contracts for Bitcoin-native DLC bonds.

use crate::api::rest::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct DlcBondRequest {
    pub organization_id: String,
    pub bond_id: String,
    pub principal_sbtc: u64,
    pub expiry_height: u64,
    pub coupon_rate: f64, // e.g. 0.045 for 4.5%
}

#[derive(Debug, Serialize)]
pub struct DlcBondResponse {
    pub dlc_contract_id: String,
    pub status: String,
    pub oracle_announcement: String,
    pub next_coupon_height: u64,
}

/// [NEXUS-DLC-01] DLC creation and management logic.
/// Anchors Bitcoin-native DLC bonds to Stacks/sBTC lifecycle.
pub async fn create_dlc_bond_handler(
    State(state): State<AppState>,
    Json(payload): Json<DlcBondRequest>,
) -> impl IntoResponse {
    tracing::info!("Creating DLC bond for id {} (Org: {})", payload.bond_id, payload.organization_id);

    // 1. Validation
    if payload.bond_id.is_empty() || payload.principal_sbtc == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(DlcBondResponse {
                dlc_contract_id: "".to_string(),
                status: "Error".to_string(),
                oracle_announcement: "".to_string(),
                next_coupon_height: 0,
            }),
        )
            .into_response();
    }

    // 2. Generate DLC Announcement (using lib-conxian-core signature logic)
    let announcement_data = format!("dlc_bond_init:{}:{}:{}", payload.bond_id, payload.principal_sbtc, payload.expiry_height);
    let oracle_announcement = match lib_conxian_core::sign_transaction(&announcement_data) {
        Ok(sig) => sig,
        Err(e) => {
            tracing::error!("Failed to sign DLC announcement: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Signing Error").into_response();
        }
    };

    let dlc_contract_id = format!("dlc_{}", Uuid::new_v4());

    // 3. Persist Bond State
    let mut conn = match state.storage.redis_client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Redis Error").into_response(),
    };

    let _: () = redis::cmd("HSET")
        .arg(format!("dlc_bond:{}", dlc_contract_id))
        .arg("org_id").arg(&payload.organization_id)
        .arg("bond_id").arg(&payload.bond_id)
        .arg("principal").arg(payload.principal_sbtc)
        .arg("status").arg("Initialized")
        .arg("announcement").arg(&oracle_announcement)
        .query_async(&mut conn).await.unwrap_or(());

    // 4. Return initialized bond details
    (
        StatusCode::CREATED,
        Json(DlcBondResponse {
            dlc_contract_id,
            status: "Initialized".to_string(),
            oracle_announcement,
            next_coupon_height: payload.expiry_height / 10, // Deterministic coupon interval
        }),
    )
        .into_response()
}
