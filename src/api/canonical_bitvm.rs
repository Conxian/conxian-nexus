use crate::api::rest::AppState;
use crate::executor::bitvm_groth16::{
    parse_gateway_envelope_json, CanonicalGroth16Error, NexusStateTransition,
};
use crate::executor::canonical_bitvm::{
    CanonicalBitvmAuditError, CanonicalBitvmServiceError, TrustedHeightError,
};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CANONICAL_BITVM_BODY_LIMIT_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalBitvmVerifyRequest {
    pub previous_state_root: String,
    pub next_state_root: String,
    pub envelope: Value,
}

#[derive(Debug, Serialize)]
pub struct CanonicalBitvmVerifyResponse {
    pub receipt_id: String,
    pub statement_hash: String,
    pub verification_key_id: String,
    pub circuit_id: String,
    pub schema_version: u16,
    pub curve: &'static str,
    pub status: &'static str,
    pub created: bool,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
struct ApiErrorDetail {
    code: &'static str,
    message: &'static str,
}

pub fn canonical_bitvm_routes() -> Router<AppState> {
    Router::new()
        .route("/verify-state-transition", post(verify_state_transition))
        .layer(DefaultBodyLimit::max(CANONICAL_BITVM_BODY_LIMIT_BYTES))
}

pub async fn verify_state_transition(
    State(state): State<AppState>,
    payload: Result<Json<CanonicalBitvmVerifyRequest>, JsonRejection>,
) -> Response {
    let Json(payload) = match payload {
        Ok(payload) => payload,
        Err(rejection) => {
            let status = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::BAD_REQUEST
            };
            return api_error(status, "malformed_payload", "request payload is malformed");
        }
    };
    let transition = match (
        decode_root(&payload.previous_state_root),
        decode_root(&payload.next_state_root),
    ) {
        (Ok(previous), Ok(next)) => NexusStateTransition {
            prev_state_root: previous,
            next_state_root: next,
        },
        _ => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "malformed_payload",
                "state roots must be 64 lowercase hexadecimal characters",
            )
        }
    };
    let envelope = match parse_gateway_envelope_json(payload.envelope) {
        Ok(envelope) => envelope,
        Err(error) => return map_parser_error(error),
    };
    let Some(service) = state.executor.canonical_bitvm_service.as_ref() else {
        return api_error(
            StatusCode::NOT_IMPLEMENTED,
            "canonical_registry_unavailable",
            "canonical BitVM verifier registry is not configured",
        );
    };
    match service.verify_and_persist(&transition, &envelope).await {
        Ok(receipt) => (
            StatusCode::OK,
            Json(CanonicalBitvmVerifyResponse {
                receipt_id: receipt.receipt_id,
                statement_hash: hex::encode(receipt.statement_hash),
                verification_key_id: hex::encode(receipt.verification_key_id.0),
                circuit_id: receipt.circuit_id,
                schema_version: receipt.schema_version,
                curve: receipt.curve.as_str(),
                status: "verified",
                created: receipt.created,
            }),
        )
            .into_response(),
        Err(error) => map_service_error(error),
    }
}

fn decode_root(value: &str) -> Result<[u8; 32], ()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(());
    }
    hex::decode(value)
        .map_err(|_| ())?
        .try_into()
        .map_err(|_| ())
}

fn map_parser_error(error: CanonicalGroth16Error) -> Response {
    match error {
        CanonicalGroth16Error::MalformedEnvelope(_)
        | CanonicalGroth16Error::RawWitnessProvided
        | CanonicalGroth16Error::InvalidProofEncoding(_)
        | CanonicalGroth16Error::NonCanonicalFieldElement => api_error(
            StatusCode::BAD_REQUEST,
            "malformed_payload",
            "canonical Gateway envelope is malformed",
        ),
        _ => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_statement",
            "canonical Gateway envelope is not accepted",
        ),
    }
}

fn map_service_error(error: CanonicalBitvmServiceError) -> Response {
    match error {
        CanonicalBitvmServiceError::NetworkMismatch { .. } => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "network_mismatch",
            "proof network does not match the configured network",
        ),
        CanonicalBitvmServiceError::TrustedHeight(TrustedHeightError::Unavailable) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "trusted_height_unavailable",
            "trusted Bitcoin height is unavailable",
        ),
        CanonicalBitvmServiceError::TrustedHeight(TrustedHeightError::Invalid) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "trusted_height_invalid",
            "trusted Bitcoin height is invalid",
        ),
        CanonicalBitvmServiceError::Audit(CanonicalBitvmAuditError::Unavailable) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "audit_unavailable",
            "canonical verification audit is unavailable",
        ),
        CanonicalBitvmServiceError::Audit(CanonicalBitvmAuditError::IntegrityConflict) => {
            api_error(
                StatusCode::CONFLICT,
                "audit_integrity_conflict",
                "canonical verification audit conflicts with immutable data",
            )
        }
        CanonicalBitvmServiceError::Verification(
            CanonicalGroth16Error::PairingVerification(_)
            | CanonicalGroth16Error::RegistryIntegrityMismatch
            | CanonicalGroth16Error::ConflictingVerificationKeyAssociation,
        ) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "canonical verification failed internally",
        ),
        CanonicalBitvmServiceError::Verification(_) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "verification_rejected",
            "canonical proof or statement was rejected",
        ),
    }
}

pub fn legacy_bitvm_unavailable() -> Response {
    api_error(
        StatusCode::NOT_IMPLEMENTED,
        "legacy_bitvm_route_deprecated",
        "legacy caller-keyed BitVM verification is unavailable",
    )
}

fn api_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (
        status,
        Json(ApiErrorBody {
            error: ApiErrorDetail { code, message },
        }),
    )
        .into_response()
}
