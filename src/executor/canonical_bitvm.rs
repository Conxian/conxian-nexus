//! Runtime boundary for canonical BitVM2 verification and immutable audit.

use crate::executor::bitvm_groth16::{
    BitcoinNetwork, CanonicalGroth16Error, CanonicalStateTransitionVerifier,
    GatewayGroth16Envelope, Groth16Curve, NexusStateTransition, VerificationKeyId,
};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use thiserror::Error;

const RECEIPT_ID_DOMAIN: &[u8] = b"CONXIAN-NEXUS-CANONICAL-BITVM-RECEIPT-ID-V1";
const PROOF_DIGEST_DOMAIN: &[u8] = b"CONXIAN-NEXUS-CANONICAL-BITVM-PROOF-DIGEST-V1";
pub const BACKEND_IDENTITY: &str = "arkworks-groth16-bn254";
pub const BACKEND_VERSION: &str = "0.6.0";

pub trait TrustedBitcoinHeightProvider: Send + Sync {
    fn current_height(&self) -> BoxFuture<'_, Result<u64, TrustedHeightError>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TrustedHeightError {
    #[error("trusted Bitcoin height source is unavailable")]
    Unavailable,
    #[error("trusted Bitcoin height source returned an invalid height")]
    Invalid,
}

#[derive(Debug, Default)]
pub struct UnavailableBitcoinHeightProvider;

impl TrustedBitcoinHeightProvider for UnavailableBitcoinHeightProvider {
    fn current_height(&self) -> BoxFuture<'_, Result<u64, TrustedHeightError>> {
        Box::pin(async { Err(TrustedHeightError::Unavailable) })
    }
}

pub trait CanonicalBitvmReceiptStore: Send + Sync {
    fn persist_immutable(
        &self,
        record: CanonicalBitvmAuditRecord,
    ) -> BoxFuture<'_, Result<AuditPersistOutcome, CanonicalBitvmAuditError>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditPersistOutcome {
    Created,
    Existing,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanonicalBitvmAuditError {
    #[error("canonical BitVM audit store is unavailable")]
    Unavailable,
    #[error("canonical BitVM audit record conflicts with immutable data")]
    IntegrityConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBitvmAuditRecord {
    pub receipt_id: String,
    pub schema_version: u16,
    pub curve: Groth16Curve,
    pub circuit_id: String,
    pub verification_key_id: VerificationKeyId,
    pub statement_hash: [u8; 32],
    pub proof_digest: [u8; 32],
    pub previous_state_root: [u8; 32],
    pub next_state_root: [u8; 32],
    pub witness_commitment: [u8; 32],
    pub network: BitcoinNetwork,
    pub anchor_block_height: u64,
    pub anchor_block_hash: [u8; 32],
    pub max_valid_height: Option<u64>,
    pub backend_identity: &'static str,
    pub backend_version: &'static str,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBitvmVerifiedReceipt {
    pub receipt_id: String,
    pub statement_hash: [u8; 32],
    pub verification_key_id: VerificationKeyId,
    pub schema_version: u16,
    pub curve: Groth16Curve,
    pub circuit_id: String,
    pub created: bool,
}

pub struct CanonicalBitvmService {
    verifier: Arc<CanonicalStateTransitionVerifier>,
    expected_network: BitcoinNetwork,
    height_provider: Arc<dyn TrustedBitcoinHeightProvider>,
    receipt_store: Arc<dyn CanonicalBitvmReceiptStore>,
}

impl CanonicalBitvmService {
    pub fn new(
        verifier: Arc<CanonicalStateTransitionVerifier>,
        expected_network: BitcoinNetwork,
        height_provider: Arc<dyn TrustedBitcoinHeightProvider>,
        receipt_store: Arc<dyn CanonicalBitvmReceiptStore>,
    ) -> Self {
        Self {
            verifier,
            expected_network,
            height_provider,
            receipt_store,
        }
    }

    pub async fn verify_and_persist(
        &self,
        transition: &NexusStateTransition,
        envelope: &GatewayGroth16Envelope,
    ) -> Result<CanonicalBitvmVerifiedReceipt, CanonicalBitvmServiceError> {
        if envelope.block_context.network != self.expected_network {
            return Err(CanonicalBitvmServiceError::NetworkMismatch {
                expected: self.expected_network,
                found: envelope.block_context.network,
            });
        }
        let current_height = self.height_provider.current_height().await?;
        if current_height == 0 {
            return Err(CanonicalBitvmServiceError::TrustedHeight(
                TrustedHeightError::Invalid,
            ));
        }
        let verified = self.verifier.verify(transition, envelope, current_height)?;
        let proof_digest = proof_digest(&envelope.proof);
        let receipt_id = receipt_id(verified.statement_hash, proof_digest);
        let record = CanonicalBitvmAuditRecord {
            receipt_id: receipt_id.clone(),
            schema_version: envelope.schema_version,
            curve: envelope.curve,
            circuit_id: envelope.circuit_id.clone(),
            verification_key_id: verified.verification_key_id,
            statement_hash: verified.statement_hash,
            proof_digest,
            previous_state_root: transition.prev_state_root,
            next_state_root: transition.next_state_root,
            witness_commitment: envelope.witness_commitment,
            network: envelope.block_context.network,
            anchor_block_height: envelope.block_context.block_height,
            anchor_block_hash: envelope.block_context.block_hash,
            max_valid_height: envelope.block_context.max_valid_height,
            backend_identity: BACKEND_IDENTITY,
            backend_version: BACKEND_VERSION,
            verified_at: Utc::now(),
        };
        let outcome = self.receipt_store.persist_immutable(record).await?;
        Ok(CanonicalBitvmVerifiedReceipt {
            receipt_id,
            statement_hash: verified.statement_hash,
            verification_key_id: verified.verification_key_id,
            schema_version: envelope.schema_version,
            curve: envelope.curve,
            circuit_id: envelope.circuit_id.clone(),
            created: outcome == AuditPersistOutcome::Created,
        })
    }
}

#[derive(Debug, Error)]
pub enum CanonicalBitvmServiceError {
    #[error("Bitcoin network mismatch")]
    NetworkMismatch {
        expected: BitcoinNetwork,
        found: BitcoinNetwork,
    },
    #[error(transparent)]
    TrustedHeight(#[from] TrustedHeightError),
    #[error(transparent)]
    Verification(#[from] CanonicalGroth16Error),
    #[error(transparent)]
    Audit(#[from] CanonicalBitvmAuditError),
}

pub struct PostgresCanonicalBitvmReceiptStore {
    storage: Arc<Storage>,
}

impl PostgresCanonicalBitvmReceiptStore {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

impl CanonicalBitvmReceiptStore for PostgresCanonicalBitvmReceiptStore {
    fn persist_immutable(
        &self,
        record: CanonicalBitvmAuditRecord,
    ) -> BoxFuture<'_, Result<AuditPersistOutcome, CanonicalBitvmAuditError>> {
        Box::pin(async move {
            let anchor_height = i64::try_from(record.anchor_block_height)
                .map_err(|_| CanonicalBitvmAuditError::IntegrityConflict)?;
            let max_valid_height = record
                .max_valid_height
                .map(i64::try_from)
                .transpose()
                .map_err(|_| CanonicalBitvmAuditError::IntegrityConflict)?;
            let result = sqlx::query(
                "INSERT INTO canonical_bitvm_receipts (
                    receipt_id, schema_version, curve, circuit_id, verification_key_id,
                    statement_hash, proof_digest, previous_state_root, next_state_root,
                    witness_commitment, bitcoin_network, anchor_block_height,
                    anchor_block_hash, max_valid_height, backend_identity,
                    backend_version, verified_at
                 ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
                 ON CONFLICT (receipt_id) DO NOTHING",
            )
            .bind(&record.receipt_id)
            .bind(i32::from(record.schema_version))
            .bind(record.curve.as_str())
            .bind(&record.circuit_id)
            .bind(hex::encode(record.verification_key_id.0))
            .bind(hex::encode(record.statement_hash))
            .bind(hex::encode(record.proof_digest))
            .bind(hex::encode(record.previous_state_root))
            .bind(hex::encode(record.next_state_root))
            .bind(hex::encode(record.witness_commitment))
            .bind(record.network.as_str())
            .bind(anchor_height)
            .bind(hex::encode(record.anchor_block_hash))
            .bind(max_valid_height)
            .bind(record.backend_identity)
            .bind(record.backend_version)
            .bind(record.verified_at)
            .execute(&self.storage.pg_pool)
            .await
            .map_err(|_| CanonicalBitvmAuditError::Unavailable)?;
            if result.rows_affected() == 1 {
                return Ok(AuditPersistOutcome::Created);
            }

            let existing = sqlx::query(
                "SELECT schema_version, curve, circuit_id, verification_key_id,
                        statement_hash, proof_digest, previous_state_root, next_state_root,
                        witness_commitment, bitcoin_network, anchor_block_height,
                        anchor_block_hash, max_valid_height, backend_identity, backend_version
                   FROM canonical_bitvm_receipts WHERE receipt_id = $1",
            )
            .bind(&record.receipt_id)
            .fetch_optional(&self.storage.pg_pool)
            .await
            .map_err(|_| CanonicalBitvmAuditError::Unavailable)?
            .ok_or(CanonicalBitvmAuditError::Unavailable)?;
            let matches = existing.get::<i32, _>("schema_version")
                == i32::from(record.schema_version)
                && existing.get::<String, _>("curve") == record.curve.as_str()
                && existing.get::<String, _>("circuit_id") == record.circuit_id
                && existing.get::<String, _>("verification_key_id")
                    == hex::encode(record.verification_key_id.0)
                && existing.get::<String, _>("statement_hash")
                    == hex::encode(record.statement_hash)
                && existing.get::<String, _>("proof_digest") == hex::encode(record.proof_digest)
                && existing.get::<String, _>("previous_state_root")
                    == hex::encode(record.previous_state_root)
                && existing.get::<String, _>("next_state_root")
                    == hex::encode(record.next_state_root)
                && existing.get::<String, _>("witness_commitment")
                    == hex::encode(record.witness_commitment)
                && existing.get::<String, _>("bitcoin_network") == record.network.as_str()
                && existing.get::<i64, _>("anchor_block_height") == anchor_height
                && existing.get::<String, _>("anchor_block_hash")
                    == hex::encode(record.anchor_block_hash)
                && existing.get::<Option<i64>, _>("max_valid_height") == max_valid_height
                && existing.get::<String, _>("backend_identity") == record.backend_identity
                && existing.get::<String, _>("backend_version") == record.backend_version;
            if matches {
                Ok(AuditPersistOutcome::Existing)
            } else {
                Err(CanonicalBitvmAuditError::IntegrityConflict)
            }
        })
    }
}

pub fn proof_digest(proof: &[u8]) -> [u8; 32] {
    domain_separated_hash(PROOF_DIGEST_DOMAIN, proof)
}

pub fn receipt_id(statement_hash: [u8; 32], proof_digest: [u8; 32]) -> String {
    let mut payload = [0u8; 64];
    payload[..32].copy_from_slice(&statement_hash);
    payload[32..].copy_from_slice(&proof_digest);
    hex::encode(domain_separated_hash(RECEIPT_ID_DOMAIN, &payload))
}

fn domain_separated_hash(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((payload.len() as u32).to_be_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}
