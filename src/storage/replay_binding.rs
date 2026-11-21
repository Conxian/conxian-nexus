//! SDK-compatible replay binding keys for the Nexus consume-once store.
//!
//! The enclave SDK binds a replay reservation to provider, proof subject/mechanism,
//! nonce, operation, purpose, policy, key identity and evidence digests, and keys
//! the store by `"{domain}:{hex(binding_digest)}"` (`ReplayReservation::encoded_key`).
//! Nexus previously accepted an opaque caller-chosen `key`, so two distinct
//! operations could collide on the same string and nothing bound the record to the
//! provider/operation/policy evidence (#439 F2).
//!
//! This module reproduces the SDK's canonical encoding and key format exactly
//! (`conxius-enclave-sdk` `src/enclave/replay_guard.rs`) so a Nexus record and an
//! SDK reservation describe the same binding, without pulling the full SDK (and
//! its synchronous `ReplayStore` trait) into this crate. It also observes the
//! anti-rollback clock as part of the consume path (#439 F4), which was previously
//! opt-in.

use sha2::{Digest, Sha256};

use super::idempotency::{ConsumeOutcome, IdempotencyError, IdempotencyStore};

/// Domain separator, byte-identical to the SDK's `REPLAY_BINDING_DOMAIN`.
pub const REPLAY_BINDING_DOMAIN: &str = "CONXIAN-REPLAY-BINDING/v1";
/// Binding version, byte-identical to the SDK's `REPLAY_BINDING_VERSION` (`u16`).
pub const REPLAY_BINDING_VERSION: u16 = 1;

const MAX_COMPONENT_BYTES: usize = 4096;
const MAX_NONCE_BYTES: usize = 4096;
const MAX_KEY_IDENTITY_BYTES: usize = 4096;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayBindingError {
    #[error("replay binding identifier is invalid")]
    InvalidIdentifier,
    #[error("replay binding input is empty")]
    EmptyInput,
    #[error("replay binding component is missing: {0}")]
    MissingComponent(&'static str),
    #[error("replay binding input exceeds its bound")]
    OversizedInput,
}

fn append_len_prefixed(output: &mut Vec<u8>, value: &[u8]) -> Result<(), ReplayBindingError> {
    let length = u32::try_from(value.len()).map_err(|_| ReplayBindingError::OversizedInput)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
    Ok(())
}

/// Hash one component with the SDK's domain + version + length-prefixed label.
fn hash_binding_component(label: &str, value: &[u8]) -> Result<[u8; 32], ReplayBindingError> {
    let mut canonical = Vec::new();
    append_len_prefixed(&mut canonical, REPLAY_BINDING_DOMAIN.as_bytes())?;
    canonical.extend_from_slice(&REPLAY_BINDING_VERSION.to_be_bytes());
    append_len_prefixed(&mut canonical, label.as_bytes())?;
    append_len_prefixed(&mut canonical, value)?;
    Ok(Sha256::digest(canonical).into())
}

fn validate_identifier(value: &str) -> Result<(), ReplayBindingError> {
    if value.is_empty() {
        return Err(ReplayBindingError::InvalidIdentifier);
    }
    if value.len() > MAX_COMPONENT_BYTES {
        return Err(ReplayBindingError::OversizedInput);
    }
    Ok(())
}

/// A complete provider-neutral replay binding, mirroring the SDK `ReplayBinding`.
///
/// Only fixed-size digests of nonce, key identity and evidence cross the storage
/// boundary; the textual identifiers are length-prefixed into the canonical form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayBinding {
    pub provider: String,
    pub proof_subject: String,
    pub proof_mechanism: String,
    pub nonce_digest: [u8; 32],
    pub operation_digest: [u8; 32],
    pub purpose: String,
    pub policy_digest: [u8; 32],
    pub key_identity_digest: [u8; 32],
    pub evidence_digest: [u8; 32],
    pub proof_id: Option<String>,
    pub audience: Option<String>,
}

impl ReplayBinding {
    /// Canonical, domain-separated byte encoding — identical to the SDK.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ReplayBindingError> {
        for value in [
            &self.provider,
            &self.proof_subject,
            &self.proof_mechanism,
            &self.purpose,
        ] {
            validate_identifier(value)?;
        }
        for value in [&self.proof_id, &self.audience].into_iter().flatten() {
            validate_identifier(value)?;
        }

        let mut output = Vec::new();
        append_len_prefixed(&mut output, REPLAY_BINDING_DOMAIN.as_bytes())?;
        output.extend_from_slice(&REPLAY_BINDING_VERSION.to_be_bytes());
        append_len_prefixed(&mut output, self.provider.as_bytes())?;
        append_len_prefixed(&mut output, self.proof_subject.as_bytes())?;
        append_len_prefixed(&mut output, self.proof_mechanism.as_bytes())?;
        output.extend_from_slice(&self.nonce_digest);
        output.extend_from_slice(&self.operation_digest);
        append_len_prefixed(&mut output, self.purpose.as_bytes())?;
        output.extend_from_slice(&self.policy_digest);
        output.extend_from_slice(&self.key_identity_digest);
        output.extend_from_slice(&self.evidence_digest);
        match &self.proof_id {
            Some(proof_id) => {
                output.push(1);
                append_len_prefixed(&mut output, proof_id.as_bytes())?;
            }
            None => output.push(0),
        }
        match &self.audience {
            Some(audience) => {
                output.push(1);
                append_len_prefixed(&mut output, audience.as_bytes())?;
            }
            None => output.push(0),
        }
        Ok(output)
    }

    /// 32-byte binding digest — identical to the SDK `ReplayBinding::digest`.
    pub fn digest(&self) -> Result<[u8; 32], ReplayBindingError> {
        Ok(Sha256::digest(self.canonical_bytes()?).into())
    }

    /// Store key, identical to the SDK `ReplayReservation::encoded_key`.
    pub fn encoded_key(&self) -> Result<String, ReplayBindingError> {
        Ok(format!(
            "{}:{}",
            REPLAY_BINDING_DOMAIN,
            hex::encode(self.digest()?)
        ))
    }
}

/// Builder mirroring the SDK's `ReplayBinding::builder()`; raw nonce and key
/// identity are hashed here so the stored digests match the SDK byte-for-byte.
#[derive(Default)]
pub struct ReplayBindingBuilder {
    provider: Option<String>,
    proof_subject: Option<String>,
    proof_mechanism: Option<String>,
    nonce: Option<Vec<u8>>,
    operation_digest: Option<[u8; 32]>,
    purpose: Option<String>,
    policy_digest: Option<[u8; 32]>,
    key_identity: Option<Vec<u8>>,
    evidence_digest: Option<[u8; 32]>,
    proof_id: Option<String>,
    audience: Option<String>,
}

impl ReplayBindingBuilder {
    pub fn provider(mut self, value: impl Into<String>) -> Self {
        self.provider = Some(value.into());
        self
    }
    pub fn proof_subject(mut self, value: impl Into<String>) -> Self {
        self.proof_subject = Some(value.into());
        self
    }
    pub fn proof_mechanism(mut self, value: impl Into<String>) -> Self {
        self.proof_mechanism = Some(value.into());
        self
    }
    pub fn nonce(mut self, value: &[u8]) -> Self {
        self.nonce = Some(value.to_vec());
        self
    }
    pub fn operation_digest(mut self, value: [u8; 32]) -> Self {
        self.operation_digest = Some(value);
        self
    }
    pub fn purpose(mut self, value: impl Into<String>) -> Self {
        self.purpose = Some(value.into());
        self
    }
    pub fn policy_digest(mut self, value: [u8; 32]) -> Self {
        self.policy_digest = Some(value);
        self
    }
    pub fn key_identity(mut self, value: &[u8]) -> Self {
        self.key_identity = Some(value.to_vec());
        self
    }
    pub fn evidence_digest(mut self, value: [u8; 32]) -> Self {
        self.evidence_digest = Some(value);
        self
    }
    pub fn proof_id(mut self, value: impl Into<String>) -> Self {
        self.proof_id = Some(value.into());
        self
    }
    pub fn audience(mut self, value: impl Into<String>) -> Self {
        self.audience = Some(value.into());
        self
    }

    pub fn build(self) -> Result<ReplayBinding, ReplayBindingError> {
        let provider = self
            .provider
            .ok_or(ReplayBindingError::MissingComponent("provider"))?;
        let proof_subject = self
            .proof_subject
            .ok_or(ReplayBindingError::MissingComponent("proof subject"))?;
        let proof_mechanism = self
            .proof_mechanism
            .ok_or(ReplayBindingError::MissingComponent("proof mechanism"))?;
        let nonce = self
            .nonce
            .ok_or(ReplayBindingError::MissingComponent("nonce"))?;
        let operation_digest = self
            .operation_digest
            .ok_or(ReplayBindingError::MissingComponent("operation digest"))?;
        let purpose = self
            .purpose
            .ok_or(ReplayBindingError::MissingComponent("purpose"))?;
        let policy_digest = self
            .policy_digest
            .ok_or(ReplayBindingError::MissingComponent("policy digest"))?;
        let key_identity = self
            .key_identity
            .ok_or(ReplayBindingError::MissingComponent("key identity"))?;
        let evidence_digest = self
            .evidence_digest
            .ok_or(ReplayBindingError::MissingComponent("evidence digest"))?;

        for value in [&provider, &proof_subject, &proof_mechanism, &purpose] {
            validate_identifier(value)?;
        }
        for value in [&self.proof_id, &self.audience].into_iter().flatten() {
            validate_identifier(value)?;
        }
        if nonce.is_empty() || key_identity.is_empty() {
            return Err(ReplayBindingError::EmptyInput);
        }
        if nonce.len() > MAX_NONCE_BYTES || key_identity.len() > MAX_KEY_IDENTITY_BYTES {
            return Err(ReplayBindingError::OversizedInput);
        }

        Ok(ReplayBinding {
            provider,
            proof_subject,
            proof_mechanism,
            nonce_digest: hash_binding_component("nonce", &nonce)?,
            operation_digest,
            purpose,
            policy_digest,
            key_identity_digest: hash_binding_component("key-identity", &key_identity)?,
            evidence_digest,
            proof_id: self.proof_id,
            audience: self.audience,
        })
    }
}

impl ReplayBinding {
    /// Start building a complete replay binding, mirroring the SDK.
    pub fn builder() -> ReplayBindingBuilder {
        ReplayBindingBuilder::default()
    }
}

impl IdempotencyStore {
    /// Consume a binding-derived replay key, observing the anti-rollback clock
    /// first so a clock rollback fails closed before any effect is authorised.
    ///
    /// `now_secs` is bound to the binding domain so independent domains do not
    /// share a high-water mark. Returns [`ConsumeOutcome`] keyed by the canonical
    /// binding, so two distinct bindings can never collide.
    pub async fn consume_binding(
        &self,
        binding: &ReplayBinding,
        operation: &str,
        now_secs: u64,
        retain_until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<ConsumeOutcome, IdempotencyError> {
        let key = binding
            .encoded_key()
            .map_err(|_| IdempotencyError::InvalidKey)?;
        let now_secs = i64::try_from(now_secs).map_err(|_| IdempotencyError::InvalidClock)?;

        self.observe_time(REPLAY_BINDING_DOMAIN, now_secs).await?;

        match retain_until {
            Some(retain_until) => self.consume_once_until(&key, operation, retain_until).await,
            None => self.consume_once(&key, operation).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ReplayBinding {
        ReplayBinding::builder()
            .provider("aws-nitro")
            .proof_subject("release-signer")
            .proof_mechanism("nitro-attestation")
            .nonce(&[1u8; 32])
            .operation_digest([2u8; 32])
            .purpose("release")
            .policy_digest([3u8; 32])
            .key_identity(&[4u8; 32])
            .evidence_digest([5u8; 32])
            .proof_id("proof-1")
            .build()
            .unwrap()
    }

    fn lazy_store() -> IdempotencyStore {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/nexus")
            .expect("connect_lazy should not require a live DB");
        IdempotencyStore::new(pool)
    }

    #[test]
    fn encoded_key_uses_sdk_domain_and_hex() {
        let key = binding().encoded_key().unwrap();
        assert!(key.starts_with("CONXIAN-REPLAY-BINDING/v1:"));
        let hex = key.split_once(':').unwrap().1;
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn canonical_encoding_is_stable_and_order_sensitive() {
        assert_eq!(
            binding().canonical_bytes().unwrap(),
            binding().canonical_bytes().unwrap()
        );
        let mut other = binding();
        other.operation_digest = [9u8; 32];
        assert_ne!(binding().digest().unwrap(), other.digest().unwrap());
    }

    #[test]
    fn proof_id_presence_changes_digest() {
        let mut without = binding();
        without.proof_id = None;
        assert_ne!(binding().digest().unwrap(), without.digest().unwrap());
    }

    #[test]
    fn distinct_bindings_never_collide() {
        let mut other = binding();
        other.policy_digest = [7u8; 32];
        assert_ne!(
            binding().encoded_key().unwrap(),
            other.encoded_key().unwrap()
        );
    }

    #[test]
    fn empty_provider_rejected() {
        assert_eq!(
            ReplayBinding::builder()
                .provider("")
                .proof_subject("s")
                .proof_mechanism("m")
                .nonce(&[1u8; 32])
                .operation_digest([2u8; 32])
                .purpose("p")
                .policy_digest([3u8; 32])
                .key_identity(&[4u8; 32])
                .evidence_digest([5u8; 32])
                .build(),
            Err(ReplayBindingError::InvalidIdentifier)
        );
    }

    #[test]
    fn empty_nonce_rejected() {
        assert_eq!(
            ReplayBinding::builder()
                .provider("p")
                .proof_subject("s")
                .proof_mechanism("m")
                .nonce(&[])
                .operation_digest([2u8; 32])
                .purpose("p")
                .policy_digest([3u8; 32])
                .key_identity(&[4u8; 32])
                .evidence_digest([5u8; 32])
                .build(),
            Err(ReplayBindingError::EmptyInput)
        );
    }

    #[test]
    fn oversized_component_rejected() {
        assert_eq!(
            ReplayBinding::builder()
                .provider("p")
                .proof_subject("x".repeat(MAX_COMPONENT_BYTES + 1))
                .proof_mechanism("m")
                .nonce(&[1u8; 32])
                .operation_digest([2u8; 32])
                .purpose("p")
                .policy_digest([3u8; 32])
                .key_identity(&[4u8; 32])
                .evidence_digest([5u8; 32])
                .build(),
            Err(ReplayBindingError::OversizedInput)
        );
    }

    #[tokio::test]
    async fn consume_binding_rejects_overflowing_clock_before_db() {
        let store = lazy_store();
        let result = store
            .consume_binding(&binding(), "op", u64::MAX, None)
            .await;
        assert!(matches!(result, Err(IdempotencyError::InvalidClock)));
    }
}
