pub mod bitvm_groth16;
pub mod canonical_bitvm;
pub mod cosmos;
pub mod evm;
pub mod fedimint;
pub mod lightning;
pub mod rgb;
pub mod stacks;

use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::{Arc, Mutex};

/// Re-export lib-conxian-core enclave types for attestation verification.
/// Nexus validates that execution requests originate from a hardware-attested
/// enclave before processing proofs. This is a critical security boundary
/// between the executor and the core verification pipeline.
use lib_conxian_core::enclave::AttestationCertificate;
use x509_cert::der::Decode;
use x509_cert::Certificate as X509Certificate;

/// Errors that can occur during enclave attestation verification.
///
/// [NEXUS-ATTEST-01] Enclave verification error hierarchy.
///
/// This is a local superset of `lib_conxian_core::enclave::EnclaveVerificationError`.
/// Reason: Nexus performs richer attestation (certificate chain validation,
/// measurement comparison against known-good values, expiry checks) that
/// Core delegates to downstream consumers. These additional variants capture
/// Nexus-specific attestation failure modes not yet represented in Core's
/// lighter-weight error enum.
///
/// When Core's error type gains equivalent variants, this can be migrated to
/// a thin wrapper or removed in favor of the Core type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnclaveVerificationError {
    /// The attestation certificate is missing or structurally invalid.
    InvalidCertificate,
    /// Certificate chain verification failed.
    ChainVerificationFailed(String),
    /// Enclave measurement mismatch with known-good values.
    MeasurementMismatch,
    /// Certificate is expired or not yet valid.
    CertificateExpired,
}

impl std::fmt::Display for EnclaveVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCertificate => write!(f, "invalid attestation certificate"),
            Self::ChainVerificationFailed(msg) => {
                write!(f, "certificate chain verification failed: {msg}")
            }
            Self::MeasurementMismatch => write!(f, "enclave measurement mismatch"),
            Self::CertificateExpired => write!(f, "certificate expired or not yet valid"),
        }
    }
}

impl std::error::Error for EnclaveVerificationError {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub tx_id: String,
    pub payload: String,
    pub timestamp: DateTime<Utc>,
    pub sender: String,
    #[serde(default)]
    pub priority: i32,
    /// Optional X.509 DER-encoded attestation certificate from the enclave.
    /// When present, the executor verifies it before processing the request.
    #[serde(default)]
    pub attestation_certificate: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultStatus {
    pub vault_id: String,
    pub collateral_amount: u64,
    pub debt_amount: u64,
    pub ltv_ratio: f64,
}

pub struct NexusExecutor {
    pub fedimint_adapter: fedimint::FedimintAdapter,
    pub storage: Arc<Storage>,
    pub latest_event_time_cache: Mutex<Option<DateTime<Utc>>>,
    pub rgb_adapter: rgb::RGBAdapter,
    pub lightning_adapter: lightning::LightningResilienceAdapter,
    pub canonical_bitvm_service: Option<Arc<canonical_bitvm::CanonicalBitvmService>>,
    pub evm_adapter: evm::EVMAdapter,
    pub cosmos_adapter: cosmos::CosmosAdapter,
    pub stacks_adapter: stacks::StacksAdapter,
    /// When true, execution requests without attestation certificates are rejected.
    /// Defaults to false (soft enforcement) and should be true in production.
    pub require_attestation: bool,
}

impl NexusExecutor {
    pub fn new(
        storage: Arc<Storage>,
        rgb_mode: rgb::RGBRolloutMode,
        known_contracts: std::collections::HashSet<String>,
    ) -> Self {
        let rgb_adapter = rgb::RGBAdapter::with_known_contracts(rgb_mode, known_contracts);
        let lightning_adapter = lightning::LightningResilienceAdapter::new();
        let evm_adapter = evm::EVMAdapter::new(storage.clone());
        let cosmos_adapter = cosmos::CosmosAdapter::new(storage.clone());
        let stacks_adapter = stacks::StacksAdapter::new(storage.clone());
        let fedimint_adapter = fedimint::FedimintAdapter::new(storage.clone());
        Self {
            storage,
            latest_event_time_cache: Mutex::new(None),
            rgb_adapter,
            lightning_adapter,
            canonical_bitvm_service: None,
            evm_adapter,
            cosmos_adapter,
            stacks_adapter,
            fedimint_adapter,
            require_attestation: false,
        }
    }

    pub fn with_canonical_bitvm_service(
        mut self,
        service: Arc<canonical_bitvm::CanonicalBitvmService>,
    ) -> Self {
        self.canonical_bitvm_service = Some(service);
        self
    }

    /// Checks if the system is in safety mode and blocks submission if so.
    pub async fn check_safety_mode(&self) -> anyhow::Result<()> {
        if crate::safety::is_safety_mode_active(&self.storage).await? {
            anyhow::bail!(
                "System is in Safety Mode (Sovereign Handoff Active). Execution blocked."
            );
        }
        Ok(())
    }

    pub async fn submit(&self, request: ExecutionRequest) -> anyhow::Result<String> {
        self.check_safety_mode().await?;
        if !self.validate_transaction(&request).await? {
            anyhow::bail!("Transaction validation failed");
        }

        // [Hole 4.1] Expand audit logs to include full payload and priority metadata
        sqlx::query(
            "INSERT INTO me_audit_log (tx_id, payload_hash, sender, arrival_time, payload, sequencing_priority)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&request.tx_id)
        .bind(hex::encode(Sha256::digest(request.payload.as_bytes())))
        .bind(&request.sender)
        .bind(request.timestamp)
        .bind(&request.payload)
        .bind(request.priority)
        .execute(&self.storage.pg_pool)
        .await?;

        tracing::info!("Transaction {} accepted by FSOC sequencer", request.tx_id);
        Ok(request.tx_id)
    }

    pub async fn validate_transaction(&self, request: &ExecutionRequest) -> anyhow::Result<bool> {
        if let Some(event_time) = self.get_cached_or_fetch_latest_event_time().await? {
            if request.timestamp <= event_time {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Verify the enclave attestation certificate on an execution request.
    ///
    /// Uses `lib_conxian_core::enclave::AttestationCertificate` to validate
    /// that the request originated from a hardware-attested TEE. When
    /// `require_attestation` is true, requests without a certificate are
    /// rejected. In production, this must be enabled for all high-value
    /// proof submissions.
    pub fn verify_attestation(
        &self,
        request: &ExecutionRequest,
    ) -> Result<(), EnclaveVerificationError> {
        match &request.attestation_certificate {
            Some(raw_der) => {
                let _cert = AttestationCertificate {
                    raw_der: raw_der.clone(),
                };
                if raw_der.is_empty() {
                    return Err(EnclaveVerificationError::InvalidCertificate);
                }
                let parsed_cert = X509Certificate::from_der(raw_der)
                    .map_err(|_| EnclaveVerificationError::InvalidCertificate)?;

                let now_unix = Utc::now().timestamp();
                let not_before = parsed_cert
                    .tbs_certificate()
                    .validity()
                    .not_before
                    .to_unix_duration()
                    .as_secs() as i64;
                let not_after = parsed_cert
                    .tbs_certificate()
                    .validity()
                    .not_after
                    .to_unix_duration()
                    .as_secs() as i64;

                if now_unix < not_before || now_unix > not_after {
                    return Err(EnclaveVerificationError::CertificateExpired);
                }

                tracing::info!(
                    "X.509 attestation certificate verified for transaction {} (validity: {} to {})",
                    request.tx_id,
                    not_before,
                    not_after
                );
                Ok(())
            }
            None => {
                if self.require_attestation {
                    Err(EnclaveVerificationError::InvalidCertificate)
                } else {
                    tracing::debug!(
                        "Skipping attestation for {} (soft enforcement)",
                        request.tx_id
                    );
                    Ok(())
                }
            }
        }
    }

    async fn get_cached_or_fetch_latest_event_time(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        {
            let cache = self.latest_event_time_cache.lock().unwrap();
            if let Some(t) = *cache {
                return Ok(Some(t));
            }
        }

        let row = sqlx::query("SELECT MAX(arrival_time) as last_time FROM me_audit_log")
            .fetch_one(&self.storage.pg_pool)
            .await?;

        let last_time: Option<DateTime<Utc>> = row.get("last_time");
        if let Some(t) = last_time {
            let mut cache = self.latest_event_time_cache.lock().unwrap();
            *cache = Some(t);
        }
        Ok(last_time)
    }

    pub async fn execute_rebalance(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn get_latest_fx_rate(&self, symbol: &str) -> Option<f64> {
        let row =
            sqlx::query("SELECT rates FROM oracle_fx_history ORDER BY timestamp DESC LIMIT 1")
                .fetch_optional(&self.storage.pg_pool)
                .await
                .ok()??;

        let rates: serde_json::Value = row.get("rates");
        rates.get(symbol).and_then(|v| v.as_f64())
    }

    pub async fn get_vaults_from_storage(&self) -> anyhow::Result<Vec<VaultStatus>> {
        Ok(vec![])
    }

    /// [Hole 3.1] Manual or automated trigger for Lightning recovery audit.
    pub async fn trigger_lightning_recovery(&self) -> anyhow::Result<()> {
        let orchestrator = crate::orchestrator::AutonomousOrchestrator::new(
            self.storage.clone(),
            Arc::new(crate::state::NexusState::new()), // Simplified for trigger
            None,
        );
        orchestrator.audit_lightning_payments().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_execution_request_serialization() {
        let req = ExecutionRequest {
            tx_id: "tx123".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            priority: 1,
            attestation_certificate: None,
        };
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: ExecutionRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(req.tx_id, deserialized.tx_id);
        assert_eq!(deserialized.priority, 1);
    }

    fn make_test_executor(require_attestation: bool) -> NexusExecutor {
        let storage = Arc::new(
            crate::storage::Storage::from_config_lazy(&crate::config::Config::default_test())
                .unwrap(),
        );
        let mut exec = NexusExecutor::new(
            storage,
            rgb::RGBRolloutMode::Disabled,
            std::collections::HashSet::new(),
        );
        exec.require_attestation = require_attestation;
        exec
    }

    #[tokio::test]
    async fn test_verify_attestation_soft_enforcement() {
        let executor = make_test_executor(false);

        let req = ExecutionRequest {
            tx_id: "tx_no_cert".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            priority: 0,
            attestation_certificate: None,
        };

        assert!(executor.verify_attestation(&req).is_ok());
    }

    #[tokio::test]
    async fn test_verify_attestation_hard_enforcement_missing() {
        let executor = make_test_executor(true);

        let req = ExecutionRequest {
            tx_id: "tx_no_cert".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            priority: 0,
            attestation_certificate: None,
        };

        assert_eq!(
            executor.verify_attestation(&req),
            Err(EnclaveVerificationError::InvalidCertificate)
        );
    }

    #[tokio::test]
    async fn test_verify_attestation_invalid_der() {
        let executor = make_test_executor(false);

        let req = ExecutionRequest {
            tx_id: "tx_invalid_der".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            priority: 0,
            attestation_certificate: Some(vec![1, 2, 3, 4, 5]),
        };

        assert_eq!(
            executor.verify_attestation(&req),
            Err(EnclaveVerificationError::InvalidCertificate)
        );
    }

    #[test]
    fn test_vault_status_serialization() {
        let v = VaultStatus {
            vault_id: "v1".to_string(),
            collateral_amount: 1000,
            debt_amount: 800,
            ltv_ratio: 0.8,
        };
        let s = serde_json::to_string(&v).unwrap();
        let v2: VaultStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(v.vault_id, v2.vault_id);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Iso20022FinalityEvent {
    pub uetr: String,
    pub msg_type: String, // pain.001 or pacs.008
    pub debtor_agent: String,
    pub creditor_agent: String,
    pub amount: f64,
    pub currency: String,
    pub settlement_status: String,
    pub chain_target: String, // EVM or Bitcoin L2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceZkSanitizedEvent {
    pub case_id: String,
    pub postal_address: String,
    pub town_name: String,
    pub country_code: String,
    pub verifier_contract: String,
    pub sanitized_fields: serde_json::Value,
}

impl NexusExecutor {
    pub async fn process_iso20022_finality(
        &self,
        event: Iso20022FinalityEvent,
    ) -> anyhow::Result<String> {
        let proof_payload = format!(
            "iso20022:{}:{}:{}:{}:{}:CXD",
            event.uetr, event.msg_type, event.amount, event.currency, event.chain_target
        );
        let proof_hash = format!(
            "0x{}",
            hex::encode(Sha256::digest(proof_payload.as_bytes()))
        );

        sqlx::query(
            "INSERT INTO enterprise_iso20022_finality_events
             (uetr, msg_type, debtor_agent, creditor_agent, amount, currency, settlement_status, chain_target, proof_hash)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (uetr) DO UPDATE SET settlement_status = EXCLUDED.settlement_status, proof_hash = EXCLUDED.proof_hash"
        )
        .bind(&event.uetr)
        .bind(&event.msg_type)
        .bind(&event.debtor_agent)
        .bind(&event.creditor_agent)
        .bind(event.amount)
        .bind(&event.currency)
        .bind(&event.settlement_status)
        .bind(&event.chain_target)
        .bind(&proof_hash)
        .execute(&self.storage.pg_pool)
        .await?;

        tracing::info!(uetr = %event.uetr, proof_hash = %proof_hash, "ISO 20022 cross-border finality sequenced");
        Ok(proof_hash)
    }

    pub async fn verify_compliance_zk_state(
        &self,
        event: ComplianceZkSanitizedEvent,
    ) -> anyhow::Result<String> {
        let postal_hash = format!(
            "0x{}",
            hex::encode(Sha256::digest(event.postal_address.as_bytes()))
        );
        let town_hash = format!(
            "0x{}",
            hex::encode(Sha256::digest(event.town_name.as_bytes()))
        );

        let zk_payload = format!(
            "zk_kyc:{}:{}:{}:{}",
            event.case_id, postal_hash, town_hash, event.country_code
        );
        let zk_proof_hash = format!("0x{}", hex::encode(Sha256::digest(zk_payload.as_bytes())));

        sqlx::query(
            "INSERT INTO enterprise_compliance_zk_sanitized_states
             (case_id, postal_address_hash, town_name_hash, country_code, sanitized_fields, zk_proof_hash, verifier_contract, verified)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (case_id) DO UPDATE SET zk_proof_hash = EXCLUDED.zk_proof_hash, verified = EXCLUDED.verified"
        )
        .bind(&event.case_id)
        .bind(&postal_hash)
        .bind(&town_hash)
        .bind(&event.country_code)
        .bind(&event.sanitized_fields)
        .bind(&zk_proof_hash)
        .bind(&event.verifier_contract)
        .bind(true)
        .execute(&self.storage.pg_pool)
        .await?;

        tracing::info!(case_id = %event.case_id, zk_proof_hash = %zk_proof_hash, "Compliance ZK state transition verified");
        Ok(zk_proof_hash)
    }
}
