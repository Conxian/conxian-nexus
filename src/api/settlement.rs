//! [CON-162] External Settlement Trigger Module.
//! Handles ISO 20022, PAPSS, and BRICS triggers for TEE-verified proposals.

use crate::api::rest::AppState;
use crate::storage::kwil::{KwilSettlementLogCommitment, KwilSettlementProposalCommitment};
use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;
use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
pub struct PolicyRejection {
    pub code: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct SettlementProposalResponse {
    pub proposal_id: String,
    pub status: String,
    pub unlock_height: u64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_rejection: Option<PolicyRejection>,
}

#[derive(Debug, Deserialize)]
struct RawRoutingPolicyMetadata {
    system: Option<String>,
    #[serde(alias = "trustTier")]
    trust_tier: Option<String>,
    #[serde(alias = "verificationClass")]
    verification_class: Option<String>,
    #[serde(alias = "policyVersion")]
    policy_version: Option<String>,
    #[serde(alias = "evidenceHash")]
    evidence_hash: Option<String>,
    #[serde(default, alias = "requestedTrustTier")]
    requested_trust_tier: Option<String>,
}

#[derive(Debug)]
pub struct RoutingPolicyMetadata {
    pub system: RoutingSystem,
    pub trust_tier: TrustTier,
    pub verification_class: VerificationClass,
    pub policy_version: String,
    pub evidence_hash: String,
    pub requested_trust_tier: Option<TrustTier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingSystem {
    Ibc,
    Hyperlane,
    LayerZeroV2,
    WormholeNtt,
    AxelarGmp,
}

impl RoutingSystem {
    fn parse(value: &str) -> Option<Self> {
        match normalize_policy_token(value).as_str() {
            "ibc" => Some(Self::Ibc),
            "hyperlane" => Some(Self::Hyperlane),
            "layerzero" | "layerzero_v2" => Some(Self::LayerZeroV2),
            "wormhole_ntt" => Some(Self::WormholeNtt),
            "axelar_gmp" => Some(Self::AxelarGmp),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Ibc => "IBC",
            Self::Hyperlane => "Hyperlane",
            Self::LayerZeroV2 => "LayerZero v2",
            Self::WormholeNtt => "Wormhole NTT",
            Self::AxelarGmp => "Axelar GMP",
        }
    }

    fn required_verification_class(self) -> VerificationClass {
        match self {
            Self::Ibc => VerificationClass::LightClient,
            Self::Hyperlane => VerificationClass::AppDefinedMultiverifier,
            Self::LayerZeroV2 => VerificationClass::ExternalQuorum,
            Self::WormholeNtt => VerificationClass::ExternalQuorum,
            Self::AxelarGmp => VerificationClass::SharedPos,
        }
    }

    fn allowed_tiers(self) -> &'static [TrustTier] {
        const IBC_TIERS: &[TrustTier] = &[TrustTier::T1, TrustTier::T2];
        const CONDITIONAL_TIERS: &[TrustTier] = &[TrustTier::T2, TrustTier::T3];

        match self {
            Self::Ibc => IBC_TIERS,
            Self::Hyperlane => CONDITIONAL_TIERS,
            Self::LayerZeroV2 => CONDITIONAL_TIERS,
            Self::WormholeNtt => CONDITIONAL_TIERS,
            Self::AxelarGmp => CONDITIONAL_TIERS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustTier {
    T1,
    T2,
    T3,
    T4,
}

impl TrustTier {
    fn parse(value: &str) -> Option<Self> {
        match normalize_policy_token(value).as_str() {
            "t1" | "tier1" => Some(Self::T1),
            "t2" | "tier2" => Some(Self::T2),
            "t3" | "tier3" => Some(Self::T3),
            "t4" | "tier4" => Some(Self::T4),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::T1 => "T1",
            Self::T2 => "T2",
            Self::T3 => "T3",
            Self::T4 => "T4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationClass {
    LightClient,
    ExternalQuorum,
    AppDefinedMultiverifier,
    SharedPos,
}

impl VerificationClass {
    fn parse(value: &str) -> Option<Self> {
        match normalize_policy_token(value).as_str() {
            "light_client" | "lightclient" => Some(Self::LightClient),
            "external_quorum" => Some(Self::ExternalQuorum),
            "app_defined_multiverifier" => Some(Self::AppDefinedMultiverifier),
            "shared_pos" => Some(Self::SharedPos),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::LightClient => "light_client",
            Self::ExternalQuorum => "external_quorum",
            Self::AppDefinedMultiverifier => "app_defined_multiverifier",
            Self::SharedPos => "shared_pos",
        }
    }
}

#[derive(Debug)]
pub struct RoutingPolicyBlock {
    pub code: &'static str,
    pub reason: String,
    pub details: Option<serde_json::Value>,
}

impl RoutingPolicyBlock {
    fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
            details: None,
        }
    }

    fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

fn normalize_policy_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn extract_inline_routing_policy(payload: &serde_json::Value) -> Option<&serde_json::Value> {
    payload
        .get("routing_policy")
        .or_else(|| payload.get("routingPolicy"))
}

fn extract_routing_policy(payload: &serde_json::Value) -> Option<&serde_json::Value> {
    extract_inline_routing_policy(payload)
        .or_else(|| {
            payload
                .get("metadata")
                .and_then(extract_inline_routing_policy)
        })
        .or_else(|| {
            payload
                .get("routing")
                .and_then(extract_inline_routing_policy)
        })
        .or_else(|| {
            payload
                .get("bridge_metadata")
                .and_then(extract_inline_routing_policy)
        })
        .or_else(|| {
            payload
                .get("bridgeMetadata")
                .and_then(extract_inline_routing_policy)
        })
}

fn require_non_empty_field(
    value: Option<String>,
    field_name: &'static str,
) -> Result<String, RoutingPolicyBlock> {
    match value.map(|v| v.trim().to_string()) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(RoutingPolicyBlock::new(
            "missing_required_routing_policy_field",
            format!("routing policy field '{}' is required", field_name),
        )
        .with_details(json!({ "field": field_name }))),
    }
}

fn optional_non_empty_field(
    value: Option<String>,
    field_name: &'static str,
) -> Result<Option<String>, RoutingPolicyBlock> {
    match value {
        Some(v) if v.trim().is_empty() => Err(RoutingPolicyBlock::new(
            "invalid_routing_policy_field",
            format!("routing policy field '{}' must be non-empty", field_name),
        )
        .with_details(json!({ "field": field_name }))),
        Some(v) => Ok(Some(v.trim().to_string())),
        None => Ok(None),
    }
}

pub fn validate_routing_policy_metadata(
    payload: &serde_json::Value,
) -> Result<RoutingPolicyMetadata, RoutingPolicyBlock> {
    let routing_policy = extract_routing_policy(payload)
        .ok_or_else(|| {
            RoutingPolicyBlock::new(
                "missing_routing_policy",
                "routing policy metadata is required",
            )
        })?
        .clone();

    if !routing_policy.is_object() {
        return Err(RoutingPolicyBlock::new(
            "invalid_routing_policy",
            "routing policy metadata must be a JSON object",
        ));
    }

    let raw: RawRoutingPolicyMetadata = serde_json::from_value(routing_policy).map_err(|e| {
        RoutingPolicyBlock::new(
            "invalid_routing_policy",
            format!("unable to parse routing policy metadata: {e}"),
        )
    })?;

    let system_raw = require_non_empty_field(raw.system, "system")?;
    let trust_tier_raw = require_non_empty_field(raw.trust_tier, "trust_tier")?;
    let verification_class_raw =
        require_non_empty_field(raw.verification_class, "verification_class")?;
    let policy_version = require_non_empty_field(raw.policy_version, "policy_version")?;
    let evidence_hash = require_non_empty_field(raw.evidence_hash, "evidence_hash")?;
    let requested_trust_tier_raw =
        optional_non_empty_field(raw.requested_trust_tier, "requested_trust_tier")?;

    let system = RoutingSystem::parse(&system_raw).ok_or_else(|| {
        RoutingPolicyBlock::new(
            "unknown_system",
            format!("routing system '{}' is not approved", system_raw),
        )
        .with_details(json!({ "system": system_raw }))
    })?;

    let trust_tier = TrustTier::parse(&trust_tier_raw).ok_or_else(|| {
        RoutingPolicyBlock::new(
            "unknown_trust_tier",
            format!("trust tier '{}' is not recognized", trust_tier_raw),
        )
        .with_details(json!({ "trust_tier": trust_tier_raw }))
    })?;

    let verification_class =
        VerificationClass::parse(&verification_class_raw).ok_or_else(|| {
            RoutingPolicyBlock::new(
                "unknown_verification_class",
                format!(
                    "verification class '{}' is not recognized",
                    verification_class_raw
                ),
            )
            .with_details(json!({ "verification_class": verification_class_raw }))
        })?;

    let requested_trust_tier = requested_trust_tier_raw
        .as_deref()
        .map(|raw_requested| {
            TrustTier::parse(raw_requested).ok_or_else(|| {
                RoutingPolicyBlock::new(
                    "unknown_requested_trust_tier",
                    format!("requested trust tier '{}' is not recognized", raw_requested),
                )
                .with_details(json!({ "requested_trust_tier": raw_requested }))
            })
        })
        .transpose()?;

    if let Some(requested) = requested_trust_tier {
        if requested != trust_tier {
            return Err(RoutingPolicyBlock::new(
                "requested_trust_tier_mismatch",
                "requested trust tier does not match enforced trust tier",
            )
            .with_details(json!({
                "requested_trust_tier": requested.as_str(),
                "trust_tier": trust_tier.as_str(),
            })));
        }
    }

    if trust_tier == TrustTier::T4 {
        return Err(RoutingPolicyBlock::new(
            "trust_tier_not_allowed",
            "trust tier T4 is denied in this phase",
        )
        .with_details(json!({ "trust_tier": trust_tier.as_str() })));
    }

    if trust_tier == TrustTier::T1 && system != RoutingSystem::Ibc {
        return Err(
            RoutingPolicyBlock::new("t1_requires_ibc", "trust tier T1 allows IBC only")
                .with_details(json!({
                    "trust_tier": trust_tier.as_str(),
                    "system": system.as_str(),
                })),
        );
    }

    if verification_class != system.required_verification_class() {
        return Err(RoutingPolicyBlock::new(
            "verification_class_mismatch",
            format!(
                "verification class '{}' is not approved for system '{}'",
                verification_class.as_str(),
                system.as_str()
            ),
        )
        .with_details(json!({
            "system": system.as_str(),
            "expected_verification_class": system.required_verification_class().as_str(),
            "provided_verification_class": verification_class.as_str(),
        })));
    }

    if !system.allowed_tiers().contains(&trust_tier) {
        return Err(RoutingPolicyBlock::new(
            "tier_system_not_allowed",
            format!(
                "system '{}' is not approved for trust tier '{}'",
                system.as_str(),
                trust_tier.as_str()
            ),
        )
        .with_details(json!({
            "system": system.as_str(),
            "trust_tier": trust_tier.as_str(),
        })));
    }

    Ok(RoutingPolicyMetadata {
        system,
        trust_tier,
        verification_class,
        policy_version,
        evidence_hash,
        requested_trust_tier,
    })
}

/// [CON-162] Handles external settlement triggers.
/// Verifies TEE attestation and initiates a 144-block time-lock proposal.
pub fn settlement_routes() -> Router<AppState> {
    Router::new().route("/trigger", post(settlement_trigger_handler))
}

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
            StatusCode::FORBIDDEN,
            Json(SettlementProposalResponse {
                proposal_id: "".to_string(),
                status: "Rejected".to_string(),
                unlock_height: 0,
                message: "Invalid TEE Attestation. Security floor violated.".to_string(),
                policy_rejection: None,
            }),
        )
            .into_response();
    }

    // 2. Enforce routing policy metadata before oracle checks and DB writes [CON-803]
    let routing_policy = match validate_routing_policy_metadata(&payload.payload) {
        Ok(policy) => policy,
        Err(policy_block) => {
            tracing::warn!(
                code = policy_block.code,
                reason = %policy_block.reason,
                external_id = %payload.external_id,
                source = %payload.source,
                "Settlement trigger blocked by routing policy"
            );
            return (
                StatusCode::FORBIDDEN,
                Json(SettlementProposalResponse {
                    proposal_id: "".to_string(),
                    status: "PolicyBlocked".to_string(),
                    unlock_height: 0,
                    message: "Routing policy enforcement rejected settlement trigger.".to_string(),
                    policy_rejection: Some(PolicyRejection {
                        code: policy_block.code.to_string(),
                        reason: policy_block.reason,
                        details: policy_block.details,
                    }),
                }),
            )
                .into_response();
        }
    };

    tracing::info!(
        system = routing_policy.system.as_str(),
        trust_tier = routing_policy.trust_tier.as_str(),
        verification_class = routing_policy.verification_class.as_str(),
        policy_version = %routing_policy.policy_version,
        evidence_hash = %routing_policy.evidence_hash,
        requested_trust_tier = ?routing_policy.requested_trust_tier.map(|tier| tier.as_str()),
        "Routing policy metadata validated"
    );

    // 3. Oracle Verification
    if let Some(oracle) = &state.oracle {
        match oracle
            .verify_external_signal(&payload.source, &payload.payload)
            .await
        {
            Ok(true) => tracing::info!("Oracle verified {} signal", payload.source),
            Ok(false) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(SettlementProposalResponse {
                        proposal_id: "".to_string(),
                        status: "Rejected".to_string(),
                        unlock_height: 0,
                        message: "Oracle verification failed for external signal.".to_string(),
                        policy_rejection: None,
                    }),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(SettlementProposalResponse {
                        proposal_id: "".to_string(),
                        status: "Error".to_string(),
                        unlock_height: 0,
                        message: format!("Oracle service error: {}", e),
                        policy_rejection: None,
                    }),
                )
                    .into_response();
            }
        }
    }

    // 4. Log external settlement event [CON-164]
    // Extract institutional identifiers for reconciliation
    let fiat_value = payload.payload.get("amount").and_then(|v| v.as_f64());

    // Reconciliation helpers (UETR for ISO20022, unique refs for PAPSS)
    let uetr = payload.payload.get("uetr").and_then(|v| v.as_str());
    let e2e_id = payload
        .payload
        .get("end_to_end_id")
        .and_then(|v| v.as_str());

    let external_tx_ref = uetr.or(e2e_id).unwrap_or(&payload.external_id).to_string();
    let _ = sqlx::query(
        "INSERT INTO cxn_external_settlement_logs (external_tx_reference, settlement_network_origin, fiat_value_pegged, raw_payload)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&external_tx_ref)
    .bind(&payload.source)
    .bind(fiat_value)
    .bind(&payload.payload)
    .execute(&state.storage.pg_pool)
    .await;

    // [CON-330] Pilot: Mirror settlement log to Kwil
    if let Some(kwil) = &state.kwil {
        let _ = kwil
            .persist_settlement_log(KwilSettlementLogCommitment {
                external_tx_reference: external_tx_ref,
                settlement_network_origin: payload.source.clone(),
                fiat_value_pegged: fiat_value,
                raw_payload: payload.payload.clone(),
            })
            .await
            .map_err(|e| tracing::warn!("Kwil settlement log persistence failed: {}", e))
            .ok();
    }

    // 5. Get current block height to calculate time-lock
    let row_res = sqlx::query("SELECT MAX(height) as max_height FROM stacks_blocks")
        .fetch_optional(&state.storage.pg_pool)
        .await;

    let current_height: i64 = match row_res {
        Ok(Some(row)) => row.get::<Option<i64>, _>("max_height").unwrap_or(0),
        _ => 0,
    };

    let unlock_height = (current_height + 144) as u64;
    let proposal_id = format!("prop_{}", Uuid::new_v4());

    // 6. Persist the proposal as "proposal-only"
    let res = sqlx::query(
        "INSERT INTO settlement_proposals (proposal_id, external_id, source, payload, status, init_height, unlock_height)
         VALUES ($1, $2, $3, $4, 'active', $5, $6)",
    )
    .bind(&proposal_id)
    .bind(&payload.external_id)
    .bind(&payload.source)
    .bind(&payload.payload)
    .bind(current_height)
    .bind(unlock_height as i64)
    .execute(&state.storage.pg_pool)
    .await;

    // [CON-330] Pilot: Mirror settlement proposal to Kwil
    if let Some(kwil) = &state.kwil {
        let _ = kwil
            .persist_settlement_proposal(KwilSettlementProposalCommitment {
                proposal_id: proposal_id.clone(),
                external_id: payload.external_id.clone(),
                source: payload.source.clone(),
                payload: payload.payload.clone(),
                status: "active".to_string(),
                init_height: current_height as u64,
                unlock_height,
            })
            .await
            .map_err(|e| tracing::warn!("Kwil settlement proposal persistence failed: {}", e))
            .ok();
    }

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
                policy_rejection: None,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to persist settlement proposal: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SettlementProposalResponse {
                    proposal_id: "".to_string(),
                    status: "Error".to_string(),
                    unlock_height: 0,
                    message: "Internal persistence failure.".to_string(),
                    policy_rejection: None,
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_payload_with_routing_policy(routing_policy: serde_json::Value) -> serde_json::Value {
        json!({
            "amount": 1000,
            "currency": "USD",
            "routing_policy": routing_policy,
        })
    }

    #[test]
    fn allows_t1_ibc_with_required_metadata() {
        let payload = base_payload_with_routing_policy(json!({
            "system": "IBC",
            "trust_tier": "T1",
            "verification_class": "light_client",
            "policy_version": "2026-06-01",
            "evidence_hash": "0xabc123",
            "requested_trust_tier": "T1",
        }));

        let result = validate_routing_policy_metadata(&payload);
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_missing_routing_policy_metadata() {
        let payload = json!({
            "amount": 1000,
            "currency": "USD",
        });

        let err = validate_routing_policy_metadata(&payload).unwrap_err();
        assert_eq!(err.code, "missing_routing_policy");
    }

    #[test]
    fn rejects_unknown_system() {
        let payload = base_payload_with_routing_policy(json!({
            "system": "UnknownBridge",
            "trust_tier": "T2",
            "verification_class": "external_quorum",
            "policy_version": "2026-06-01",
            "evidence_hash": "0xabc123",
        }));

        let err = validate_routing_policy_metadata(&payload).unwrap_err();
        assert_eq!(err.code, "unknown_system");
    }

    #[test]
    fn rejects_t1_non_ibc_system() {
        let payload = base_payload_with_routing_policy(json!({
            "system": "Hyperlane",
            "trust_tier": "T1",
            "verification_class": "app_defined_multiverifier",
            "policy_version": "2026-06-01",
            "evidence_hash": "0xabc123",
        }));

        let err = validate_routing_policy_metadata(&payload).unwrap_err();
        assert_eq!(err.code, "t1_requires_ibc");
    }

    #[test]
    fn rejects_requested_trust_tier_mismatch() {
        let payload = base_payload_with_routing_policy(json!({
            "system": "IBC",
            "trust_tier": "T1",
            "verification_class": "light_client",
            "policy_version": "2026-06-01",
            "evidence_hash": "0xabc123",
            "requested_trust_tier": "T2",
        }));

        let err = validate_routing_policy_metadata(&payload).unwrap_err();
        assert_eq!(err.code, "requested_trust_tier_mismatch");
    }
}
