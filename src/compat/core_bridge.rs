//! Nexus-owned compatibility for the small legacy Core signing surface still in use.
//!
//! This module intentionally contains only deterministic secp256k1 signing,
//! HASH160 public-key derivation, and signed Clarity-call construction. It is
//! not a BitVM verifier and must not grow protocol, persistence, or networking
//! responsibilities.
//!
//! # Core v0.3.0 canonical types
//!
//! Use `lib_conxian_core::control_model` for canonical chain identity, trust
//! tier, bridge system, and verification types. The `core_types` sub-module
//! re-exports the most commonly needed items for Nexus observation boundaries.

/// Re-exports of canonical Core v0.3.0 types for Nexus observation and
/// verification boundaries. These are the single source of truth for chain
/// identity across the Conxian ecosystem.
pub mod core_types {
    pub use lib_conxian_core::control_model::{
        chain_family_for, BridgeSystem, Chain, ChainFamily, FinalityClass, TrustTier,
        VerificationClass, VerificationStatus,
    };
    pub use lib_conxian_core::signing::{SignerCapabilities, SigningAlgorithm, SigningTarget};
    pub use lib_conxian_core::verifier::{
        ChainId, ProofVerificationRequest, ProofVerificationResult, ProtocolVerifier,
        ProtocolVerifierBackend, ProtocolVerifierError, TransactionFinalityStatus,
        VerifiedBlockReference, VerifierCapabilities, VerifierCapability,
    };
}

use anyhow::Context;
use k256::ecdsa::{signature::Signer, Signature, SigningKey};
use ripemd::Ripemd160;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ENV_CONXIAN_PRIVATE_KEY_HEX: &str = "CONXIAN_PRIVATE_KEY_HEX";
pub const ENV_NEXUS_PRIVATE_KEY: &str = "NEXUS_PRIVATE_KEY";

#[derive(Clone)]
pub struct Wallet {
    signing_key: SigningKey,
}

impl Wallet {
    pub fn new() -> anyhow::Result<Self> {
        Self::from_env_with(|name| std::env::var(name))
    }

    fn from_env_with<F>(read_env: F) -> anyhow::Result<Self>
    where
        F: Fn(&str) -> Result<String, std::env::VarError>,
    {
        if let Ok(hex_key) = read_env(ENV_CONXIAN_PRIVATE_KEY_HEX) {
            return Self::from_private_key_hex(&hex_key);
        }

        if let Ok(hex_key) = read_env(ENV_NEXUS_PRIVATE_KEY) {
            return Self::from_private_key_hex(&hex_key);
        }

        Err(anyhow::anyhow!(
            "missing private key env var: set {ENV_CONXIAN_PRIVATE_KEY_HEX} (or legacy {ENV_NEXUS_PRIVATE_KEY})"
        ))
    }

    pub fn from_private_key_hex(hex_key: &str) -> anyhow::Result<Self> {
        let bytes =
            hex::decode(hex_key.trim()).with_context(|| "invalid hex in private key env var")?;
        Self::from_private_key_bytes(&bytes)
    }

    pub fn from_private_key_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let signing_key = SigningKey::from_slice(bytes).with_context(|| "invalid private key")?;
        Ok(Self { signing_key })
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.signing_key
            .verifying_key()
            .to_sec1_point(true)
            .as_bytes()
            .to_vec()
    }

    pub fn public_key(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    pub fn stacks_address_hash(&self) -> String {
        let public_key = self.public_key_bytes();
        let sha256 = Sha256::digest(&public_key);
        hex::encode(Ripemd160::digest(sha256))
    }

    pub fn sign(&self, message: &str) -> String {
        let digest = Sha256::digest(message.as_bytes());
        let signature: Signature = self.signing_key.sign(&digest);
        hex::encode(signature.to_bytes())
    }
}

pub fn sign_transaction(tx_id: &str) -> anyhow::Result<String> {
    Ok(Wallet::new()?.sign(tx_id))
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClarityCall {
    pub contract_address: String,
    pub contract_name: String,
    pub function_name: String,
    pub arguments: Vec<String>,
    pub sender_address: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedContractCall {
    pub payload: ClarityCall,
    pub signature: String,
    pub public_key: String,
}

pub struct ContractBridge;

impl ContractBridge {
    pub fn create_signed_call(
        wallet: &Wallet,
        contract: &str,
        function: &str,
        args: Vec<String>,
    ) -> anyhow::Result<SignedContractCall> {
        let (contract_address, contract_name) = parse_contract_principal(contract)?;
        let payload = ClarityCall {
            contract_address,
            contract_name,
            function_name: function.to_string(),
            arguments: args,
            sender_address: wallet.stacks_address_hash(),
        };
        let serialized = serde_json::to_string(&payload)
            .map_err(|error| anyhow::anyhow!("serialization failed: {error}"))?;

        Ok(SignedContractCall {
            signature: wallet.sign(&serialized),
            public_key: wallet.public_key(),
            payload,
        })
    }
}

fn parse_contract_principal(contract: &str) -> anyhow::Result<(String, String)> {
    let contract = contract.trim();
    if contract.is_empty() {
        return Err(anyhow::anyhow!("contract principal is empty"));
    }

    let (address, name) = contract
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("invalid contract principal"))?;
    let address = address.trim();
    let name = name.trim();
    if address.is_empty() || name.is_empty() {
        return Err(anyhow::anyhow!("invalid contract principal"));
    }

    Ok((address.to_string(), name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::{signature::Verifier, VerifyingKey};

    fn private_key_one() -> [u8; 32] {
        let mut key = [0_u8; 32];
        key[31] = 1;
        key
    }

    fn private_key_two_hex() -> String {
        let mut key = [0_u8; 32];
        key[31] = 2;
        hex::encode(key)
    }

    #[test]
    fn private_key_one_matches_compressed_public_key_and_hash160() {
        let wallet = Wallet::from_private_key_bytes(&private_key_one()).expect("valid key");

        assert_eq!(
            wallet.public_key(),
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
        assert_eq!(
            wallet.stacks_address_hash(),
            "751e76e8199196d454941c45d1b3a323f1433bd6"
        );
    }

    #[test]
    fn signatures_are_deterministic_and_verify() {
        let wallet = Wallet::from_private_key_bytes(&private_key_one()).expect("valid key");
        let payload = "nexus-compatibility-signature-vector";
        let first = wallet.sign(payload);
        let second = wallet.sign(payload);
        assert_eq!(first, second);
        assert_eq!(
            first,
            "32c933d4ba8833a88da4fc2fcceaf0258c5785ace56f171ce9cc6b370facd40a654708b35a9712382523f54e2afb03afda2d8ed5796b3df855d08f7d25f991ec"
        );

        let signature = Signature::from_slice(&hex::decode(first).expect("signature hex"))
            .expect("valid signature");
        let verifying_key =
            VerifyingKey::from_sec1_bytes(&wallet.public_key_bytes()).expect("valid public key");
        let digest = Sha256::digest(payload.as_bytes());
        verifying_key
            .verify(&digest, &signature)
            .expect("signature verifies");
    }

    #[test]
    fn contract_call_signs_canonical_serialized_payload() {
        let wallet = Wallet::from_private_key_bytes(&private_key_one()).expect("valid key");
        let signed = ContractBridge::create_signed_call(
            &wallet,
            " ST000000000000000000002AMW42H.oracle ",
            "update-fx-rates",
            vec!["{\"USD\":1}".to_string()],
        )
        .expect("valid call");
        let serialized = serde_json::to_string(&signed.payload).expect("serializable payload");

        assert_eq!(
            signed.payload.contract_address,
            "ST000000000000000000002AMW42H"
        );
        assert_eq!(signed.payload.contract_name, "oracle");
        assert_eq!(signed.signature, wallet.sign(&serialized));
        assert_eq!(signed.public_key, wallet.public_key());
    }

    #[test]
    fn contract_principal_parsing_fails_closed() {
        let wallet = Wallet::from_private_key_bytes(&private_key_one()).expect("valid key");
        for invalid in ["", "address-only", ".name", "address."] {
            assert!(
                ContractBridge::create_signed_call(&wallet, invalid, "function", Vec::new())
                    .is_err()
            );
        }
    }

    #[test]
    fn environment_lookup_prefers_canonical_key_without_mutating_process_env() {
        let canonical = hex::encode(private_key_one());
        let legacy = private_key_two_hex();
        let wallet = Wallet::from_env_with(|name| match name {
            ENV_CONXIAN_PRIVATE_KEY_HEX => Ok(canonical.clone()),
            ENV_NEXUS_PRIVATE_KEY => Ok(legacy.clone()),
            _ => Err(std::env::VarError::NotPresent),
        })
        .expect("canonical key selected");

        assert_eq!(
            wallet.public_key(),
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
    }

    #[test]
    fn environment_lookup_uses_legacy_key_without_mutating_process_env() {
        let legacy = private_key_two_hex();
        let wallet = Wallet::from_env_with(|name| {
            if name == ENV_NEXUS_PRIVATE_KEY {
                Ok(legacy.clone())
            } else {
                Err(std::env::VarError::NotPresent)
            }
        })
        .expect("legacy key selected");
        assert_ne!(
            wallet.public_key(),
            Wallet::from_private_key_bytes(&private_key_one())
                .expect("valid key")
                .public_key()
        );
    }

    #[test]
    fn missing_environment_keys_fail_closed_without_mutating_process_env() {
        let error = Wallet::from_env_with(|_| Err(std::env::VarError::NotPresent))
            .err()
            .expect("missing key rejected");
        assert_eq!(
            error.to_string(),
            "missing private key env var: set CONXIAN_PRIVATE_KEY_HEX (or legacy NEXUS_PRIVATE_KEY)"
        );
    }
}
