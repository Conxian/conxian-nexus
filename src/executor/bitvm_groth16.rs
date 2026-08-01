//! Canonical BN254 Groth16 boundary for Nexus state transitions.
//!
//! This is intentionally separate from the legacy BLS12-381 BitVM route. It
//! implements Gateway schema v1, selects verification keys only from trusted
//! startup configuration, and performs real Arkworks pairing verification.

use ark_bn254::{Bn254, Fr};
use ark_ff::PrimeField;
use ark_groth16::{prepare_verifying_key, Groth16, PreparedVerifyingKey, Proof, VerifyingKey};
use ark_serialize::CanonicalDeserialize;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io::Cursor, sync::Arc};
use thiserror::Error;

pub const GROTH16_SCHEMA_VERSION: u16 = 1;
pub const NEXUS_STATE_TRANSITION_CIRCUIT_ID: &str = "conxian-nexus-state-transition-bn254-v1";
pub const NEXUS_STATE_TRANSITION_PUBLIC_INPUTS: usize = 12;
pub const BN254_FIELD_ELEMENT_BYTES: usize = 32;
pub const GROTH16_COMPRESSED_PROOF_BYTES: usize = 128;
pub const MAX_VERIFICATION_KEY_BYTES: usize = 1024 * 1024;

const FIELD_ENCODING_BN254_BIG_ENDIAN_32: u8 = 1;
const STATEMENT_ENCODING_DOMAIN: &[u8] = b"CONXIAN-GROTH16-STATEMENT-ENCODING-V1";
const STATEMENT_HASH_DOMAIN: &[u8] = b"CONXIAN-GROTH16-STATEMENT-HASH-V1";
const VERIFICATION_KEY_ID_DOMAIN: &[u8] = b"CONXIAN-GROTH16-VERIFICATION-KEY-ID-V1";

/// BN254 scalar modulus in the Gateway contract's fixed-width big-endian form.
pub const BN254_SCALAR_MODULUS: [u8; 32] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x4e, 0x3e, 0x1f, 0x59, 0x3f, 0x00, 0x00, 0x01,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Groth16Curve {
    Bn254,
}

impl Groth16Curve {
    fn tag(self) -> u8 {
        match self {
            Self::Bn254 => 1,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bn254 => "bn254",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VerificationKeyId(pub [u8; 32]);

impl VerificationKeyId {
    pub fn from_key_bytes(bytes: &[u8]) -> Result<Self, CanonicalGroth16Error> {
        if bytes.is_empty() || bytes.len() > MAX_VERIFICATION_KEY_BYTES {
            return Err(CanonicalGroth16Error::InvalidVerificationKey(
                "verification key must contain 1..=1048576 bytes".to_owned(),
            ));
        }
        Ok(Self(domain_separated_hash(
            VERIFICATION_KEY_ID_DOMAIN,
            bytes,
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldElement([u8; 32]);

impl FieldElement {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, CanonicalGroth16Error> {
        if bytes >= BN254_SCALAR_MODULUS {
            return Err(CanonicalGroth16Error::NonCanonicalFieldElement);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn as_fr(&self) -> Fr {
        Fr::from_be_bytes_mod_order(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

impl BitcoinNetwork {
    pub fn parse(value: &str) -> Result<Self, CanonicalGroth16Error> {
        match value {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            "signet" => Ok(Self::Signet),
            "regtest" => Ok(Self::Regtest),
            other => Err(CanonicalGroth16Error::InvalidBlockContext(format!(
                "unsupported Bitcoin network `{other}`"
            ))),
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Mainnet => 1,
            Self::Testnet => 2,
            Self::Signet => 3,
            Self::Regtest => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Signet => "signet",
            Self::Regtest => "regtest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitcoinBlockContext {
    pub network: BitcoinNetwork,
    pub block_height: u64,
    /// Canonical Bitcoin display order, matching Gateway's envelope.
    pub block_hash: [u8; 32],
    pub max_valid_height: Option<u64>,
}

impl BitcoinBlockContext {
    fn validate_at(&self, current_height: u64) -> Result<(), CanonicalGroth16Error> {
        if self.block_height == 0 || self.block_hash == [0; 32] {
            return Err(CanonicalGroth16Error::InvalidBlockContext(
                "anchor height and block hash must be non-zero".to_owned(),
            ));
        }
        if current_height < self.block_height {
            return Err(CanonicalGroth16Error::ProofFromFuture {
                current_height,
                anchor_height: self.block_height,
            });
        }
        if let Some(max_height) = self.max_valid_height {
            if max_height < self.block_height {
                return Err(CanonicalGroth16Error::InvalidBlockContext(
                    "max_valid_height must be at least block_height".to_owned(),
                ));
            }
            if current_height > max_height {
                return Err(CanonicalGroth16Error::ProofExpired {
                    current_height,
                    max_valid_height: max_height,
                });
            }
        }
        Ok(())
    }
}

/// Named Nexus state roots supplied by the transition layer, not copied from
/// caller-controlled public-input slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NexusStateTransition {
    pub prev_state_root: [u8; 32],
    pub next_state_root: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayGroth16Envelope {
    pub schema_version: u16,
    pub curve: Groth16Curve,
    pub circuit_id: String,
    pub verification_key_id: VerificationKeyId,
    pub public_inputs: Vec<FieldElement>,
    pub witness_commitment: [u8; 32],
    pub block_context: BitcoinBlockContext,
    pub proof: Vec<u8>,
    pub statement_hash: [u8; 32],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGatewayEnvelope {
    schema_version: u16,
    curve: String,
    circuit_id: String,
    verification_key_id: String,
    public_inputs: Vec<String>,
    witness_commitment: String,
    block_context: RawBlockContext,
    proof: String,
    statement_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBlockContext {
    network: String,
    block_height: u64,
    block_hash: String,
    max_valid_height: Option<u64>,
}

/// Parse Gateway schema v1 exactly. Unknown fields and raw witness material
/// are rejected before cryptographic work.
pub fn parse_gateway_envelope_json(
    value: Value,
) -> Result<GatewayGroth16Envelope, CanonicalGroth16Error> {
    if value.get("witness").is_some() || value.get("raw_witness").is_some() {
        return Err(CanonicalGroth16Error::RawWitnessProvided);
    }
    let raw: RawGatewayEnvelope = serde_json::from_value(value)
        .map_err(|error| CanonicalGroth16Error::MalformedEnvelope(error.to_string()))?;
    if raw.schema_version != GROTH16_SCHEMA_VERSION {
        return Err(CanonicalGroth16Error::UnsupportedSchemaVersion(
            raw.schema_version,
        ));
    }
    let curve = match raw.curve.as_str() {
        "bn254" => Groth16Curve::Bn254,
        other => return Err(CanonicalGroth16Error::UnsupportedCurve(other.to_owned())),
    };
    if raw.public_inputs.len() != NEXUS_STATE_TRANSITION_PUBLIC_INPUTS {
        return Err(CanonicalGroth16Error::PublicInputCount {
            expected: NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
            found: raw.public_inputs.len(),
        });
    }
    let public_inputs = raw
        .public_inputs
        .iter()
        .map(|value| decode_fixed_hex(value).and_then(FieldElement::from_bytes))
        .collect::<Result<Vec<_>, _>>()?;
    if raw.proof.len() != GROTH16_COMPRESSED_PROOF_BYTES * 2 {
        return Err(CanonicalGroth16Error::InvalidProofEncoding(format!(
            "proof must be exactly {} hexadecimal characters",
            GROTH16_COMPRESSED_PROOF_BYTES * 2
        )));
    }
    let proof = decode_hex(&raw.proof, "proof")?;
    if proof.iter().all(|byte| *byte == 0) {
        return Err(CanonicalGroth16Error::AllZeroProof);
    }
    Ok(GatewayGroth16Envelope {
        schema_version: raw.schema_version,
        curve,
        circuit_id: raw.circuit_id,
        verification_key_id: VerificationKeyId(decode_fixed_hex(&raw.verification_key_id)?),
        public_inputs,
        witness_commitment: decode_fixed_hex(&raw.witness_commitment)?,
        block_context: BitcoinBlockContext {
            network: BitcoinNetwork::parse(&raw.block_context.network)?,
            block_height: raw.block_context.block_height,
            block_hash: decode_fixed_hex(&raw.block_context.block_hash)?,
            max_valid_height: raw.block_context.max_valid_height,
        },
        proof,
        statement_hash: decode_fixed_hex(&raw.statement_hash)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicInputLayout {
    NexusStateTransitionV1,
}

#[derive(Debug, Clone)]
pub struct TrustedVerificationKeyConfig {
    pub schema_version: u16,
    pub curve: Groth16Curve,
    pub circuit_id: String,
    pub verification_key_id: VerificationKeyId,
    pub public_input_count: usize,
    pub public_input_layout: PublicInputLayout,
    pub enabled: bool,
    pub verification_key_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RegistryKey {
    schema_version: u16,
    curve: Groth16Curve,
    circuit_id: String,
    verification_key_id: VerificationKeyId,
}

#[derive(Debug)]
struct TrustedVerificationKey {
    key: RegistryKey,
    public_input_count: usize,
    public_input_layout: PublicInputLayout,
    enabled: bool,
    canonical_bytes: Vec<u8>,
    prepared: PreparedVerifyingKey<Bn254>,
}

/// Trusted startup/config registry. Runtime verification accepts only IDs and
/// never caller-supplied verification-key bytes.
#[derive(Debug, Default)]
pub struct TrustedVerificationKeyRegistry {
    entries: HashMap<RegistryKey, Arc<TrustedVerificationKey>>,
}

impl TrustedVerificationKeyRegistry {
    pub fn register(
        &mut self,
        config: TrustedVerificationKeyConfig,
    ) -> Result<(), CanonicalGroth16Error> {
        if config.schema_version != GROTH16_SCHEMA_VERSION {
            return Err(CanonicalGroth16Error::UnsupportedSchemaVersion(
                config.schema_version,
            ));
        }
        if config.curve != Groth16Curve::Bn254 {
            return Err(CanonicalGroth16Error::UnsupportedCurve(
                "registry curve is not BN254".to_owned(),
            ));
        }
        validate_circuit_id(&config.circuit_id)?;
        if config.public_input_count != NEXUS_STATE_TRANSITION_PUBLIC_INPUTS
            || config.public_input_layout != PublicInputLayout::NexusStateTransitionV1
        {
            return Err(CanonicalGroth16Error::RegistryLayoutMismatch);
        }
        let derived_id = VerificationKeyId::from_key_bytes(&config.verification_key_bytes)?;
        if derived_id != config.verification_key_id {
            return Err(CanonicalGroth16Error::VerificationKeyIdMismatch {
                supplied: config.verification_key_id,
                derived: derived_id,
            });
        }
        let vk: VerifyingKey<Bn254> = deserialize_exact(
            &config.verification_key_bytes,
            "verification key",
            CanonicalGroth16Error::InvalidVerificationKey,
        )?;
        let key = RegistryKey {
            schema_version: config.schema_version,
            curve: config.curve,
            circuit_id: config.circuit_id,
            verification_key_id: config.verification_key_id,
        };
        if let Some(existing) = self
            .entries
            .values()
            .find(|entry| entry.key.verification_key_id == config.verification_key_id)
        {
            let same_association = existing.key == key
                && existing.public_input_count == config.public_input_count
                && existing.public_input_layout == config.public_input_layout
                && existing.enabled == config.enabled;
            return Err(if same_association {
                CanonicalGroth16Error::DuplicateVerificationKey
            } else {
                CanonicalGroth16Error::ConflictingVerificationKeyAssociation
            });
        }
        let entry = TrustedVerificationKey {
            key: key.clone(),
            public_input_count: config.public_input_count,
            public_input_layout: config.public_input_layout,
            enabled: config.enabled,
            canonical_bytes: config.verification_key_bytes,
            prepared: prepare_verifying_key(&vk),
        };
        self.entries.insert(key, Arc::new(entry));
        Ok(())
    }

    fn resolve(
        &self,
        envelope: &GatewayGroth16Envelope,
    ) -> Result<Arc<TrustedVerificationKey>, CanonicalGroth16Error> {
        let requested = RegistryKey {
            schema_version: envelope.schema_version,
            curve: envelope.curve,
            circuit_id: envelope.circuit_id.clone(),
            verification_key_id: envelope.verification_key_id,
        };
        if let Some(entry) = self.entries.get(&requested) {
            if !entry.enabled {
                return Err(CanonicalGroth16Error::VerificationKeyDisabled);
            }
            if entry.public_input_count != NEXUS_STATE_TRANSITION_PUBLIC_INPUTS
                || entry.public_input_layout != PublicInputLayout::NexusStateTransitionV1
            {
                return Err(CanonicalGroth16Error::RegistryLayoutMismatch);
            }
            let current_id = VerificationKeyId::from_key_bytes(&entry.canonical_bytes)?;
            if current_id != entry.key.verification_key_id {
                return Err(CanonicalGroth16Error::RegistryIntegrityMismatch);
            }
            return Ok(Arc::clone(entry));
        }
        if self
            .entries
            .keys()
            .any(|key| key.verification_key_id == envelope.verification_key_id)
        {
            return Err(CanonicalGroth16Error::VerificationKeyAssociationMismatch);
        }
        Err(CanonicalGroth16Error::VerificationKeyNotFound(
            envelope.verification_key_id,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReceipt {
    pub statement_hash: [u8; 32],
    pub verification_key_id: VerificationKeyId,
}

pub struct CanonicalStateTransitionVerifier {
    registry: Arc<TrustedVerificationKeyRegistry>,
}

impl CanonicalStateTransitionVerifier {
    pub fn new(registry: Arc<TrustedVerificationKeyRegistry>) -> Self {
        Self { registry }
    }

    pub fn verify(
        &self,
        transition: &NexusStateTransition,
        envelope: &GatewayGroth16Envelope,
        current_block_height: u64,
    ) -> Result<VerificationReceipt, CanonicalGroth16Error> {
        if current_block_height == 0 {
            return Err(CanonicalGroth16Error::InvalidCurrentBlockHeight);
        }
        if envelope.schema_version != GROTH16_SCHEMA_VERSION {
            return Err(CanonicalGroth16Error::UnsupportedSchemaVersion(
                envelope.schema_version,
            ));
        }
        if envelope.curve != Groth16Curve::Bn254 {
            return Err(CanonicalGroth16Error::UnsupportedCurve(
                "only BN254 is canonical".to_owned(),
            ));
        }
        if envelope.circuit_id != NEXUS_STATE_TRANSITION_CIRCUIT_ID {
            return Err(CanonicalGroth16Error::CircuitMismatch {
                expected: NEXUS_STATE_TRANSITION_CIRCUIT_ID.to_owned(),
                found: envelope.circuit_id.clone(),
            });
        }
        envelope.block_context.validate_at(current_block_height)?;
        if envelope.witness_commitment == [0; 32] {
            return Err(CanonicalGroth16Error::InvalidWitnessCommitment);
        }
        if envelope.proof.len() != GROTH16_COMPRESSED_PROOF_BYTES {
            return Err(CanonicalGroth16Error::InvalidProofEncoding(format!(
                "proof must be exactly {GROTH16_COMPRESSED_PROOF_BYTES} bytes"
            )));
        }
        if envelope.proof.iter().all(|byte| *byte == 0) {
            return Err(CanonicalGroth16Error::AllZeroProof);
        }
        if envelope.public_inputs.len() != NEXUS_STATE_TRANSITION_PUBLIC_INPUTS {
            return Err(CanonicalGroth16Error::PublicInputCount {
                expected: NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
                found: envelope.public_inputs.len(),
            });
        }
        let expected_inputs = derive_public_inputs(
            transition,
            &envelope.block_context,
            envelope.witness_commitment,
        )?;
        if envelope.public_inputs.as_slice() != expected_inputs.as_slice() {
            let slot = envelope
                .public_inputs
                .iter()
                .zip(expected_inputs.iter())
                .position(|(found, expected)| found != expected)
                .unwrap_or(0);
            return Err(CanonicalGroth16Error::PublicInputMismatch { slot });
        }
        let computed_hash = statement_hash(envelope)?;
        if computed_hash != envelope.statement_hash {
            return Err(CanonicalGroth16Error::StatementHashMismatch {
                expected: computed_hash,
                supplied: envelope.statement_hash,
            });
        }
        let trusted_key = self.registry.resolve(envelope)?;
        let proof: Proof<Bn254> = deserialize_exact(
            &envelope.proof,
            "proof",
            CanonicalGroth16Error::InvalidProofEncoding,
        )?;
        let inputs = envelope
            .public_inputs
            .iter()
            .map(FieldElement::as_fr)
            .collect::<Vec<_>>();
        let valid = Groth16::<Bn254>::verify_proof(&trusted_key.prepared, &proof, &inputs)
            .map_err(|error| CanonicalGroth16Error::PairingVerification(error.to_string()))?;
        if !valid {
            return Err(CanonicalGroth16Error::InvalidProof);
        }
        Ok(VerificationReceipt {
            statement_hash: computed_hash,
            verification_key_id: envelope.verification_key_id,
        })
    }
}

/// Derive the exact Nexus profile-v1 input vector. Every 32-byte root/hash is
/// split into high/low 128-bit limbs without field reduction.
pub fn derive_public_inputs(
    transition: &NexusStateTransition,
    context: &BitcoinBlockContext,
    witness_commitment: [u8; 32],
) -> Result<[FieldElement; 12], CanonicalGroth16Error> {
    let [prev_hi, prev_lo] = split_32(transition.prev_state_root)?;
    let [next_hi, next_lo] = split_32(transition.next_state_root)?;
    let [block_hi, block_lo] = split_32(context.block_hash)?;
    let [witness_hi, witness_lo] = split_32(witness_commitment)?;
    Ok([
        prev_hi,
        prev_lo,
        next_hi,
        next_lo,
        field_from_u64(context.network.tag() as u64)?,
        field_from_u64(context.block_height)?,
        block_hi,
        block_lo,
        field_from_u64(u64::from(context.max_valid_height.is_some()))?,
        field_from_u64(context.max_valid_height.unwrap_or(0))?,
        witness_hi,
        witness_lo,
    ])
}

pub fn statement_hash(
    envelope: &GatewayGroth16Envelope,
) -> Result<[u8; 32], CanonicalGroth16Error> {
    let encoded = canonical_statement_encode(envelope)?;
    Ok(domain_separated_hash(STATEMENT_HASH_DOMAIN, &encoded))
}

fn canonical_statement_encode(
    envelope: &GatewayGroth16Envelope,
) -> Result<Vec<u8>, CanonicalGroth16Error> {
    validate_circuit_id(&envelope.circuit_id)?;
    if envelope.public_inputs.len() != NEXUS_STATE_TRANSITION_PUBLIC_INPUTS {
        return Err(CanonicalGroth16Error::PublicInputCount {
            expected: NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
            found: envelope.public_inputs.len(),
        });
    }
    let mut encoded = Vec::new();
    encoded.extend_from_slice(STATEMENT_ENCODING_DOMAIN);
    encoded.extend_from_slice(&envelope.schema_version.to_be_bytes());
    encoded.push(envelope.curve.tag());
    encoded.push(FIELD_ENCODING_BN254_BIG_ENDIAN_32);
    append_len(&mut encoded, envelope.circuit_id.as_bytes())?;
    encoded.extend_from_slice(&envelope.verification_key_id.0);
    append_u32(&mut encoded, envelope.public_inputs.len())?;
    for input in &envelope.public_inputs {
        encoded.extend_from_slice(input.as_bytes());
    }
    encoded.extend_from_slice(&envelope.witness_commitment);
    encoded.push(envelope.block_context.network.tag());
    encoded.extend_from_slice(&envelope.block_context.block_height.to_be_bytes());
    encoded.extend_from_slice(&envelope.block_context.block_hash);
    match envelope.block_context.max_valid_height {
        Some(height) => {
            encoded.push(1);
            encoded.extend_from_slice(&height.to_be_bytes());
        }
        None => encoded.push(0),
    }
    Ok(encoded)
}

fn split_32(bytes: [u8; 32]) -> Result<[FieldElement; 2], CanonicalGroth16Error> {
    let mut high = [0u8; 32];
    let mut low = [0u8; 32];
    high[16..].copy_from_slice(&bytes[..16]);
    low[16..].copy_from_slice(&bytes[16..]);
    Ok([
        FieldElement::from_bytes(high)?,
        FieldElement::from_bytes(low)?,
    ])
}

fn field_from_u64(value: u64) -> Result<FieldElement, CanonicalGroth16Error> {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    FieldElement::from_bytes(bytes)
}

fn validate_circuit_id(value: &str) -> Result<(), CanonicalGroth16Error> {
    if value.is_empty()
        || value.len() > 128
        || !value.is_ascii()
        || value.chars().any(|character| !character.is_ascii_graphic())
    {
        return Err(CanonicalGroth16Error::InvalidCircuitId);
    }
    Ok(())
}

fn domain_separated_hash(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

fn append_len(target: &mut Vec<u8>, bytes: &[u8]) -> Result<(), CanonicalGroth16Error> {
    append_u32(target, bytes.len())?;
    target.extend_from_slice(bytes);
    Ok(())
}

fn append_u32(target: &mut Vec<u8>, value: usize) -> Result<(), CanonicalGroth16Error> {
    let value = u32::try_from(value).map_err(|_| CanonicalGroth16Error::LengthOverflow)?;
    target.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn decode_fixed_hex<const N: usize>(value: &str) -> Result<[u8; N], CanonicalGroth16Error> {
    if value.len() != N * 2 {
        return Err(CanonicalGroth16Error::MalformedEnvelope(format!(
            "expected exactly {} hexadecimal characters",
            N * 2
        )));
    }
    decode_hex(value, "fixed-width field")?
        .try_into()
        .map_err(|_| CanonicalGroth16Error::MalformedEnvelope("wrong decoded width".to_owned()))
}

fn decode_hex(value: &str, field: &str) -> Result<Vec<u8>, CanonicalGroth16Error> {
    hex::decode(value)
        .map_err(|error| CanonicalGroth16Error::MalformedEnvelope(format!("{field}: {error}")))
}

fn deserialize_exact<T, F>(bytes: &[u8], label: &str, error: F) -> Result<T, CanonicalGroth16Error>
where
    T: CanonicalDeserialize,
    F: Fn(String) -> CanonicalGroth16Error,
{
    let mut cursor = Cursor::new(bytes);
    let value = T::deserialize_compressed(&mut cursor)
        .map_err(|source| error(format!("failed to deserialize {label}: {source}")))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(error(format!("trailing bytes after canonical {label}")));
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanonicalGroth16Error {
    #[error("unsupported Groth16 schema version {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("unsupported Groth16 curve: {0}")]
    UnsupportedCurve(String),
    #[error("invalid circuit identifier")]
    InvalidCircuitId,
    #[error("circuit mismatch: expected {expected}, found {found}")]
    CircuitMismatch { expected: String, found: String },
    #[error("non-canonical BN254 scalar field element")]
    NonCanonicalFieldElement,
    #[error("public input count mismatch: expected {expected}, found {found}")]
    PublicInputCount { expected: usize, found: usize },
    #[error("public input mismatch at slot {slot}")]
    PublicInputMismatch { slot: usize },
    #[error("invalid witness commitment")]
    InvalidWitnessCommitment,
    #[error("raw witness material is not accepted")]
    RawWitnessProvided,
    #[error("invalid Bitcoin block context: {0}")]
    InvalidBlockContext(String),
    #[error("proof anchor {anchor_height} is above current height {current_height}")]
    ProofFromFuture {
        current_height: u64,
        anchor_height: u64,
    },
    #[error("proof expired at {max_valid_height}; current height is {current_height}")]
    ProofExpired {
        current_height: u64,
        max_valid_height: u64,
    },
    #[error("statement hash mismatch")]
    StatementHashMismatch {
        expected: [u8; 32],
        supplied: [u8; 32],
    },
    #[error("invalid verification key: {0}")]
    InvalidVerificationKey(String),
    #[error("verification key id mismatch")]
    VerificationKeyIdMismatch {
        supplied: VerificationKeyId,
        derived: VerificationKeyId,
    },
    #[error("verification key is not registered: {0:?}")]
    VerificationKeyNotFound(VerificationKeyId),
    #[error("verification key is registered for a different schema/curve/circuit")]
    VerificationKeyAssociationMismatch,
    #[error("verification key is disabled")]
    VerificationKeyDisabled,
    #[error("trusted verification-key registry layout mismatch")]
    RegistryLayoutMismatch,
    #[error("trusted verification-key registry integrity mismatch")]
    RegistryIntegrityMismatch,
    #[error("duplicate trusted verification-key entry")]
    DuplicateVerificationKey,
    #[error("verification key id has a conflicting trusted association")]
    ConflictingVerificationKeyAssociation,
    #[error("trusted current Bitcoin height must be non-zero")]
    InvalidCurrentBlockHeight,
    #[error("all-zero Groth16 proof is invalid")]
    AllZeroProof,
    #[error("invalid proof encoding: {0}")]
    InvalidProofEncoding(String),
    #[error("Groth16 pairing verification failed: {0}")]
    PairingVerification(String),
    #[error("Groth16 proof is invalid")]
    InvalidProof,
    #[error("malformed Gateway Groth16 envelope: {0}")]
    MalformedEnvelope(String),
    #[error("canonical length exceeds u32")]
    LengthOverflow,
}
