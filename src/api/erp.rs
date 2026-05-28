//! [CON-63] OData/ERP Translation Layer for Conxian Nexus.
//! Bridges SAP/Oracle OData payloads to x402 mandates.

use crate::api::rest::AppState;
use axum::routing::post;
use axum::Router;
use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

const MAX_ERP_TX_IDS: usize = 1000;
const ERP_ATTESTATION_REPLAY_PREFIX: &str = "nexus:erp:attestation:nonce:v1";
const ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS: i64 = 60;
const ERP_ATTESTATION_MAX_LIFETIME_SECONDS: i64 = 600;

#[derive(Debug, Deserialize, Clone)]
pub struct ErpAttestation {
    pub key_id: String,
    pub nonce: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
pub struct ErpSyncRequest {
    pub organization_id: String,
    pub erp_type: String, // "SAP", "Oracle", "MicrosoftDynamics"
    pub odata_payload: serde_json::Value,
    pub timestamp: i64,
    pub attestation: ErpAttestation,
}

#[derive(Debug, Serialize)]
pub struct ErpSyncResponse {
    pub status: String,
    pub mandate_id: Option<String>,
    pub reconciled_entries: usize,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct VerifiedErpAttestation {
    attestation_id: String,
    replay_key: String,
    replay_ttl_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErpAttestationError {
    Misconfigured,
    InvalidAttestationFormat,
    InvalidSignature,
    ExpiredAttestation,
    ReplayDetected,
}

impl ErpAttestationError {
    fn status_code(self) -> StatusCode {
        match self {
            Self::Misconfigured => StatusCode::SERVICE_UNAVAILABLE,
            Self::ReplayDetected => StatusCode::CONFLICT,
            Self::InvalidAttestationFormat => StatusCode::BAD_REQUEST,
            Self::InvalidSignature | Self::ExpiredAttestation => {
                StatusCode::FORBIDDEN
            }
        }
    }
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

    // [NEXUS-ERP-02] ERP Reconciliation Logic.
    let entries = payload
        .odata_payload
        .get("value")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut tx_ids = Vec::new();
    for entry in entries {
        if let Some(tx_id) = entry.get("TransactionID").and_then(|v| v.as_str()) {
            tx_ids.push(tx_id.to_string());
        }
        if tx_ids.len() >= MAX_ERP_TX_IDS {
            break;
        }
    }

    let action = map_erp_action(&payload.odata_payload);

    // Centralized trusted keys from state.config (CON-330)
    let trusted_keys = &state.config.erp_attestation_trusted_keys;
    if trusted_keys.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let verified_attestation = verify_erp_attestation(
        &payload,
        action,
        &tx_ids,
        trusted_keys,
        Utc::now().timestamp(),
    )
    .map_err(map_erp_attestation_error_to_status)?;

    let mut conn = state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let is_new: bool = redis::cmd("SET")
        .arg(&verified_attestation.replay_key)
        .arg(1)
        .arg("NX")
        .arg("EX")
        .arg(verified_attestation.replay_ttl_seconds)
        .query_async::<Option<String>>(&mut conn)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .is_some();

    if !is_new {
        return Err(StatusCode::CONFLICT);
    }

    Ok(Json(ErpSyncResponse {
        status: "success".to_string(),
        mandate_id: Some(verified_attestation.attestation_id),
        reconciled_entries: tx_ids.len(),
        errors: Vec::new(),
    }))
}

fn map_erp_action(odata_payload: &serde_json::Value) -> &'static str {
    if odata_payload.get("@odata.context").is_some() {
        "sync_odata_v4"
    } else {
        "sync_legacy"
    }
}

fn map_erp_attestation_error_to_status(error: ErpAttestationError) -> StatusCode {
    error.status_code()
}

fn verify_erp_attestation(
    request: &ErpSyncRequest,
    action: &str,
    tx_ids: &[String],
    trusted_keys: &HashMap<String, String>,
    now: i64,
) -> Result<VerifiedErpAttestation, ErpAttestationError> {
    let att = &request.attestation;

    if att.expires_at <= att.issued_at {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }
    if now < att.issued_at - ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS {
        return Err(ErpAttestationError::ExpiredAttestation);
    }
    if now > att.expires_at + ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS {
        return Err(ErpAttestationError::ExpiredAttestation);
    }
    if att.expires_at - att.issued_at > ERP_ATTESTATION_MAX_LIFETIME_SECONDS {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    let secret = trusted_keys
        .get(&att.key_id)
        .ok_or(ErpAttestationError::InvalidSignature)?;

    let canonical_payload = canonical_erp_attestation_payload(request, action, tx_ids);
    let mut hmac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ErpAttestationError::Misconfigured)?;
    hmac.update(canonical_payload.as_bytes());

    let signature_bytes = hex::decode(&att.signature).map_err(|_| ErpAttestationError::InvalidSignature)?;
    hmac.verify_slice(&signature_bytes).map_err(|_| ErpAttestationError::InvalidSignature)?;

    let mut hasher = Sha256::new();
    hasher.update(canonical_payload.as_bytes());
    let digest = hex::encode(hasher.finalize());

    Ok(VerifiedErpAttestation {
        attestation_id: format!("erp_att_{}", &digest[..24]),
        replay_key: format!("{}:{}", ERP_ATTESTATION_REPLAY_PREFIX, att.nonce),
        replay_ttl_seconds: (att.expires_at - now).max(0) as u64 + 60,
    })
}

fn canonical_erp_attestation_payload(
    request: &ErpSyncRequest,
    action: &str,
    tx_ids: &[String],
) -> String {
    use crate::storage::kwil::encode_payload_value;
    format!(
        "{}|organization_id={}|erp_type={}|request_timestamp={}|action={}|tx_ids={}|key_id={}|nonce={}|issued_at={}|expires_at={}",
        "nexus:erp:attestation:v1",
        encode_payload_value(&request.organization_id),
        encode_payload_value(&request.erp_type),
        request.timestamp,
        encode_payload_value(action),
        encode_payload_value(&tx_ids.join(",")),
        encode_payload_value(&request.attestation.key_id),
        encode_payload_value(&request.attestation.nonce),
        request.attestation.issued_at,
        request.attestation.expires_at
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_request() -> ErpSyncRequest {
        ErpSyncRequest {
            organization_id: "org-1".to_string(),
            erp_type: "SAP".to_string(),
            odata_payload: serde_json::json!({"value": []}),
            timestamp: 1_700_000_000,
            attestation: ErpAttestation {
                key_id: "erp-key-1".to_string(),
                nonce: "nonce-1".to_string(),
                issued_at: 1_700_000_000,
                expires_at: 1_700_000_300,
                signature: "".to_string(),
            },
        }
    }

    fn sign_test_request(req: &mut ErpSyncRequest, action: &str, tx_ids: &[String], secret: &str) {
        let payload = canonical_erp_attestation_payload(req, action, tx_ids);
        let mut hmac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        hmac.update(payload.as_bytes());
        req.attestation.signature = hex::encode(hmac.finalize().into_bytes());
    }

    #[test]
    fn valid_attestation_signature_passes_verification() {
        let mut request = build_test_request();
        let secret = "test-secret";
        let mut trusted_keys = HashMap::new();
        trusted_keys.insert("erp-key-1".to_string(), secret.to_string());

        let action = "sync_odata_v4";
        let tx_ids = vec!["tx1".to_string()];
        sign_test_request(&mut request, action, &tx_ids, secret);

        let verified = verify_erp_attestation(&request, action, &tx_ids, &trusted_keys, 1_700_000_000).unwrap();
        assert!(verified.attestation_id.starts_with("erp_att_"));
    }

    #[test]
    fn invalid_attestation_signature_is_rejected() {
        let mut request = build_test_request();
        let tx_ids = vec!["tx1".to_string()];
        let action = "sync_odata_v4";
        sign_test_request(&mut request, action, &tx_ids, "wrong-secret");

        let mut trusted_keys = HashMap::new();
        trusted_keys.insert("erp-key-1".to_string(), "test-secret".to_string());

        let result = verify_erp_attestation(&request, action, &tx_ids, &trusted_keys, 1_700_000_000);
        assert_eq!(result.unwrap_err(), ErpAttestationError::InvalidSignature);
    }

    #[test]
    fn expired_attestation_is_rejected() {
        let mut request = build_test_request();
        let tx_ids = vec!["tx1".to_string()];
        let action = "sync_odata_v4";
        let secret = "test-secret";

        request.attestation.issued_at = 1_699_998_000;
        request.attestation.expires_at = 1_699_998_050;
        sign_test_request(&mut request, action, &tx_ids, secret);

        let mut trusted_keys = HashMap::new();
        trusted_keys.insert("erp-key-1".to_string(), secret.to_string());

        let result = verify_erp_attestation(&request, action, &tx_ids, &trusted_keys, 1_700_000_000);
        assert_eq!(result.unwrap_err(), ErpAttestationError::ExpiredAttestation);
    }
}
