//! Canonical Gateway-v1-compatible BN254 Groth16 verification for the Nexus
//! BitVM state-transition profile.
//!
//! This authenticates supplied transition fields, executes a local pairing
//! check, and persists an audit record. It does not establish Bitcoin
//! inclusion/finality, disputes, complete BitVM execution, or mainnet readiness.

use crate::config::BitVmVerificationKeyConfig;
use crate::storage::Storage;
use ark_bn254::{Bn254, Fr};
use ark_crypto_primitives::snark::SNARK;
use ark_ff::{BigInt, BigInteger, PrimeField};
use ark_groth16::{Groth16, Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fmt, sync::Arc};
use thiserror::Error;

pub const SCHEMA_VERSION: u16 = 1;
pub const CURVE: &str = "bn254";
pub const CIRCUIT_ID: &str = "conxian-nexus-bitvm-state-transition-v1";
pub const PUBLIC_INPUT_COUNT: usize = 7;
pub const COMPRESSED_PROOF_BYTES: usize = 128;
pub const MAX_VERIFICATION_KEY_BYTES: usize = 1024 * 1024;

const STATEMENT_ENCODING_DOMAIN: &[u8] = b"CONXIAN-GROTH16-STATEMENT-ENCODING-V1";
const STATEMENT_HASH_DOMAIN: &[u8] = b"CONXIAN-GROTH16-STATEMENT-HASH-V1";
const VERIFICATION_KEY_ID_DOMAIN: &[u8] = b"CONXIAN-GROTH16-VERIFICATION-KEY-ID-V1";
const PROOF_DIGEST_DOMAIN: &[u8] = b"CONXIAN-GROTH16-PROOF-DIGEST-V1";
const FIELD_ENCODING_BN254_BIG_ENDIAN_32: u8 = 1;

pub const BN254_SCALAR_MODULUS: [u8; 32] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x4e, 0x3e, 0x1f, 0x59, 0x3f, 0x00, 0x00, 0x01,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

impl BitcoinNetwork {
    fn tag(self) -> u8 {
        match self {
            Self::Mainnet => 1,
            Self::Testnet => 2,
            Self::Signet => 3,
            Self::Regtest => 4,
        }
    }
}

impl fmt::Display for BitcoinNetwork {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Signet => "signet",
            Self::Regtest => "regtest",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BitcoinBlockContext {
    pub network: BitcoinNetwork,
    pub block_height: u64,
    pub block_hash: String,
    pub max_valid_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BitVMTransition {
    pub schema_version: u16,
    pub curve: String,
    pub circuit_id: String,
    pub verification_key_id: String,
    pub prev_state_root: String,
    pub next_state_root: String,
    pub steps_verified: u64,
    pub witness_commitment: String,
    pub public_inputs: Vec<String>,
    pub block_context: BitcoinBlockContext,
    pub proof: String,
    pub statement_hash: String,
    #[serde(default)]
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BitVMVerificationResult {
    pub valid: bool,
    pub statement_hash: String,
    pub circuit_id: String,
    pub verification_key_id: String,
    pub steps_verified: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BitVMErrorResponse {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum BitVMError {
    #[error("malformed request: {0}")]
    Malformed(String),
    #[error("binding mismatch: {0}")]
    Binding(String),
    #[error("statement hash mismatch")]
    StatementHashMismatch,
    #[error("malformed proof encoding: {0}")]
    MalformedProof(String),
    #[error("proof was rejected by the BN254 verifier")]
    InvalidProof,
    #[error("verification key is unavailable")]
    VerificationKeyUnavailable,
    #[error("BN254 verifier is unavailable")]
    VerifierUnavailable,
    #[error("required audit persistence failed")]
    AuditPersistence,
    #[error("verifier operation failed: {0}")]
    Operation(String),
}

impl BitVMError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed(_) | Self::MalformedProof(_) => "malformed_request",
            Self::Binding(_) => "binding_mismatch",
            Self::StatementHashMismatch => "statement_hash_mismatch",
            Self::InvalidProof => "invalid_proof",
            Self::VerificationKeyUnavailable => "verification_key_unavailable",
            Self::VerifierUnavailable => "verifier_unavailable",
            Self::AuditPersistence => "audit_persistence_failed",
            Self::Operation(_) => "verification_operation_failed",
        }
    }

    pub fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::Malformed(_)
            | Self::Binding(_)
            | Self::StatementHashMismatch
            | Self::MalformedProof(_) => StatusCode::BAD_REQUEST,
            Self::InvalidProof => StatusCode::UNPROCESSABLE_ENTITY,
            Self::VerificationKeyUnavailable | Self::VerifierUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::AuditPersistence | Self::Operation(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn response(&self) -> BitVMErrorResponse {
        BitVMErrorResponse {
            code: self.code(),
            message: self.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct RegisteredVerificationKey {
    pub schema_version: u16,
    pub curve: String,
    pub circuit_id: String,
    pub verification_key_id: [u8; 32],
    pub enabled: bool,
    verifying_key: Arc<VerifyingKey<Bn254>>,
}

#[derive(Clone, Default)]
pub struct VerificationKeyRegistry {
    keys: HashMap<[u8; 32], RegisteredVerificationKey>,
}

impl VerificationKeyRegistry {
    pub fn from_config(entries: &[BitVmVerificationKeyConfig]) -> anyhow::Result<Self> {
        let mut keys = HashMap::new();
        for entry in entries {
            if entry.schema_version != SCHEMA_VERSION {
                anyhow::bail!("BitVM registry entry has unsupported schema_version");
            }
            if entry.curve != CURVE {
                anyhow::bail!("BitVM registry entry has unsupported curve");
            }
            if entry.circuit_id != CIRCUIT_ID {
                anyhow::bail!("BitVM registry entry has incorrect circuit association");
            }
            let supplied_id =
                decode_lower_hex::<32>(&entry.verification_key_id, "verification_key_id")
                    .map_err(anyhow::Error::msg)?;
            let vk_bytes = BASE64_STANDARD
                .decode(&entry.verification_key_base64)
                .map_err(|_| anyhow::anyhow!("BitVM verification key is not valid base64"))?;
            if vk_bytes.is_empty() || vk_bytes.len() > MAX_VERIFICATION_KEY_BYTES {
                anyhow::bail!("BitVM verification key size is outside the accepted range");
            }
            let derived_id = verification_key_id(&vk_bytes);
            if supplied_id != derived_id {
                anyhow::bail!("BitVM verification key ID does not match exact key bytes");
            }
            let mut remaining = vk_bytes.as_slice();
            let verifying_key = VerifyingKey::<Bn254>::deserialize_compressed(&mut remaining)
                .map_err(|_| anyhow::anyhow!("BitVM verification key canonical decode failed"))?;
            if !remaining.is_empty() {
                anyhow::bail!("BitVM verification key contains trailing bytes");
            }
            let mut canonical = Vec::new();
            verifying_key.serialize_compressed(&mut canonical)?;
            if canonical != vk_bytes {
                anyhow::bail!("BitVM verification key is not canonically encoded");
            }
            if verifying_key.gamma_abc_g1.len() != PUBLIC_INPUT_COUNT + 1 {
                anyhow::bail!("BitVM verification key does not accept exactly seven public inputs");
            }
            if keys.contains_key(&derived_id) {
                anyhow::bail!("duplicate BitVM verification key ID");
            }
            keys.insert(
                derived_id,
                RegisteredVerificationKey {
                    schema_version: entry.schema_version,
                    curve: entry.curve.clone(),
                    circuit_id: entry.circuit_id.clone(),
                    verification_key_id: derived_id,
                    enabled: entry.enabled,
                    verifying_key: Arc::new(verifying_key),
                },
            );
        }
        Ok(Self { keys })
    }

    pub fn resolve(
        &self,
        schema_version: u16,
        curve: &str,
        circuit_id: &str,
        verification_key_id: &[u8; 32],
    ) -> Result<RegisteredVerificationKey, BitVMError> {
        let key = self
            .keys
            .get(verification_key_id)
            .filter(|key| key.enabled)
            .ok_or(BitVMError::VerificationKeyUnavailable)?;
        if key.schema_version != schema_version
            || key.curve != curve
            || key.circuit_id != circuit_id
            || key.verification_key_id != *verification_key_id
        {
            return Err(BitVMError::VerificationKeyUnavailable);
        }
        Ok(key.clone())
    }
}

#[async_trait]
pub trait Bn254Verifier: Send + Sync {
    async fn verify(
        &self,
        key: &RegisteredVerificationKey,
        public_inputs: &[Fr],
        proof_bytes: &[u8],
    ) -> Result<bool, BitVMError>;
}

pub struct ArkworksBn254Verifier;

#[async_trait]
impl Bn254Verifier for ArkworksBn254Verifier {
    async fn verify(
        &self,
        key: &RegisteredVerificationKey,
        public_inputs: &[Fr],
        proof_bytes: &[u8],
    ) -> Result<bool, BitVMError> {
        if proof_bytes.len() != COMPRESSED_PROOF_BYTES {
            return Err(BitVMError::MalformedProof(
                "compressed BN254 proof must be exactly 128 bytes".to_string(),
            ));
        }
        let mut remaining = proof_bytes;
        let proof = Proof::<Bn254>::deserialize_compressed(&mut remaining).map_err(|_| {
            BitVMError::MalformedProof("Arkworks canonical proof decode failed".to_string())
        })?;
        if !remaining.is_empty() {
            return Err(BitVMError::MalformedProof(
                "compressed proof contains trailing bytes".to_string(),
            ));
        }
        let mut canonical = Vec::new();
        proof
            .serialize_compressed(&mut canonical)
            .map_err(|error| {
                BitVMError::Operation(format!("proof canonical serialization failed: {error}"))
            })?;
        if canonical != proof_bytes {
            return Err(BitVMError::MalformedProof(
                "proof is not canonically encoded".to_string(),
            ));
        }
        Groth16::<Bn254>::verify(&key.verifying_key, public_inputs, &proof)
            .map_err(|error| BitVMError::Operation(format!("Arkworks verifier error: {error}")))
    }
}

pub struct UnavailableBn254Verifier;

#[async_trait]
impl Bn254Verifier for UnavailableBn254Verifier {
    async fn verify(
        &self,
        _key: &RegisteredVerificationKey,
        _public_inputs: &[Fr],
        _proof_bytes: &[u8],
    ) -> Result<bool, BitVMError> {
        Err(BitVMError::VerifierUnavailable)
    }
}

#[derive(Debug, Clone)]
pub struct BitVMAuditRecord {
    pub statement_hash: String,
    pub schema_version: i32,
    pub curve: String,
    pub circuit_id: String,
    pub verification_key_id: String,
    pub prev_state_root: String,
    pub next_state_root: String,
    pub public_inputs_hash: String,
    pub proof_digest: String,
    pub witness_commitment: String,
    pub steps_verified: i64,
    pub bitcoin_network: String,
    pub bitcoin_anchor_height: i64,
    pub bitcoin_anchor_hash: String,
    pub bitcoin_max_valid_height: Option<i64>,
    pub trace_id: Option<String>,
}

#[async_trait]
pub trait BitVMAuditSink: Send + Sync {
    async fn persist(&self, record: &BitVMAuditRecord) -> Result<(), BitVMError>;
}

pub struct PostgresBitVMAuditSink {
    storage: Arc<Storage>,
}

impl PostgresBitVMAuditSink {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl BitVMAuditSink for PostgresBitVMAuditSink {
    async fn persist(&self, record: &BitVMAuditRecord) -> Result<(), BitVMError> {
        let result = sqlx::query(
            "INSERT INTO bitvm_groth16_v1_audit (
                statement_hash, schema_version, curve, circuit_id, verification_key_id,
                prev_state_root, next_state_root, public_inputs_hash, proof_digest,
                witness_commitment, steps_verified, bitcoin_network, bitcoin_anchor_height,
                bitcoin_anchor_hash, bitcoin_max_valid_height, trace_id
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
             ON CONFLICT (statement_hash) DO UPDATE
             SET statement_hash = EXCLUDED.statement_hash
             WHERE bitvm_groth16_v1_audit.schema_version = EXCLUDED.schema_version
               AND bitvm_groth16_v1_audit.curve = EXCLUDED.curve
               AND bitvm_groth16_v1_audit.circuit_id = EXCLUDED.circuit_id
               AND bitvm_groth16_v1_audit.verification_key_id = EXCLUDED.verification_key_id
               AND bitvm_groth16_v1_audit.prev_state_root = EXCLUDED.prev_state_root
               AND bitvm_groth16_v1_audit.next_state_root = EXCLUDED.next_state_root
               AND bitvm_groth16_v1_audit.public_inputs_hash = EXCLUDED.public_inputs_hash
               AND bitvm_groth16_v1_audit.proof_digest = EXCLUDED.proof_digest
               AND bitvm_groth16_v1_audit.witness_commitment = EXCLUDED.witness_commitment
               AND bitvm_groth16_v1_audit.steps_verified = EXCLUDED.steps_verified
               AND bitvm_groth16_v1_audit.bitcoin_network = EXCLUDED.bitcoin_network
               AND bitvm_groth16_v1_audit.bitcoin_anchor_height = EXCLUDED.bitcoin_anchor_height
               AND bitvm_groth16_v1_audit.bitcoin_anchor_hash = EXCLUDED.bitcoin_anchor_hash
               AND bitvm_groth16_v1_audit.bitcoin_max_valid_height IS NOT DISTINCT FROM EXCLUDED.bitcoin_max_valid_height",
        )
        .bind(&record.statement_hash)
        .bind(record.schema_version)
        .bind(&record.curve)
        .bind(&record.circuit_id)
        .bind(&record.verification_key_id)
        .bind(&record.prev_state_root)
        .bind(&record.next_state_root)
        .bind(&record.public_inputs_hash)
        .bind(&record.proof_digest)
        .bind(&record.witness_commitment)
        .bind(record.steps_verified)
        .bind(&record.bitcoin_network)
        .bind(record.bitcoin_anchor_height)
        .bind(&record.bitcoin_anchor_hash)
        .bind(record.bitcoin_max_valid_height)
        .bind(&record.trace_id)
        .execute(&self.storage.pg_pool)
        .await
        .map_err(|_| BitVMError::AuditPersistence)?;
        if result.rows_affected() != 1 {
            return Err(BitVMError::AuditPersistence);
        }
        Ok(())
    }
}

pub struct UnavailableBitVMAuditSink;

#[async_trait]
impl BitVMAuditSink for UnavailableBitVMAuditSink {
    async fn persist(&self, _record: &BitVMAuditRecord) -> Result<(), BitVMError> {
        Err(BitVMError::AuditPersistence)
    }
}

pub struct BitVMAdapter {
    registry: VerificationKeyRegistry,
    verifier: Arc<dyn Bn254Verifier>,
    audit_sink: Arc<dyn BitVMAuditSink>,
}

impl BitVMAdapter {
    pub fn unavailable() -> Self {
        Self {
            registry: VerificationKeyRegistry::default(),
            verifier: Arc::new(UnavailableBn254Verifier),
            audit_sink: Arc::new(UnavailableBitVMAuditSink),
        }
    }

    pub fn from_config(
        storage: Arc<Storage>,
        entries: &[BitVmVerificationKeyConfig],
    ) -> anyhow::Result<Self> {
        Ok(Self {
            registry: VerificationKeyRegistry::from_config(entries)?,
            verifier: Arc::new(ArkworksBn254Verifier),
            audit_sink: Arc::new(PostgresBitVMAuditSink::new(storage)),
        })
    }

    pub fn with_components(
        registry: VerificationKeyRegistry,
        verifier: Arc<dyn Bn254Verifier>,
        audit_sink: Arc<dyn BitVMAuditSink>,
    ) -> Self {
        Self {
            registry,
            verifier,
            audit_sink,
        }
    }

    pub async fn verify_transition(
        &self,
        transition: &BitVMTransition,
    ) -> Result<BitVMVerificationResult, BitVMError> {
        let validated = ValidatedTransition::parse(transition)?;
        let key = self.registry.resolve(
            validated.schema_version,
            CURVE,
            CIRCUIT_ID,
            &validated.verification_key_id,
        )?;
        let public_input_fields = validated
            .public_inputs
            .iter()
            .map(canonical_field_to_fr)
            .collect::<Result<Vec<_>, _>>()?;
        let valid = self
            .verifier
            .verify(&key, &public_input_fields, &validated.proof)
            .await?;
        if !valid {
            return Err(BitVMError::InvalidProof);
        }
        self.audit_sink.persist(&validated.audit_record()).await?;
        Ok(BitVMVerificationResult {
            valid: true,
            statement_hash: hex::encode(validated.statement_hash),
            circuit_id: CIRCUIT_ID.to_string(),
            verification_key_id: hex::encode(validated.verification_key_id),
            steps_verified: validated.steps_verified,
        })
    }
}

struct ValidatedTransition {
    schema_version: u16,
    verification_key_id: [u8; 32],
    prev_state_root: String,
    next_state_root: String,
    steps_verified: u64,
    witness_commitment: [u8; 32],
    public_inputs: [[u8; 32]; PUBLIC_INPUT_COUNT],
    block_context: CanonicalBlockContext,
    proof: [u8; COMPRESSED_PROOF_BYTES],
    statement_hash: [u8; 32],
    trace_id: Option<String>,
}

struct CanonicalBlockContext {
    network: BitcoinNetwork,
    block_height: u64,
    block_hash: [u8; 32],
    block_hash_hex: String,
    max_valid_height: Option<u64>,
}

impl ValidatedTransition {
    fn parse(raw: &BitVMTransition) -> Result<Self, BitVMError> {
        if raw.schema_version != SCHEMA_VERSION {
            return Err(BitVMError::Binding("schema_version must be 1".to_string()));
        }
        if raw.curve != CURVE {
            return Err(BitVMError::Binding("curve must be bn254".to_string()));
        }
        if raw.circuit_id != CIRCUIT_ID {
            return Err(BitVMError::Binding(
                "circuit_id does not match the fixed profile".to_string(),
            ));
        }
        let verification_key_id =
            decode_lower_hex::<32>(&raw.verification_key_id, "verification_key_id")
                .map_err(BitVMError::Malformed)?;
        if verification_key_id == [0u8; 32] {
            return Err(BitVMError::Malformed(
                "verification_key_id must be non-zero".to_string(),
            ));
        }
        let prev_root = decode_prefixed_root(&raw.prev_state_root, "prev_state_root")?;
        let next_root = decode_prefixed_root(&raw.next_state_root, "next_state_root")?;
        let witness_commitment =
            decode_lower_hex::<32>(&raw.witness_commitment, "witness_commitment")
                .map_err(BitVMError::Malformed)?;
        if witness_commitment == [0u8; 32] {
            return Err(BitVMError::Malformed(
                "witness_commitment must be non-zero".to_string(),
            ));
        }
        if raw.public_inputs.len() != PUBLIC_INPUT_COUNT {
            return Err(BitVMError::Binding(
                "public_inputs must contain exactly seven values".to_string(),
            ));
        }
        if raw.steps_verified > i64::MAX as u64 {
            return Err(BitVMError::Malformed(
                "steps_verified exceeds the durable audit range".to_string(),
            ));
        }
        let mut supplied = [[0u8; 32]; PUBLIC_INPUT_COUNT];
        for (index, value) in raw.public_inputs.iter().enumerate() {
            supplied[index] = decode_lower_hex::<32>(value, &format!("public_inputs[{index}]"))
                .map_err(BitVMError::Malformed)?;
            validate_canonical_field(&supplied[index], index)?;
        }
        let expected =
            canonical_public_inputs(prev_root, next_root, raw.steps_verified, witness_commitment);
        if supplied != expected {
            return Err(BitVMError::Binding(
                "ordered public_inputs do not match named transition fields".to_string(),
            ));
        }
        if raw.block_context.block_height == 0 {
            return Err(BitVMError::Malformed(
                "block_height must be greater than zero".to_string(),
            ));
        }
        if raw.block_context.block_height > i64::MAX as u64 {
            return Err(BitVMError::Malformed(
                "block_height exceeds the durable audit range".to_string(),
            ));
        }
        if let Some(max_valid_height) = raw.block_context.max_valid_height {
            if max_valid_height > i64::MAX as u64 {
                return Err(BitVMError::Malformed(
                    "max_valid_height exceeds the durable audit range".to_string(),
                ));
            }
            if max_valid_height < raw.block_context.block_height {
                return Err(BitVMError::Malformed(
                    "max_valid_height must be at least block_height".to_string(),
                ));
            }
        }
        let block_hash = decode_lower_hex::<32>(&raw.block_context.block_hash, "block_hash")
            .map_err(BitVMError::Malformed)?;
        if block_hash == [0u8; 32] {
            return Err(BitVMError::Malformed(
                "block_hash must be non-zero".to_string(),
            ));
        }
        let proof = decode_lower_hex::<COMPRESSED_PROOF_BYTES>(&raw.proof, "proof")
            .map_err(BitVMError::Malformed)?;
        if proof.iter().all(|byte| *byte == 0) {
            return Err(BitVMError::MalformedProof(
                "compressed proof must not be all zero bytes".to_string(),
            ));
        }
        let supplied_statement_hash = decode_lower_hex::<32>(&raw.statement_hash, "statement_hash")
            .map_err(BitVMError::Malformed)?;
        let validated = Self {
            schema_version: raw.schema_version,
            verification_key_id,
            prev_state_root: raw.prev_state_root.clone(),
            next_state_root: raw.next_state_root.clone(),
            steps_verified: raw.steps_verified,
            witness_commitment,
            public_inputs: supplied,
            block_context: CanonicalBlockContext {
                network: raw.block_context.network,
                block_height: raw.block_context.block_height,
                block_hash,
                block_hash_hex: raw.block_context.block_hash.clone(),
                max_valid_height: raw.block_context.max_valid_height,
            },
            proof,
            statement_hash: supplied_statement_hash,
            trace_id: raw
                .trace_id
                .clone()
                .filter(|value| !value.trim().is_empty()),
        };
        if validated.computed_statement_hash() != supplied_statement_hash {
            return Err(BitVMError::StatementHashMismatch);
        }
        Ok(validated)
    }

    fn canonical_statement(&self) -> Vec<u8> {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(STATEMENT_ENCODING_DOMAIN);
        encoded.extend_from_slice(&self.schema_version.to_be_bytes());
        encoded.push(1);
        encoded.push(FIELD_ENCODING_BN254_BIG_ENDIAN_32);
        encoded.extend_from_slice(&(CIRCUIT_ID.len() as u32).to_be_bytes());
        encoded.extend_from_slice(CIRCUIT_ID.as_bytes());
        encoded.extend_from_slice(&self.verification_key_id);
        encoded.extend_from_slice(&(PUBLIC_INPUT_COUNT as u32).to_be_bytes());
        for input in &self.public_inputs {
            encoded.extend_from_slice(input);
        }
        encoded.extend_from_slice(&self.witness_commitment);
        encoded.push(self.block_context.network.tag());
        encoded.extend_from_slice(&self.block_context.block_height.to_be_bytes());
        encoded.extend_from_slice(&self.block_context.block_hash);
        match self.block_context.max_valid_height {
            Some(height) => {
                encoded.push(1);
                encoded.extend_from_slice(&height.to_be_bytes());
            }
            None => encoded.push(0),
        }
        encoded
    }

    fn computed_statement_hash(&self) -> [u8; 32] {
        hash_domain_separated(STATEMENT_HASH_DOMAIN, &self.canonical_statement())
    }

    fn audit_record(&self) -> BitVMAuditRecord {
        let public_inputs = self.public_inputs.concat();
        BitVMAuditRecord {
            statement_hash: hex::encode(self.statement_hash),
            schema_version: i32::from(self.schema_version),
            curve: CURVE.to_string(),
            circuit_id: CIRCUIT_ID.to_string(),
            verification_key_id: hex::encode(self.verification_key_id),
            prev_state_root: self.prev_state_root.clone(),
            next_state_root: self.next_state_root.clone(),
            public_inputs_hash: hex::encode(Sha256::digest(public_inputs)),
            proof_digest: hex::encode(hash_domain_separated(PROOF_DIGEST_DOMAIN, &self.proof)),
            witness_commitment: hex::encode(self.witness_commitment),
            steps_verified: self.steps_verified as i64,
            bitcoin_network: self.block_context.network.to_string(),
            bitcoin_anchor_height: self.block_context.block_height as i64,
            bitcoin_anchor_hash: self.block_context.block_hash_hex.clone(),
            bitcoin_max_valid_height: self
                .block_context
                .max_valid_height
                .map(|height| height as i64),
            trace_id: self.trace_id.clone(),
        }
    }
}

pub fn canonical_public_inputs(
    prev_state_root: [u8; 32],
    next_state_root: [u8; 32],
    steps_verified: u64,
    witness_commitment: [u8; 32],
) -> [[u8; 32]; PUBLIC_INPUT_COUNT] {
    let mut steps = [0u8; 32];
    steps[24..].copy_from_slice(&steps_verified.to_be_bytes());
    [
        limb(prev_state_root, true),
        limb(prev_state_root, false),
        limb(next_state_root, true),
        limb(next_state_root, false),
        steps,
        limb(witness_commitment, true),
        limb(witness_commitment, false),
    ]
}

fn limb(value: [u8; 32], high: bool) -> [u8; 32] {
    let mut encoded = [0u8; 32];
    let source = if high { &value[..16] } else { &value[16..] };
    encoded[16..].copy_from_slice(source);
    encoded
}

fn decode_prefixed_root(value: &str, field: &str) -> Result<[u8; 32], BitVMError> {
    let Some(raw) = value.strip_prefix("0x") else {
        return Err(BitVMError::Malformed(format!("{field} must start with 0x")));
    };
    decode_lower_hex::<32>(raw, field).map_err(BitVMError::Malformed)
}

fn decode_lower_hex<const N: usize>(value: &str, field: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 {
        return Err(format!(
            "{field} must contain exactly {} lowercase hexadecimal characters",
            N * 2
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{field} must use lowercase hexadecimal without a prefix"
        ));
    }
    let bytes = hex::decode(value).map_err(|_| format!("{field} is not valid hexadecimal"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{field} has the wrong decoded length"))
}

fn validate_canonical_field(value: &[u8; 32], index: usize) -> Result<(), BitVMError> {
    if value >= &BN254_SCALAR_MODULUS {
        return Err(BitVMError::Malformed(format!(
            "public_inputs[{index}] is not below the BN254 scalar modulus"
        )));
    }
    Ok(())
}

fn canonical_field_to_fr(value: &[u8; 32]) -> Result<Fr, BitVMError> {
    let bits = value
        .iter()
        .flat_map(|byte| (0..8).rev().map(move |bit| (byte >> bit) & 1 == 1))
        .collect::<Vec<_>>();
    let integer = BigInt::<4>::from_bits_be(&bits);
    Fr::from_bigint(integer)
        .ok_or_else(|| BitVMError::Malformed("canonical BN254 field conversion failed".to_string()))
}

pub fn verification_key_id(vk_bytes: &[u8]) -> [u8; 32] {
    hash_domain_separated(VERIFICATION_KEY_ID_DOMAIN, vk_bytes)
}

pub fn canonical_statement_hash(
    verification_key_id: [u8; 32],
    public_inputs: &[[u8; 32]; PUBLIC_INPUT_COUNT],
    witness_commitment: [u8; 32],
    block_context: &BitcoinBlockContext,
) -> Result<[u8; 32], BitVMError> {
    if block_context.block_height == 0 {
        return Err(BitVMError::Malformed(
            "block_height must be greater than zero".to_string(),
        ));
    }
    if let Some(max_valid_height) = block_context.max_valid_height {
        if max_valid_height < block_context.block_height {
            return Err(BitVMError::Malformed(
                "max_valid_height must be at least block_height".to_string(),
            ));
        }
    }
    let block_hash = decode_lower_hex::<32>(&block_context.block_hash, "block_hash")
        .map_err(BitVMError::Malformed)?;
    let mut encoded = Vec::new();
    encoded.extend_from_slice(STATEMENT_ENCODING_DOMAIN);
    encoded.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    encoded.push(1);
    encoded.push(FIELD_ENCODING_BN254_BIG_ENDIAN_32);
    encoded.extend_from_slice(&(CIRCUIT_ID.len() as u32).to_be_bytes());
    encoded.extend_from_slice(CIRCUIT_ID.as_bytes());
    encoded.extend_from_slice(&verification_key_id);
    encoded.extend_from_slice(&(PUBLIC_INPUT_COUNT as u32).to_be_bytes());
    for input in public_inputs {
        encoded.extend_from_slice(input);
    }
    encoded.extend_from_slice(&witness_commitment);
    encoded.push(block_context.network.tag());
    encoded.extend_from_slice(&block_context.block_height.to_be_bytes());
    encoded.extend_from_slice(&block_hash);
    match block_context.max_valid_height {
        Some(height) => {
            encoded.push(1);
            encoded.extend_from_slice(&height.to_be_bytes());
        }
        None => encoded.push(0),
    }
    Ok(hash_domain_separated(STATEMENT_HASH_DOMAIN, &encoded))
}

fn hash_domain_separated(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_inputs_use_high_low_limbs_and_authenticated_steps() {
        let prev = [0x11; 32];
        let next = [0x22; 32];
        let witness = [0x33; 32];
        let inputs = canonical_public_inputs(prev, next, 42, witness);
        assert_eq!(&inputs[0][16..], &prev[..16]);
        assert_eq!(&inputs[1][16..], &prev[16..]);
        assert_eq!(&inputs[2][16..], &next[..16]);
        assert_eq!(&inputs[3][16..], &next[16..]);
        assert_eq!(&inputs[4][24..], &42u64.to_be_bytes());
        assert_eq!(&inputs[5][16..], &witness[..16]);
        assert_eq!(&inputs[6][16..], &witness[16..]);
    }

    #[test]
    fn modulus_is_rejected_without_reduction() {
        assert!(validate_canonical_field(&BN254_SCALAR_MODULUS, 0).is_err());
    }
}
