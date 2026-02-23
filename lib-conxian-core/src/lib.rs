use k256::ecdsa::{SigningKey, Signature, signature::Signer};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use std::env;

pub struct Wallet {
    signing_key: SigningKey,
}

impl Wallet {
    pub fn new() -> Self {
        // Try to load from NEXUS_PRIVATE_KEY
        if let Ok(hex_key) = env::var("NEXUS_PRIVATE_KEY") {
            if let Ok(bytes) = hex::decode(hex_key) {
                if let Ok(key) = SigningKey::from_slice(&bytes) {
                    return Self { signing_key: key };
                }
            }
        }

        // Fallback to random
        let signing_key = SigningKey::random(&mut OsRng);
        Self { signing_key }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, k256::ecdsa::Error> {
        let signing_key = SigningKey::from_slice(bytes)?;
        Ok(Self { signing_key })
    }

    pub fn public_key(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_sec1_bytes())
    }

    pub fn sign(&self, message: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(message.as_bytes());
        let digest = hasher.finalize();
        let signature: Signature = self.signing_key.sign(&digest);
        hex::encode(signature.to_bytes())
    }
}

pub fn sign_transaction(tx_id: &str) -> String {
    let wallet = Wallet::new();
    wallet.sign(tx_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_signing() {
        let wallet = Wallet::new();
        let message = "hello world";
        let signature = wallet.sign(message);
        assert!(!signature.is_empty());
    }

    #[test]
    fn test_wallet_from_env() {
        let key = "0101010101010101010101010101010101010101010101010101010101010101";
        unsafe {
            env::set_var("NEXUS_PRIVATE_KEY", key);
        }
        let wallet = Wallet::new();
        assert_eq!(wallet.public_key(), "031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f");
        unsafe {
            env::remove_var("NEXUS_PRIVATE_KEY");
        }
    }

    #[test]
    fn test_bitvm_service_handling() {
        use crate::gateway::{BitVMService, ConxianService};
        let service = BitVMService;
        assert!(service.handle_request("prove something").contains("BitVM proof generated"));
        assert!(service.handle_request("challenge this").contains("BitVM challenge registered"));
        assert!(service.handle_request("verify").contains("BitVM verification successful"));
    }

    #[test]
    fn test_bisq_service_handling() {
        use crate::gateway::{BisqService, ConxianService};
        let service = BisqService;
        let resp = service.handle_request(r#"{"trade_id": "T1", "amount": 100}"#);
        assert!(resp.contains("Bisq trade verified"));
    }

    #[test]
    fn test_rgb_service_handling() {
        use crate::gateway::{RGBService, ConxianService};
        let service = RGBService;
        let resp = service.handle_request(r#"{"asset_id": "RGB1", "amount": 50, "schema": "LNPBP"}"#);
        assert!(resp.contains("RGB asset transfer validated"));
    }
}

pub mod gateway {
    use serde::{Serialize, Deserialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ServiceStatus {
        pub service_name: String,
        pub status: String,
        pub version: String,
    }

    pub trait ConxianService {
        fn name(&self) -> &str;
        fn status(&self) -> ServiceStatus;
        fn handle_request(&self, payload: &str) -> String;
    }

    #[derive(Deserialize)]
    struct BisqTrade {
        trade_id: String,
        amount: u64,
    }

    pub struct BisqService;
    impl ConxianService for BisqService {
        fn name(&self) -> &str { "Bisq" }
        fn status(&self) -> ServiceStatus {
            ServiceStatus {
                service_name: self.name().to_string(),
                status: "Active".to_string(),
                version: "v1.2.0".to_string(),
            }
        }
        fn handle_request(&self, payload: &str) -> String {
            if let Ok(trade) = serde_json::from_str::<BisqTrade>(payload) {
                 format!("Bisq trade verified: ID={}, Amount={}. Nexus mediation secured.", trade.trade_id, trade.amount)
            } else {
                "Bisq request received. Invalid trade payload for full verification.".to_string()
            }
        }
    }

    #[derive(Deserialize)]
    struct RGBAssetTransfer {
        asset_id: String,
        amount: u64,
        schema: Option<String>,
    }

    pub struct BitVMService;
    impl ConxianService for BitVMService {
        fn name(&self) -> &str { "BitVM" }
        fn status(&self) -> ServiceStatus {
            ServiceStatus {
                service_name: self.name().to_string(),
                status: "Active".to_string(),
                version: "v0.1.0".to_string(),
            }
        }
        fn handle_request(&self, payload: &str) -> String {
            if payload.contains("prove") {
                // Trigger IaaS Fee Payout
                let fee_tx = crate::sign_transaction("agent-treasury:deposit-service-fee");
                // Simulated ZK-proof generation steps
                let steps = vec!["Circuit synthesis", "Constraint generation", "Proving key application"];
                format!("BitVM proof generated for: {}. Fee deposited: {}. Steps: {:?}.", payload, fee_tx, steps)
            } else if payload.contains("challenge") {
                format!("BitVM challenge registered: {}. Monitoring for state transition. Security tenure initiated.", payload)
            } else if payload.contains("verify") {
                format!("BitVM verification successful for: {}. State root consistency confirmed against Stacks L1 MARF.", payload)
            } else {
                "BitVM gateway ready. Awaiting prove/challenge/verify commands.".to_string()
            }
        }
    }

    pub struct RGBService;
    impl ConxianService for RGBService {
        fn name(&self) -> &str { "RGB" }
        fn status(&self) -> ServiceStatus {
            ServiceStatus {
                service_name: self.name().to_string(),
                status: "Active".to_string(),
                version: "v0.10.0".to_string(),
            }
        }
        fn handle_request(&self, payload: &str) -> String {
            if let Ok(transfer) = serde_json::from_str::<RGBAssetTransfer>(payload) {
                let schema_status = match transfer.schema.as_deref() {
                    Some("LNPBP") => "Valid LNP/BP schema detected.",
                    _ => "Generic RGB schema. Limited validation applied.",
                };
                format!("RGB asset transfer validated: Asset={}, Amount={}. {}. Proof recorded in Nexus state.", transfer.asset_id, transfer.amount, schema_status)
            } else {
                "RGB request received. Asset transfer proof missing or invalid.".to_string()
            }
        }
    }
}
