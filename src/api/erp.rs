//! [CON-63] OData/ERP Translation Layer for Conxian Gateway.
//! Bridges SAP/Oracle OData payloads to x402 mandates.

use crate::api::rest::AppState;
use axum::routing::post;
use axum::Router;
use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

type HmacSha256 = Hmac<Sha256>;

const MAX_ERP_TX_IDS: usize = 1000;
const MAX_ERP_ERRORS: usize = 100;
const ERP_ATTESTATION_TRUSTED_KEYS_JSON_ENV: &str = "ERP_ATTESTATION_TRUSTED_KEYS_JSON";
const ERP_ATTESTATION_REPLAY_PREFIX: &str = "nexus:erp:attestation:nonce:v1";
const ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS: i64 = 60;
const ERP_ATTESTATION_MAX_LIFETIME_SECONDS: i64 = 600;
const ERP_ATTESTATION_SIGNATURE_HEX_LEN: usize = 64;
const MAX_ERP_ATTESTATION_KEY_ID_LEN: usize = 128;
const MAX_ERP_ATTESTATION_NONCE_LEN: usize = 256;

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
    ContextMismatch,
    ReplayDetected,
    ReplayStoreUnavailable,
}

impl ErpAttestationError {
    fn status_code(self) -> StatusCode {
        match self {
            Self::Misconfigured | Self::ReplayStoreUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::ReplayDetected => StatusCode::CONFLICT,
            Self::InvalidAttestationFormat => StatusCode::BAD_REQUEST,
            Self::InvalidSignature | Self::ExpiredAttestation | Self::ContextMismatch => {
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
            }
        }
    }

    if tx_ids.len() > MAX_ERP_TX_IDS {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    tx_ids.sort_unstable();

    // OData v4 to x402 Mandate Translation
    let action = map_erp_action(&payload.odata_payload);

    let trusted_keys =
        load_trusted_attestation_keys().map_err(map_erp_attestation_error_to_status)?;
    let verified_attestation = verify_erp_attestation(
        &payload,
        action,
        &tx_ids,
        &trusted_keys,
        Utc::now().timestamp(),
    )
    .map_err(map_erp_attestation_error_to_status)?;

    claim_attestation_nonce(
        &state,
        &verified_attestation.replay_key,
        verified_attestation.replay_ttl_seconds,
    )
    .await
    .map_err(map_erp_attestation_error_to_status)?;

    tracing::info!(
        attestation_id = %verified_attestation.attestation_id,
        "ERP attestation verified"
    );

    let mut reconciled_entries = 0;
    let mut errors = Vec::new();

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

    for tx_id in &tx_ids {
        if found.contains(tx_id) {
            reconciled_entries += 1;
        } else if errors.len() < MAX_ERP_ERRORS {
            errors.push(format!(
                "Transaction {} not found or not finalized in local state",
                tx_id
            ));
        }
    }

    let mandate_hash = format!("x402_{}", uuid::Uuid::new_v4());

    tracing::info!(
        "Translated OData to x402 Mandate. Action: {}. Attestation enforced.",
        action
    );

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

fn map_erp_action(odata_payload: &serde_json::Value) -> &'static str {
    match odata_payload.get("action").and_then(|a| a.as_str()) {
        Some("REBALANCE") => "REBALANCE_OPEX",
        Some("DISBURSE") => "DISBURSE_YIELD",
        _ => "SETTLE_TX",
    }
}

fn map_erp_attestation_error_to_status(error: ErpAttestationError) -> StatusCode {
    tracing::warn!(?error, "ERP attestation verification rejected request");
    error.status_code()
}

fn load_trusted_attestation_keys() -> Result<HashMap<String, String>, ErpAttestationError> {
    let raw = std::env::var(ERP_ATTESTATION_TRUSTED_KEYS_JSON_ENV)
        .map_err(|_| ErpAttestationError::Misconfigured)?;

    let parsed: HashMap<String, String> =
        serde_json::from_str(&raw).map_err(|_| ErpAttestationError::Misconfigured)?;

    let mut trusted_keys = HashMap::new();
    for (key_id, secret) in parsed {
        let key_id = key_id.trim();
        let secret = secret.trim();
        if key_id.is_empty() || secret.is_empty() {
            return Err(ErpAttestationError::Misconfigured);
        }
        trusted_keys.insert(key_id.to_string(), secret.to_string());
    }

    if trusted_keys.is_empty() {
        return Err(ErpAttestationError::Misconfigured);
    }

    Ok(trusted_keys)
}

fn verify_erp_attestation(
    request: &ErpSyncRequest,
    action: &str,
    tx_ids: &[String],
    trusted_keys: &HashMap<String, String>,
    now_ts: i64,
) -> Result<VerifiedErpAttestation, ErpAttestationError> {
    validate_attestation_metadata(&request.attestation)?;

    let secret = trusted_keys
        .get(request.attestation.key_id.trim())
        .ok_or(ErpAttestationError::InvalidSignature)?;

    let canonical_payload = canonical_erp_attestation_payload(request, action, tx_ids);
    if !verify_hmac_sha256_signature(
        secret,
        &canonical_payload,
        request.attestation.signature.trim(),
    ) {
        return Err(ErpAttestationError::InvalidSignature);
    }

    let replay_ttl_seconds =
        validate_attestation_time_window(&request.attestation, request.timestamp, now_ts)?;

    Ok(VerifiedErpAttestation {
        attestation_id: build_attestation_id(&canonical_payload, &request.attestation.signature),
        replay_key: build_replay_cache_key(
            &request.organization_id,
            &request.attestation.key_id,
            &request.attestation.nonce,
        ),
        replay_ttl_seconds,
    })
}

fn validate_attestation_metadata(attestation: &ErpAttestation) -> Result<(), ErpAttestationError> {
    let key_id = attestation.key_id.trim();
    let nonce = attestation.nonce.trim();
    let signature = attestation.signature.trim();

    if key_id.is_empty() || key_id.len() > MAX_ERP_ATTESTATION_KEY_ID_LEN {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    if nonce.is_empty() || nonce.len() > MAX_ERP_ATTESTATION_NONCE_LEN {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    if signature.len() != ERP_ATTESTATION_SIGNATURE_HEX_LEN
        || !signature.chars().all(|ch| ch.is_ascii_hexdigit())
    {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    if attestation.expires_at <= attestation.issued_at {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    if attestation.expires_at - attestation.issued_at > ERP_ATTESTATION_MAX_LIFETIME_SECONDS {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    Ok(())
}

fn validate_attestation_time_window(
    attestation: &ErpAttestation,
    request_timestamp: i64,
    now_ts: i64,
) -> Result<u64, ErpAttestationError> {
    if now_ts > attestation.expires_at {
        return Err(ErpAttestationError::ExpiredAttestation);
    }

    if now_ts + ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS < attestation.issued_at {
        return Err(ErpAttestationError::InvalidAttestationFormat);
    }

    let min_bound = attestation.issued_at - ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS;
    let max_bound = attestation.expires_at + ERP_ATTESTATION_MAX_CLOCK_SKEW_SECONDS;
    if request_timestamp < min_bound || request_timestamp > max_bound {
        return Err(ErpAttestationError::ContextMismatch);
    }

    let replay_ttl_seconds = (attestation.expires_at - now_ts).max(1) as u64;
    Ok(replay_ttl_seconds)
}

fn canonical_erp_attestation_payload(
    request: &ErpSyncRequest,
    action: &str,
    tx_ids: &[String],
) -> String {
    let encoded_tx_ids = if tx_ids.is_empty() {
        "-".to_string()
    } else {
        tx_ids
            .iter()
            .map(|tx_id| encode_payload_value(tx_id))
            .collect::<Vec<_>>()
            .join(",")
    };

    format!(
        "{}|organization_id={}|erp_type={}|request_timestamp={}|action={}|tx_ids={}|key_id={}|nonce={}|issued_at={}|expires_at={}",
        "nexus:erp:attestation:v1",
        encode_payload_value(&request.organization_id),
        encode_payload_value(&request.erp_type),
        request.timestamp,
        encode_payload_value(action),
        encoded_tx_ids,
        encode_payload_value(request.attestation.key_id.trim()),
        encode_payload_value(request.attestation.nonce.trim()),
        request.attestation.issued_at,
        request.attestation.expires_at,
    )
}

fn encode_payload_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '%' => out.push_str("%25"),
            '|' => out.push_str("%7C"),
            '=' => out.push_str("%3D"),
            ',' => out.push_str("%2C"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
fn compute_hmac_sha256_signature(secret: &str, payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC-SHA256 accepts variable-length keys");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn verify_hmac_sha256_signature(secret: &str, payload: &str, provided_signature: &str) -> bool {
    let provided = match hex::decode(provided_signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(payload.as_bytes());

    mac.verify_slice(&provided).is_ok()
}

fn build_replay_cache_key(organization_id: &str, key_id: &str, nonce: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(organization_id.trim().as_bytes());
    hasher.update(b"|");
    hasher.update(key_id.trim().as_bytes());
    hasher.update(b"|");
    hasher.update(nonce.trim().as_bytes());

    format!(
        "{}:{}",
        ERP_ATTESTATION_REPLAY_PREFIX,
        hex::encode(hasher.finalize())
    )
}

fn build_attestation_id(canonical_payload: &str, signature: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_payload.as_bytes());
    hasher.update(b"|");
    hasher.update(signature.trim().as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("erp_att_{}", &digest[..24])
}

async fn claim_attestation_nonce(
    state: &AppState,
    replay_key: &str,
    replay_ttl_seconds: u64,
) -> Result<(), ErpAttestationError> {
    let mut conn = state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| ErpAttestationError::ReplayStoreUnavailable)?;

    let claim_result: redis::RedisResult<Option<String>> = redis::cmd("SET")
        .arg(replay_key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(replay_ttl_seconds)
        .query_async(&mut conn)
        .await;

    let claimed = match claim_result {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => return Err(ErpAttestationError::ReplayStoreUnavailable),
    };

    ensure_attestation_not_replayed(claimed)
}

fn ensure_attestation_not_replayed(claimed: bool) -> Result<(), ErpAttestationError> {
    if claimed {
        Ok(())
    } else {
        Err(ErpAttestationError::ReplayDetected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_request() -> ErpSyncRequest {
        ErpSyncRequest {
            organization_id: "org-123".to_string(),
            erp_type: "SAP".to_string(),
            odata_payload: serde_json::json!({
                "action": "DISBURSE",
                "value": [
                    { "TransactionId": "tx-002" },
                    { "TransactionId": "tx-001" }
                ]
            }),
            timestamp: 1_700_000_000,
            attestation: ErpAttestation {
                key_id: "erp-key-1".to_string(),
                nonce: "nonce-123".to_string(),
                issued_at: 1_699_999_980,
                expires_at: 1_700_000_060,
                signature: String::new(),
            },
        }
    }

    fn sign_test_request(
        request: &mut ErpSyncRequest,
        action: &str,
        tx_ids: &[String],
        secret: &str,
    ) {
        let canonical_payload = canonical_erp_attestation_payload(request, action, tx_ids);
        request.attestation.signature = compute_hmac_sha256_signature(secret, &canonical_payload);
    }

    #[test]
    fn valid_attestation_signature_passes_verification() {
        let mut request = build_test_request();
        let tx_ids = vec!["tx-001".to_string(), "tx-002".to_string()];
        let action = "DISBURSE_YIELD";
        let secret = "prod-shared-secret";

        sign_test_request(&mut request, action, &tx_ids, secret);

        let trusted_keys = HashMap::from([("erp-key-1".to_string(), secret.to_string())]);
        let result =
            verify_erp_attestation(&request, action, &tx_ids, &trusted_keys, 1_700_000_000);

        assert!(result.is_ok());
        let verified = result.unwrap();
        assert!(verified.replay_ttl_seconds > 0);
        assert!(verified.attestation_id.starts_with("erp_att_"));
    }

    #[test]
    fn invalid_attestation_signature_is_rejected() {
        let mut request = build_test_request();
        let tx_ids = vec!["tx-001".to_string(), "tx-002".to_string()];
        let action = "DISBURSE_YIELD";

        sign_test_request(&mut request, action, &tx_ids, "wrong-secret");

        let trusted_keys = HashMap::from([("erp-key-1".to_string(), "trusted-secret".to_string())]);

        let result =
            verify_erp_attestation(&request, action, &tx_ids, &trusted_keys, 1_700_000_000);

        assert_eq!(result.unwrap_err(), ErpAttestationError::InvalidSignature);
    }

    #[test]
    fn replay_is_rejected_when_nonce_is_reused() {
        assert!(ensure_attestation_not_replayed(true).is_ok());
        assert_eq!(
            ensure_attestation_not_replayed(false).unwrap_err(),
            ErpAttestationError::ReplayDetected
        );
    }

    #[test]
    fn expired_attestation_is_rejected() {
        let mut request = build_test_request();
        let tx_ids = vec!["tx-001".to_string(), "tx-002".to_string()];
        let action = "DISBURSE_YIELD";
        let secret = "trusted-secret";

        request.attestation.issued_at = 1_699_998_000;
        request.attestation.expires_at = 1_699_998_050;
        request.timestamp = 1_699_998_010;
        sign_test_request(&mut request, action, &tx_ids, secret);

        let trusted_keys = HashMap::from([("erp-key-1".to_string(), secret.to_string())]);

        let result =
            verify_erp_attestation(&request, action, &tx_ids, &trusted_keys, 1_700_000_000);

        assert_eq!(result.unwrap_err(), ErpAttestationError::ExpiredAttestation);
    }
}
