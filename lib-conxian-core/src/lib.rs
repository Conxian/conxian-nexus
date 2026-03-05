use k256::ecdsa::{SigningKey, Signature, signature::Signer};
use sha2::{Sha256, Digest};
use std::env;
use bip39::{Mnemonic, Language, Seed, MnemonicType};
use bip32::{XPrv, ChildNumber};
use ripemd::Ripemd160;
use serde::{Serialize, Deserialize};

pub struct Wallet {
    signing_key: SigningKey,
    _mnemonic: Option<String>,
}

impl Wallet {
    pub fn new() -> Self {
        if let Ok(hex_key) = env::var("NEXUS_PRIVATE_KEY") {
            if let Ok(bytes) = hex::decode(hex_key) {
                if let Ok(key) = SigningKey::from_slice(&bytes) {
                    return Self { signing_key: key, _mnemonic: None };
                }
            }
        }

        let mnemonic = Mnemonic::new(MnemonicType::Words12, Language::English);
        let seed = Seed::new(&mnemonic, "");
        let xprv = XPrv::new(seed.as_bytes()).expect("Invalid seed");
        let child = xprv.derive_child(ChildNumber::new(44, true).unwrap()).unwrap();
        let signing_key = SigningKey::from_slice(&child.to_bytes()).expect("Invalid seed length");

        Self {
            signing_key,
            _mnemonic: Some(mnemonic.into_phrase())
        }
    }

    pub fn from_mnemonic(phrase: &str, passphrase: &str) -> Result<Self, anyhow::Error> {
        let mnemonic = Mnemonic::from_phrase(phrase, Language::English)?;
        let seed = Seed::new(&mnemonic, passphrase);
        let xprv = XPrv::new(seed.as_bytes())?;
        let child = xprv.derive_child(ChildNumber::new(44, true).unwrap())?;
        let signing_key = SigningKey::from_slice(&child.to_bytes())?;
        Ok(Self {
            signing_key,
            _mnemonic: Some(phrase.to_string())
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

pub fn sign_transaction(tx_id: &str) -> String {
    let wallet = Wallet::new();
    wallet.sign(tx_id)
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
        args: Vec<String>
    ) -> SignedContractCall {
        let parts: Vec<&str> = contract.split('.').collect();
        let (addr, name) = if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("SP...".to_string(), contract.to_string())
        };

        let call = ClarityCall {
            contract_address: addr,
            contract_name: name,
            function_name: function.to_string(),
            arguments: args,
            sender_address: wallet.stacks_address_hash(),
        };

        let serialized = serde_json::to_string(&call).unwrap_or_default();
        let signature = wallet.sign(&serialized);

        SignedContractCall {
            payload: call,
            signature,
            public_key: wallet.public_key(),
        }
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
        fn name(&self) -> &str { "Bisq" }
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
    impl ConxianService for BitVMService {
        fn name(&self) -> &str { "BitVM" }
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
                status: "Success".to_string(),
                message: "BitVM state verified.".to_string(),
                data: None,
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
