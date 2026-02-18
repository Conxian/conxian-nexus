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
        println!("Signature: {}", signature);
    }
}
