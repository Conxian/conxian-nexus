use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FedimintConfig {
    pub federation_id: String,
    pub invite_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FedimintMintProof {
    pub proof: String,
    pub federation_id: String,
    pub amount_sats: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FedimintVerificationResult {
    pub valid: bool,
    pub proof_hash: String,
    pub nonce_hash: String,
    pub federation_id: String,
    pub amount_sats: u64,
    pub message: String,
}

pub struct FedimintAdapter {
    pub storage: Arc<Storage>,
}

impl FedimintAdapter {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Validates structural input, then fails closed without a cryptographic provider.
    /// A prefix/hash is not federation signature or double-spend evidence.
    pub async fn verify_mint_proof(&self, proof: &str) -> anyhow::Result<bool> {
        let res = self
            .verify_mint_proof_detailed(proof, "fed:default_federation", 1000)
            .await?;
        Ok(res.valid)
    }

    /// Detailed verification of a Fedimint e-cash mint proof with custom federation ID and amount.
    pub async fn verify_mint_proof_detailed(
        &self,
        proof: &str,
        federation_id: &str,
        amount_sats: u64,
    ) -> anyhow::Result<FedimintVerificationResult> {
        let trimmed_proof = proof.trim();

        // Structural validation: Prefix check for Fedimint e-cash notes/proofs
        if !trimmed_proof.starts_with("fed:") && !trimmed_proof.starts_with("fed1:") {
            anyhow::bail!("Invalid Fedimint e-cash proof prefix: expected 'fed:' or 'fed1:'");
        }

        if trimmed_proof.len() < 32 {
            anyhow::bail!("Invalid Fedimint e-cash proof length: payload too short");
        }

        if federation_id.trim().is_empty() || amount_sats == 0 {
            anyhow::bail!("Invalid Fedimint federation or amount context");
        }
        // No qualified federation keyset/signature verifier or atomic spentness
        // provider is wired into this adapter. Never record arbitrary input as
        // verified, and never treat a database failure as an unspent note.
        anyhow::bail!("Fedimint cryptographic verifier not configured")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn sample_valid_proof() -> &'static str {
        "fed:e-cash-blinded-mint-note-sample-payload-long-enough-for-validation"
    }

    #[tokio::test]
    async fn test_fedimint_adapter_invalid_prefix() {
        let config = Config::default_test();
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let adapter = FedimintAdapter::new(storage);

        let err = adapter.verify_mint_proof("invalid_prefix_proof").await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("prefix"));
    }

    #[tokio::test]
    async fn test_fedimint_adapter_invalid_length() {
        let config = Config::default_test();
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let adapter = FedimintAdapter::new(storage);

        let err = adapter.verify_mint_proof("fed:short").await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("length"));
    }

    #[tokio::test]
    async fn test_fedimint_adapter_valid_proof_structure() {
        let config = Config::default_test();
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let adapter = FedimintAdapter::new(storage);

        let err = adapter
            .verify_mint_proof_detailed(sample_valid_proof(), "fed:test_federation_1", 5000)
            .await
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("cryptographic verifier not configured"));
    }
}
