use k256::ecdsa::{SigningKey, Signature, signature::Signer};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};

pub struct Wallet {
    signing_key: SigningKey,
}

impl Wallet {
    pub fn new() -> Self {
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
    // Legacy support or simplified helper
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
    fn test_bitvm_service_handling() {
        use crate::gateway::{BitVMService, ConxianService};
        let service = BitVMService;
        assert_eq!(service.handle_request("prove something"), "BitVM proof generated");
        assert_eq!(service.handle_request("challenge this"), "BitVM challenge registered");
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
        fn handle_request(&self, _payload: &str) -> String {
            "Bisq request processed".to_string()
        }
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
                "BitVM proof generated".to_string()
            } else if payload.contains("challenge") {
                "BitVM challenge registered".to_string()
            } else {
                "BitVM request processed".to_string()
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
        fn handle_request(&self, _payload: &str) -> String {
            "RGB request processed".to_string()
        }
    }
}
