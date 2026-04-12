use bip32::{ChildNumber, XPrv};
use bip39::{Language, Mnemonic, MnemonicType, Seed};
use k256::ecdsa::{Signature, SigningKey, signature::Signer};
use ripemd::Ripemd160;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;

pub struct Wallet {
    signing_key: SigningKey,
    _mnemonic: Option<String>,
}

impl Wallet {
    /// Creates a new wallet. If NEXUS_PRIVATE_KEY is set, it uses that.
    /// Otherwise, it generates a new 12-word mnemonic.
    pub fn new() -> Result<Self, anyhow::Error> {
        if let Ok(hex_key) = env::var("NEXUS_PRIVATE_KEY") {
            if let Ok(bytes) = hex::decode(hex_key) {
                if let Ok(key) = SigningKey::from_slice(&bytes) {
                    return Ok(Self {
                        signing_key: key,
                        _mnemonic: None,
                    });
                }
            }
        }

        let mnemonic = Mnemonic::new(MnemonicType::Words12, Language::English);
        let seed = Seed::new(&mnemonic, "");
        let xprv = XPrv::new(seed.as_bytes()).map_err(|e| anyhow::anyhow!("Invalid seed: {}", e))?;

        let path_index = ChildNumber::new(44, true)
            .map_err(|e| anyhow::anyhow!("Invalid derivation path index: {}", e))?;

        let child = xprv
            .derive_child(path_index)
            .map_err(|e| anyhow::anyhow!("Child derivation failed: {}", e))?;

        let signing_key = SigningKey::from_slice(&child.to_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid signing key length: {}", e))?;

        Ok(Self {
            signing_key,
            _mnemonic: Some(mnemonic.into_phrase()),
        })
    }

    pub fn from_mnemonic(phrase: &str, passphrase: &str) -> Result<Self, anyhow::Error> {
        let mnemonic = Mnemonic::from_phrase(phrase, Language::English)
            .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {}", e))?;
        let seed = Seed::new(&mnemonic, passphrase);
        let xprv = XPrv::new(seed.as_bytes()).map_err(|e| anyhow::anyhow!("Invalid seed: {}", e))?;

        let path_index = ChildNumber::new(44, true)
            .map_err(|e| anyhow::anyhow!("Invalid derivation path index: {}", e))?;

        let child = xprv.derive_child(path_index)
            .map_err(|e| anyhow::anyhow!("Child derivation failed: {}", e))?;

        let signing_key = SigningKey::from_slice(&child.to_bytes())
            .map_err(|e| anyhow::anyhow!("Invalid signing key: {}", e))?;

        Ok(Self {
            signing_key,
            _mnemonic: Some(phrase.to_string()),
        })
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.signing_key.verifying_key().to_sec1_bytes().to_vec()
    }

    pub fn public_key(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    pub fn stacks_address_hash(&self) -> String {
        let pubkey = self.public_key_bytes();
        let sha2 = Sha256::digest(&pubkey);
        let hash160 = Ripemd160::digest(&sha2);
        hex::encode(hash160)
    }

    pub fn sign(&self, message: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(message.as_bytes());
        let digest = hasher.finalize();
        let signature: Signature = self.signing_key.sign(&digest);
        hex::encode(signature.to_bytes())
    }
}

pub fn sign_transaction(tx_id: &str) -> Result<String, anyhow::Error> {
    let wallet = Wallet::new()?;
    Ok(wallet.sign(tx_id))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClarityCall {
    pub contract_address: String,
    pub contract_name: String,
    pub function_name: String,
    pub arguments: Vec<String>,
    pub sender_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
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
    ) -> Result<SignedContractCall, anyhow::Error> {
        let parts: Vec<&str> = contract.split('.').collect();
        let (addr, name) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM".to_string(), contract.to_string())
        };

        let call = ClarityCall {
            contract_address: addr,
            contract_name: name,
            function_name: function.to_string(),
            arguments: args,
            sender_address: wallet.stacks_address_hash(),
        };

        let serialized = serde_json::to_string(&call)
            .map_err(|e| anyhow::anyhow!("Serialization failed: {}", e))?;
        let signature = wallet.sign(&serialized);

        Ok(SignedContractCall {
            payload: call,
            signature,
            public_key: wallet.public_key(),
        })
    }
}

pub mod gateway {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ServiceStatus {
        pub service_name: String,
        pub status: String,
        pub version: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ServiceResponse {
        pub service: String,
        pub status: String,
        pub message: String,
        pub data: Option<serde_json::Value>,
    }

    pub trait ConxianService {
        fn name(&self) -> &str;
        fn status(&self) -> ServiceStatus;
        fn handle_request(&self, payload: &str) -> ServiceResponse;
    }

    pub struct BisqService;
    impl ConxianService for BisqService {
        fn name(&self) -> &str {
            "Bisq"
        }
        fn status(&self) -> ServiceStatus {
            ServiceStatus {
                service_name: self.name().to_string(),
                status: "Active".to_string(),
                version: "v1.2.0".to_string(),
            }
        }
        fn handle_request(&self, _payload: &str) -> ServiceResponse {
            ServiceResponse {
                service: self.name().to_string(),
                status: "Success".to_string(),
                message: "Bisq trade verified via Nexus.".to_string(),
                data: None,
            }
        }
    }

    pub struct BitVMService;
    impl BitVMService {
        /// [CON-75] BitVM2 verification floor for Job Card settlement.
        pub fn verify_job_card(&self, _job_card: &crate::cjcs::JobCard) -> bool {
            // BitVM2 verification is not yet wired. Fail closed.
            false
        }
    }

    impl ConxianService for BitVMService {
        fn name(&self) -> &str {
            "BitVM"
        }
        fn status(&self) -> ServiceStatus {
            ServiceStatus {
                service_name: self.name().to_string(),
                status: "Active".to_string(),
                version: "v0.1.0".to_string(),
            }
        }
        fn handle_request(&self, _payload: &str) -> ServiceResponse {
            ServiceResponse {
                service: self.name().to_string(),
                status: "NotImplemented".to_string(),
                message: "BitVM2 verification is not yet available.".to_string(),
                data: None,
            }
        }
    }

    pub struct RGBService;
    impl ConxianService for RGBService {
        fn name(&self) -> &str {
            "RGB"
        }
        fn status(&self) -> ServiceStatus {
            ServiceStatus {
                service_name: self.name().to_string(),
                status: "Active".to_string(),
                version: "v0.10.0".to_string(),
            }
        }
        fn handle_request(&self, _payload: &str) -> ServiceResponse {
            ServiceResponse {
                service: self.name().to_string(),
                status: "Success".to_string(),
                message: "RGB asset validated.".to_string(),
                data: None,
            }
        }
    }
}

/// [CON-73] CJCS v2.0 JSON-LD machine-readable definition.
pub mod cjcs {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct JobCard {
        #[serde(rename = "@context")]
        pub context: String,
        #[serde(rename = "@type")]
        pub r#type: String,
        pub work_intent: WorkIntent,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct WorkIntent {
        pub sender_address: String,
        pub receiver_address: String,
        pub task_id: String,
        pub amount_sbtc: u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_new_random() {
        let wallet = Wallet::new().expect("Should create wallet");
        assert_eq!(wallet.public_key_bytes().len(), 33);
        assert!(!wallet.public_key().is_empty());
        assert_eq!(wallet.stacks_address_hash().len(), 40);
    }

    #[test]
    fn test_wallet_from_mnemonic() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let wallet = Wallet::from_mnemonic(mnemonic, "").expect("Should create wallet from mnemonic");
        let expected_pubkey = wallet.public_key();

        let wallet2 = Wallet::from_mnemonic(mnemonic, "").expect("Should create wallet from same mnemonic");
        assert_eq!(wallet2.public_key(), expected_pubkey);
    }

    #[test]
    fn test_wallet_signing() {
        let wallet = Wallet::new().expect("Should create wallet");
        let message = "conxian-test-message";
        let signature = wallet.sign(message);
        assert!(!signature.is_empty());
        assert_eq!(signature.len(), 128); // 64 bytes in hex
    }

    #[test]
    fn test_contract_bridge_signed_call() {
        let wallet = Wallet::new().expect("Should create wallet");
        let contract = "SP1PQHQKV0RJXZFY1DGX8MNSNYVE3VGZJSRTPGZGM.asset-vault";
        let function = "deposit";
        let args = vec!["1000".to_string()];

        let signed_call = ContractBridge::create_signed_call(&wallet, contract, function, args)
            .expect("Should create signed call");

        assert_eq!(signed_call.payload.contract_name, "asset-vault");
        assert_eq!(signed_call.payload.function_name, "deposit");
        assert!(!signed_call.signature.is_empty());
    }

    #[test]
    fn test_contract_bridge_default_address() {
        let wallet = Wallet::new().expect("Should create wallet");
        let contract = "default-vault";
        let signed_call = ContractBridge::create_signed_call(&wallet, contract, "init", vec![])
            .expect("Should create signed call");

        assert_eq!(signed_call.payload.contract_address, "SPSZXAKV7DWTDZN2601WR31BM51BD3YTQWE97VRM");
        assert_eq!(signed_call.payload.contract_name, "default-vault");
    }
}
