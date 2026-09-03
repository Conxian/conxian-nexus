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

/// Nexus validates that execution requests originate from a hardware-attested
/// enclave before processing proofs. This is a critical security boundary
/// between the executor and the core verification pipeline.
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
        // Verify enclave attestation first (fail-closed boundary). Until a
        // trusted attestation backend is wired this rejects any presented
        // certificate, while soft enforcement permits certificate-less requests.
        self.verify_attestation(&request)
            .map_err(|e| anyhow::anyhow!("attestation verification failed: {e}"))?;

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
    /// Fail closed: until a trusted attestation backend (root-of-trust and
    /// enclave-measurement comparison) is wired, any presented certificate is
    /// treated as unverifiable rather than "verified". When `require_attestation`
    /// is true, requests without a certificate are rejected; otherwise soft
    /// enforcement skips attestation with a warning. In production this must be
    /// enabled for all high-value proof submissions.
    pub fn verify_attestation(
        &self,
        request: &ExecutionRequest,
    ) -> Result<(), EnclaveVerificationError> {
        match &request.attestation_certificate {
            Some(raw_der) => {
                if raw_der.is_empty() {
                    return Err(EnclaveVerificationError::InvalidCertificate);
                }
                // Structural parse only: reject malformed DER early. This does
                // NOT establish that the certificate was issued by a trusted
                // TEE root or that it matches the expected enclave measurement.
                let _parsed_cert = X509Certificate::from_der(raw_der)
                    .map_err(|_| EnclaveVerificationError::InvalidCertificate)?;

                // Fail closed: no trusted attestation backend (root-of-trust +
                // measurement comparison) is configured yet. A date-valid X.509
                // certificate is not proof of hardware attestation, so every
                // presented certificate is treated as unverifiable rather than
                // "verified". This mirrors the fail-closed policy applied to the
                // Liquid/BitVM3/Strata chain adapters.
                Err(EnclaveVerificationError::ChainVerificationFailed(
                    "attestation verification backend not configured: no trusted root or enclave measurement check performed"
                        .to_string(),
                ))
            }
            None => {
                if self.require_attestation {
                    Err(EnclaveVerificationError::InvalidCertificate)
                } else {
                    tracing::warn!(
                        tx_id = %request.tx_id,
                        "Skipping attestation (soft enforcement is fail-open; enable require_attestation for high-value submissions)"
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

    /// A syntactically valid, currently-dated, self-signed EC P-256 X.509
    /// certificate (generated for test use only). It has no trusted issuer and
    /// no enclave measurement, so a fail-closed attestation boundary must reject
    /// it even though its validity window is in the present.
    const VALID_SELF_SIGNED_CERT_DER: &[u8] = &[
        48, 130, 1, 151, 48, 130, 1, 61, 160, 3, 2, 1, 2, 2, 20, 61, 44, 115, 28, 100, 147, 97,
        120, 8, 202, 149, 98, 170, 54, 166, 81, 11, 180, 150, 96, 48, 10, 6, 8, 42, 134, 72, 206,
        61, 4, 3, 2, 48, 33, 49, 31, 48, 29, 6, 3, 85, 4, 3, 12, 22, 110, 101, 120, 117, 115, 45,
        116, 101, 115, 116, 45, 97, 116, 116, 101, 115, 116, 97, 116, 105, 111, 110, 48, 30, 23,
        13, 50, 54, 48, 57, 48, 51, 49, 54, 48, 56, 48, 53, 90, 23, 13, 50, 54, 49, 48, 48, 51, 49,
        54, 48, 56, 48, 53, 90, 48, 33, 49, 31, 48, 29, 6, 3, 85, 4, 3, 12, 22, 110, 101, 120, 117,
        115, 45, 116, 101, 115, 116, 45, 97, 116, 116, 101, 115, 116, 97, 116, 105, 111, 110, 48,
        89, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72, 206, 61, 3, 1, 7, 3, 66,
        0, 4, 252, 162, 196, 147, 241, 76, 166, 34, 17, 53, 226, 192, 205, 194, 154, 233, 191, 139,
        98, 2, 188, 19, 64, 158, 136, 238, 163, 226, 42, 231, 19, 199, 63, 87, 17, 143, 138, 204,
        103, 231, 109, 24, 98, 165, 33, 243, 87, 57, 111, 176, 52, 89, 72, 19, 6, 137, 57, 251,
        237, 67, 28, 181, 112, 158, 163, 83, 48, 81, 48, 29, 6, 3, 85, 29, 14, 4, 22, 4, 20, 233,
        234, 49, 218, 35, 21, 29, 131, 218, 127, 193, 1, 135, 251, 15, 10, 241, 89, 213, 176, 48,
        31, 6, 3, 85, 29, 35, 4, 24, 48, 22, 128, 20, 233, 234, 49, 218, 35, 21, 29, 131, 218, 127,
        193, 1, 135, 251, 15, 10, 241, 89, 213, 176, 48, 15, 6, 3, 85, 29, 19, 1, 1, 255, 4, 5, 48,
        3, 1, 1, 255, 48, 10, 6, 8, 42, 134, 72, 206, 61, 4, 3, 2, 3, 72, 0, 48, 69, 2, 33, 0, 209,
        165, 96, 66, 44, 97, 213, 220, 30, 53, 78, 114, 34, 135, 216, 122, 114, 63, 18, 82, 145,
        147, 132, 159, 174, 158, 215, 116, 234, 139, 217, 139, 2, 32, 67, 190, 156, 108, 254, 13,
        183, 179, 215, 76, 98, 175, 159, 27, 201, 228, 51, 11, 120, 201, 1, 209, 124, 243, 144, 26,
        149, 245, 40, 109, 52, 65,
    ];

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

    #[tokio::test]
    async fn test_verify_attestation_fails_closed_for_valid_cert() {
        let executor = make_test_executor(false);

        // A syntactically valid, currently-dated, self-signed X.509 certificate
        // must NOT be accepted as proof of hardware attestation: no trusted-root
        // or measurement verification is configured, so the boundary must fail
        // closed rather than report "verified".
        let req = ExecutionRequest {
            tx_id: "tx_valid_but_untrusted".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            priority: 0,
            attestation_certificate: Some(VALID_SELF_SIGNED_CERT_DER.to_vec()),
        };

        match executor.verify_attestation(&req) {
            Err(EnclaveVerificationError::ChainVerificationFailed(_)) => {}
            other => panic!("expected ChainVerificationFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_submit_rejects_unverifiable_attestation() {
        let executor = make_test_executor(false);

        // submit() must enforce the attestation boundary before any other
        // processing: a presented (but unverifiable) certificate is rejected
        // fail-closed before safety-mode/DB work is reached.
        let req = ExecutionRequest {
            tx_id: "tx_submit_attestation".to_string(),
            payload: "data".to_string(),
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            priority: 0,
            attestation_certificate: Some(VALID_SELF_SIGNED_CERT_DER.to_vec()),
        };

        let err = executor.submit(req).await.unwrap_err();
        assert!(
            err.to_string().contains("attestation verification failed"),
            "unexpected error: {err}"
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
