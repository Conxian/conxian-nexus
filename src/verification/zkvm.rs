//! Provider boundary for Bitcoin-state proofs produced by zkVMs.
//!
//! Nexus validates the transport envelope and Bitcoin state binding here. A
//! configured backend must perform the actual SP1 or RISC Zero receipt
//! verification. The default backend is deliberately unavailable; structural
//! validation alone must never produce a successful verification result.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ZKVM_ENVELOPE_VERSION: u16 = 1;
const MAX_PROOF_BYTES: usize = 16 * 1024 * 1024;
const MAX_PUBLIC_VALUES_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZkVmEnvironment {
    Sp1,
    RiscZero,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkVmVerificationPayload {
    pub envelope_version: u16,
    pub environment: ZkVmEnvironment,
    /// SP1 verifying-key digest or RISC Zero image ID, encoded as 32-byte hex.
    pub program_digest: String,
    pub proof_b64: String,
    /// JSON-encoded [`BitcoinPublicValues`].
    pub public_values_b64: String,
    /// Expected Bitcoin block or MMR state commitment, encoded as 32-byte hex.
    pub expected_state_commitment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BitcoinPublicValues {
    pub bitcoin_network: BitcoinNetwork,
    pub bitcoin_height: u64,
    pub state_commitment: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedZkVmProof {
    pub environment: ZkVmEnvironment,
    pub program_digest: [u8; 32],
    pub proof: Vec<u8>,
    pub public_values_bytes: Vec<u8>,
    pub public_values: BitcoinPublicValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkVmVerificationReceipt {
    pub verified: bool,
    pub environment: ZkVmEnvironment,
    pub backend: String,
    pub backend_version: String,
    pub program_digest: String,
    pub bitcoin_network: BitcoinNetwork,
    pub bitcoin_height: u64,
    pub state_commitment: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ZkVmError {
    #[error("unsupported zkVM envelope version {0}")]
    UnsupportedEnvelopeVersion(u16),
    #[error("{field} must be exactly 32 bytes encoded as hexadecimal")]
    InvalidDigest { field: &'static str },
    #[error("invalid base64 in {field}")]
    InvalidBase64 { field: &'static str },
    #[error("{field} exceeds the maximum accepted size")]
    PayloadTooLarge { field: &'static str },
    #[error("proof payload is empty")]
    EmptyProof,
    #[error("public values are not valid Bitcoin state JSON: {0}")]
    InvalidPublicValues(String),
    #[error("public state commitment does not match the expected commitment")]
    StateCommitmentMismatch,
    #[error("zkVM verifier backend is not configured for {0:?}")]
    BackendUnavailable(ZkVmEnvironment),
    #[error("zkVM backend rejected the proof: {0}")]
    ProofRejected(String),
}

/// Cryptographic provider implemented by an audited SP1 or RISC Zero adapter.
pub trait ZkVmProofBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn backend_version(&self) -> &'static str;
    fn verify(&self, proof: &ValidatedZkVmProof) -> Result<(), ZkVmError>;
}

/// Fail-closed backend used until a real verifier is deliberately configured.
#[derive(Debug, Default)]
pub struct UnavailableZkVmBackend;

impl ZkVmProofBackend for UnavailableZkVmBackend {
    fn backend_name(&self) -> &'static str {
        "unavailable"
    }

    fn backend_version(&self) -> &'static str {
        "0"
    }

    fn verify(&self, proof: &ValidatedZkVmProof) -> Result<(), ZkVmError> {
        Err(ZkVmError::BackendUnavailable(proof.environment))
    }
}

pub fn verify_zkvm_proof(
    payload: &ZkVmVerificationPayload,
    backend: &dyn ZkVmProofBackend,
) -> Result<ZkVmVerificationReceipt, ZkVmError> {
    let proof = validate_envelope(payload)?;
    backend.verify(&proof)?;

    Ok(ZkVmVerificationReceipt {
        verified: true,
        environment: proof.environment,
        backend: backend.backend_name().to_owned(),
        backend_version: backend.backend_version().to_owned(),
        program_digest: hex::encode(proof.program_digest),
        bitcoin_network: proof.public_values.bitcoin_network,
        bitcoin_height: proof.public_values.bitcoin_height,
        state_commitment: proof.public_values.state_commitment,
    })
}

pub fn validate_envelope(
    payload: &ZkVmVerificationPayload,
) -> Result<ValidatedZkVmProof, ZkVmError> {
    if payload.envelope_version != ZKVM_ENVELOPE_VERSION {
        return Err(ZkVmError::UnsupportedEnvelopeVersion(
            payload.envelope_version,
        ));
    }

    let program_digest = decode_digest(&payload.program_digest, "program_digest")?;
    let expected = decode_digest(
        &payload.expected_state_commitment,
        "expected_state_commitment",
    )?;
    let proof = decode_bounded(&payload.proof_b64, "proof_b64", MAX_PROOF_BYTES)?;
    if proof.is_empty() {
        return Err(ZkVmError::EmptyProof);
    }
    let public_values_bytes = decode_bounded(
        &payload.public_values_b64,
        "public_values_b64",
        MAX_PUBLIC_VALUES_BYTES,
    )?;
    let public_values: BitcoinPublicValues = serde_json::from_slice(&public_values_bytes)
        .map_err(|error| ZkVmError::InvalidPublicValues(error.to_string()))?;
    let public_commitment = decode_digest(&public_values.state_commitment, "state_commitment")?;
    if !constant_time_eq(&public_commitment, &expected) {
        return Err(ZkVmError::StateCommitmentMismatch);
    }

    Ok(ValidatedZkVmProof {
        environment: payload.environment,
        program_digest,
        proof,
        public_values_bytes,
        public_values,
    })
}

fn decode_digest(value: &str, field: &'static str) -> Result<[u8; 32], ZkVmError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(value).map_err(|_| ZkVmError::InvalidDigest { field })?;
    bytes
        .try_into()
        .map_err(|_| ZkVmError::InvalidDigest { field })
}

fn decode_bounded(value: &str, field: &'static str, maximum: usize) -> Result<Vec<u8>, ZkVmError> {
    let estimated = value.len().saturating_mul(3) / 4;
    if estimated > maximum {
        return Err(ZkVmError::PayloadTooLarge { field });
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| ZkVmError::InvalidBase64 { field })?;
    if bytes.len() > maximum {
        return Err(ZkVmError::PayloadTooLarge { field });
    }
    Ok(bytes)
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> ZkVmVerificationPayload {
        let public = BitcoinPublicValues {
            bitcoin_network: BitcoinNetwork::Signet,
            bitcoin_height: 245,
            state_commitment: "ab".repeat(32),
        };
        ZkVmVerificationPayload {
            envelope_version: ZKVM_ENVELOPE_VERSION,
            environment: ZkVmEnvironment::Sp1,
            program_digest: "cd".repeat(32),
            proof_b64: base64::engine::general_purpose::STANDARD.encode([7_u8; 64]),
            public_values_b64: base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&public).unwrap()),
            expected_state_commitment: "ab".repeat(32),
        }
    }

    #[test]
    fn validates_bitcoin_state_binding() {
        let validated = validate_envelope(&payload()).unwrap();
        assert_eq!(validated.public_values.bitcoin_height, 245);
    }

    #[test]
    fn rejects_mismatched_state_commitment() {
        let mut value = payload();
        value.expected_state_commitment = "ef".repeat(32);
        assert_eq!(
            validate_envelope(&value),
            Err(ZkVmError::StateCommitmentMismatch)
        );
    }

    #[test]
    fn unavailable_backend_never_verifies_structural_payloads() {
        let error = verify_zkvm_proof(&payload(), &UnavailableZkVmBackend).unwrap_err();
        assert_eq!(error, ZkVmError::BackendUnavailable(ZkVmEnvironment::Sp1));
    }
}
