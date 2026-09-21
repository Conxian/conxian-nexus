//! [CON-62/72/CON-803] Bitcoin DLC Bond Orchestrator & CET Verification.
//! Finalizes lifecycle contracts, Oracle attestation verification, and CET outcomes for Bitcoin-native DLC bonds.

use crate::api::rest::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use k256::schnorr::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

#[derive(Debug, Deserialize)]
pub struct DlcCetOutcomeRequest {
    pub dlc_contract_id: String,
    pub oracle_pubkey: String,
    pub attestation_signature: String,
    pub outcome_value: u64,
    pub total_principal_sbtc: u64,
}

#[derive(Debug, Serialize)]
pub struct DlcCetOutcomeResponse {
    pub dlc_contract_id: String,
    pub verified: bool,
    pub cet_status: String,
    pub payout_sbtc_investor: u64,
    pub payout_sbtc_issuer: u64,
}

fn validate_dlc_request(payload: &DlcBondRequest) -> Result<(), &'static str> {
    if payload.bond_id.trim().is_empty() {
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

/// Cryptographically verifies a BIP-340 Schnorr signature from a DLC Oracle over a 32-byte digest.
pub fn verify_dlc_oracle_attestation(
    oracle_pubkey_hex: &str,
    digest: &[u8; 32],
    signature_hex: &str,
) -> Result<bool, &'static str> {
    let pk_clean = oracle_pubkey_hex.trim_start_matches("0x");
    let sig_clean = signature_hex.trim_start_matches("0x");

    let pk_bytes = hex::decode(pk_clean).map_err(|_| "invalid oracle_pubkey hex")?;
    let sig_bytes = hex::decode(sig_clean).map_err(|_| "invalid attestation_signature hex")?;

    if sig_bytes.len() != 64 {
        return Err("signature must be 64 bytes");
    }

    let verifying_key = if pk_bytes.len() == 32 {
        let key_array: [u8; 32] = pk_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "invalid key array")?;
        VerifyingKey::from_bytes(&key_array.into()).map_err(|_| "invalid 32-byte BIP-340 key")?
    } else if pk_bytes.len() == 33 {
        let xonly_slice = &pk_bytes[1..33];
        let key_array: [u8; 32] = xonly_slice.try_into().map_err(|_| "invalid key slice")?;
        VerifyingKey::from_bytes(&key_array.into()).map_err(|_| "invalid 33-byte SEC1 key")?
    } else {
        return Err("oracle_pubkey must be 32 bytes (XOnly) or 33 bytes (SEC1)");
    };

    let signature = Signature::try_from(sig_bytes.as_slice())
        .map_err(|_| "invalid Schnorr signature encoding")?;

    verifying_key
        .verify_raw(digest, &signature)
        .map(|_| true)
        .map_err(|_| "BIP-340 Schnorr signature verification failed")
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

    // 2. Generate DLC Announcement using the Nexus-owned compatibility signer.
    let announcement_data = build_announcement_data(&payload);
    let oracle_announcement = match sign_announcement_with(
        &announcement_data,
        crate::compat::core_bridge::sign_transaction,
    ) {
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
            next_coupon_height: calculate_next_coupon_height(payload.expiry_height),
        }),
    )
        .into_response()
}

/// [NEXUS-DLC-02] Verifies DLC Contract Execution Transaction (CET) outcome and Oracle attestation.
pub async fn verify_dlc_cet_outcome_handler(
    Json(payload): Json<DlcCetOutcomeRequest>,
) -> impl IntoResponse {
    if payload.dlc_contract_id.trim().is_empty()
        || payload.oracle_pubkey.trim().is_empty()
        || payload.attestation_signature.trim().is_empty()
        || payload.total_principal_sbtc == 0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(DlcCetOutcomeResponse {
                dlc_contract_id: payload.dlc_contract_id,
                verified: false,
                cet_status: "Rejected".to_string(),
                payout_sbtc_investor: 0,
                payout_sbtc_issuer: 0,
            }),
        )
            .into_response();
    }

    // Compute canonical outcome digest: SHA-256("dlc_cet_outcome:<dlc_contract_id>:<outcome_value>")
    let msg = format!(
        "dlc_cet_outcome:{}:{}",
        payload.dlc_contract_id, payload.outcome_value
    );
    let digest: [u8; 32] = Sha256::digest(msg.as_bytes()).into();

    let is_valid = match verify_dlc_oracle_attestation(
        &payload.oracle_pubkey,
        &digest,
        &payload.attestation_signature,
    ) {
        Ok(valid) => valid,
        Err(err) => {
            tracing::warn!("DLC CET Oracle attestation verification failed: {}", err);
            false
        }
    };

    if !is_valid {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(DlcCetOutcomeResponse {
                dlc_contract_id: payload.dlc_contract_id,
                verified: false,
                cet_status: "InvalidAttestation".to_string(),
                payout_sbtc_investor: 0,
                payout_sbtc_issuer: 0,
            }),
        )
            .into_response();
    }

    // Calculate CET Payout Distribution:
    // If outcome_value >= 100 -> Investor receives 100% of principal.
    // If outcome_value == 0   -> Issuer receives 100% of principal (default/short).
    // Otherwise               -> Proportional payout based on outcome_value percentage (bounded at 100%).
    let pct = payload.outcome_value.min(100);
    let payout_investor = (payload.total_principal_sbtc * pct) / 100;
    let payout_issuer = payload.total_principal_sbtc.saturating_sub(payout_investor);

    (
        StatusCode::OK,
        Json(DlcCetOutcomeResponse {
            dlc_contract_id: payload.dlc_contract_id,
            verified: true,
            cet_status: "VerifiedSettled".to_string(),
            payout_sbtc_investor: payout_investor,
            payout_sbtc_issuer: payout_issuer,
        }),
    )
        .into_response()
}

pub fn dlc_routes() -> Router<AppState> {
    Router::new()
        .route("/bond", post(create_dlc_bond_handler))
        .route("/cet/verify", post(verify_dlc_cet_outcome_handler))
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
    use axum::response::Response;
    use k256::schnorr::SigningKey;
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

        assert_eq!(validate_dlc_request(&request), Err("bond_id is required"));
    }

    #[test]
    fn test_validate_dlc_request_rejects_whitespace_bond_id() {
        let request = DlcBondRequest {
            bond_id: "   ".to_string(),
            principal_sbtc: 1,
            expiry_height: 100,
            coupon_rate: 0.05,
        };

        assert_eq!(validate_dlc_request(&request), Err("bond_id is required"));
    }

    #[test]
    fn test_validate_dlc_request_rejects_zero_principal() {
        let request = DlcBondRequest {
            bond_id: "bond-1".to_string(),
            principal_sbtc: 0,
            expiry_height: 100,
            coupon_rate: 0.05,
        };

        assert_eq!(
            validate_dlc_request(&request),
            Err("principal_sbtc must be greater than zero")
        );
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

    #[test]
    fn test_verify_dlc_oracle_attestation_valid_signature() {
        let signing_key = SigningKey::from_bytes(&[0x07u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();

        let msg = "dlc_cet_outcome:dlc_123:100";
        let digest: [u8; 32] = Sha256::digest(msg.as_bytes()).into();
        let aux_rand = [0x02u8; 32];
        let signature = signing_key.sign_raw(&digest, &aux_rand).unwrap();

        let pubkey_hex = hex::encode(verifying_key.to_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        let res = verify_dlc_oracle_attestation(&pubkey_hex, &digest, &sig_hex);
        assert_eq!(res, Ok(true));
    }

    #[test]
    fn test_verify_dlc_oracle_attestation_invalid_signature() {
        let signing_key = SigningKey::from_bytes(&[0x07u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();

        let msg = "dlc_cet_outcome:dlc_123:100";
        let digest: [u8; 32] = Sha256::digest(msg.as_bytes()).into();
        let pubkey_hex = hex::encode(verifying_key.to_bytes());
        let bad_sig_hex = hex::encode([0x01u8; 64]);

        let res = verify_dlc_oracle_attestation(&pubkey_hex, &digest, &bad_sig_hex);
        assert_eq!(res, Err("BIP-340 Schnorr signature verification failed"));
    }

    #[test]
    fn test_verify_dlc_oracle_attestation_invalid_key_length() {
        let digest = [0x00u8; 32];
        let res = verify_dlc_oracle_attestation("010203", &digest, &hex::encode([0x00u8; 64]));
        assert_eq!(
            res,
            Err("oracle_pubkey must be 32 bytes (XOnly) or 33 bytes (SEC1)")
        );
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

    async fn response_json(response: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
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
        let body = response_json(response).await;
        assert_eq!(body["status"], "Error");
        assert_eq!(body["dlc_contract_id"], "");
        assert_eq!(body["next_coupon_height"], 0);
    }

    #[tokio::test]
    async fn test_verify_dlc_cet_outcome_handler_success() {
        let signing_key = SigningKey::from_bytes(&[0x09u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();

        let contract_id = "dlc_bond_999";
        let outcome_value = 80u64;
        let total_principal = 100_000u64;

        let msg = format!("dlc_cet_outcome:{}:{}", contract_id, outcome_value);
        let digest: [u8; 32] = Sha256::digest(msg.as_bytes()).into();
        let aux_rand = [0x03u8; 32];
        let signature = signing_key.sign_raw(&digest, &aux_rand).unwrap();

        let request = DlcCetOutcomeRequest {
            dlc_contract_id: contract_id.to_string(),
            oracle_pubkey: hex::encode(verifying_key.to_bytes()),
            attestation_signature: hex::encode(signature.to_bytes()),
            outcome_value,
            total_principal_sbtc: total_principal,
        };

        let response = verify_dlc_cet_outcome_handler(Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["verified"], true);
        assert_eq!(body["cet_status"], "VerifiedSettled");
        assert_eq!(body["payout_sbtc_investor"], 80_000);
        assert_eq!(body["payout_sbtc_issuer"], 20_000);
    }

    #[tokio::test]
    async fn test_verify_dlc_cet_outcome_handler_invalid_attestation() {
        let request = DlcCetOutcomeRequest {
            dlc_contract_id: "dlc_bond_100".to_string(),
            oracle_pubkey: hex::encode([0x02u8; 32]),
            attestation_signature: hex::encode([0x00u8; 64]),
            outcome_value: 50,
            total_principal_sbtc: 10_000,
        };

        let response = verify_dlc_cet_outcome_handler(Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response_json(response).await;
        assert_eq!(body["verified"], false);
        assert_eq!(body["cet_status"], "InvalidAttestation");
    }

    #[tokio::test]
    async fn test_verify_dlc_cet_outcome_handler_bad_request() {
        let request = DlcCetOutcomeRequest {
            dlc_contract_id: "".to_string(),
            oracle_pubkey: "".to_string(),
            attestation_signature: "".to_string(),
            outcome_value: 0,
            total_principal_sbtc: 0,
        };

        let response = verify_dlc_cet_outcome_handler(Json(request))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
