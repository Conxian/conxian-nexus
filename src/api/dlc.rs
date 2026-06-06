//! [CON-62/72] Bitcoin DLC Bond Orchestrator.
//! Finalizes lifecycle contracts for Bitcoin-native DLC bonds.

use crate::api::rest::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct DlcBondRequest {
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

fn validate_dlc_request(payload: &DlcBondRequest) -> Result<(), &'static str> {
    if payload.bond_id.is_empty() {
        return Err("bond_id is required");
    }

    if payload.principal_sbtc == 0 {
        return Err("principal_sbtc must be greater than zero");
    }

    Ok(())
}

fn build_announcement_data(payload: &DlcBondRequest) -> String {
    format!(
        "dlc_bond_init:{}:{}:{}",
        payload.bond_id, payload.principal_sbtc, payload.expiry_height
    )
}

fn calculate_next_coupon_height(expiry_height: u64) -> u64 {
    expiry_height / 10
}

fn sign_announcement_with<F, E>(announcement_data: &str, signer: F) -> Result<String, String>
where
    F: FnOnce(&str) -> Result<String, E>,
    E: std::fmt::Display,
{
    signer(announcement_data).map_err(|e| e.to_string())
}

/// [NEXUS-DLC-01] DLC creation and management logic.
/// Anchors Bitcoin-native DLC bonds to Stacks/sBTC lifecycle.
pub async fn create_dlc_bond_handler(
    State(state): State<AppState>,
    Json(payload): Json<DlcBondRequest>,
) -> impl IntoResponse {
    tracing::info!("Creating DLC bond for id {}", payload.bond_id);

    // 1. Validation
    if validate_dlc_request(&payload).is_err() {
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
    let announcement_data = build_announcement_data(&payload);
    let oracle_announcement =
        match sign_announcement_with(&announcement_data, lib_conxian_core::sign_transaction) {
            Ok(sig) => sig,
            Err(e) => {
                tracing::error!("Failed to sign DLC announcement: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Signing Error").into_response();
            }
        };

    let dlc_contract_id = format!("dlc_{}", Uuid::new_v4());

    // 3. Persist Bond State
    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Redis Error").into_response(),
    };

    let _: () = redis::cmd("HSET")
        .arg(format!("dlc_bond:{}", dlc_contract_id))
        .arg("bond_id")
        .arg(&payload.bond_id)
        .arg("principal")
        .arg(payload.principal_sbtc)
        .arg("status")
        .arg("Initialized")
        .arg("announcement")
        .arg(&oracle_announcement)
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    // 4. Return initialized bond details
    (
        StatusCode::CREATED,
        Json(DlcBondResponse {
            dlc_contract_id,
            status: "Initialized".to_string(),
            oracle_announcement,
            next_coupon_height: calculate_next_coupon_height(payload.expiry_height), // Mocked coupon interval
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::rest::AppState;
    use crate::config::Config;
    use crate::executor::rgb::RGBRolloutMode;
    use crate::executor::NexusExecutor;
    use crate::state::NexusState;
    use crate::storage::tableland::TablelandAdapter;
    use crate::storage::Storage;
    use axum::extract::State;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[test]
    fn test_validate_dlc_request_rejects_empty_bond_id() {
        let request = DlcBondRequest {
            bond_id: "".to_string(),
            principal_sbtc: 1,
            expiry_height: 100,
            coupon_rate: 0.05,
        };

        assert!(validate_dlc_request(&request).is_err());
    }

    #[test]
    fn test_validate_dlc_request_rejects_zero_principal() {
        let request = DlcBondRequest {
            bond_id: "bond-1".to_string(),
            principal_sbtc: 0,
            expiry_height: 100,
            coupon_rate: 0.05,
        };

        assert!(validate_dlc_request(&request).is_err());
    }

    #[test]
    fn test_validate_dlc_request_accepts_valid_payload() {
        let request = DlcBondRequest {
            bond_id: "bond-1".to_string(),
            principal_sbtc: 1,
            expiry_height: 100,
            coupon_rate: 0.05,
        };

        assert!(validate_dlc_request(&request).is_ok());
    }

    #[test]
    fn test_build_announcement_data_formats_payload() {
        let request = DlcBondRequest {
            bond_id: "bond-1".to_string(),
            principal_sbtc: 42,
            expiry_height: 2100,
            coupon_rate: 0.05,
        };

        let announcement = build_announcement_data(&request);
        assert_eq!(announcement, "dlc_bond_init:bond-1:42:2100");
    }

    #[test]
    fn test_calculate_next_coupon_height() {
        assert_eq!(calculate_next_coupon_height(2200), 220);
    }

    #[test]
    fn test_sign_announcement_with_success() {
        let result =
            sign_announcement_with("payload", |_| Ok::<String, &'static str>("sig".to_string()));
        assert_eq!(result, Ok("sig".to_string()));
    }

    #[test]
    fn test_sign_announcement_with_error() {
        let result = sign_announcement_with("payload", |_| Err::<String, _>("boom"));
        assert_eq!(result, Err("boom".to_string()));
    }

    fn build_test_state(storage: Arc<Storage>) -> AppState {
        let config = Arc::new(Config::default_test());
        let nexus_state = Arc::new(NexusState::new());
        let executor = Arc::new(NexusExecutor::new(
            storage.clone(),
            RGBRolloutMode::Disabled,
            HashSet::new(),
        ));
        let tableland = Arc::new(TablelandAdapter::new(
            storage.clone(),
            config.tableland_base_url.clone(),
        ));

        AppState {
            storage,
            nexus_state,
            executor,
            oracle: None,
            tableland,
            kwil: None,
            nostr: None,
            gateway_url: None,
            http_client: reqwest::Client::new(),
            config,
        }
    }

    fn test_state() -> AppState {
        build_test_state(Storage::for_tests())
    }

    #[tokio::test]
    async fn test_create_dlc_bond_handler_rejects_invalid_payload() {
        let state = test_state();
        let request = DlcBondRequest {
            bond_id: "".to_string(),
            principal_sbtc: 0,
            expiry_height: 100,
            coupon_rate: 0.05,
        };

        let response = create_dlc_bond_handler(State(state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_dlc_bond_handler_returns_redis_error_when_connection_fails() {
        let storage = Arc::new(
            Storage::new_lazy("postgres://localhost/nexus", "redis://127.0.0.1:1/")
                .expect("lazy test storage should be constructible"),
        );
        let state = build_test_state(storage);

        let request = DlcBondRequest {
            bond_id: "bond-1".to_string(),
            principal_sbtc: 100,
            expiry_height: 500,
            coupon_rate: 0.05,
        };

        let response = create_dlc_bond_handler(State(state), Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
