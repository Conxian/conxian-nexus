use std::sync::Mutex;
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};

pub struct NexusState {
    state_root: Mutex<String>,
    last_updated: Mutex<DateTime<Utc>>,
}

impl NexusState {
    pub fn new() -> Self {
        Self {
            state_root: Mutex::new("0x0000000000000000000000000000000000000000000000000000000000000000".to_string()),
            last_updated: Mutex::new(Utc::now()),
        }
    }

    pub fn get_state_root(&self) -> String {
        self.state_root.lock().unwrap().clone()
    }

    pub fn update_state(&self, block_hash: &str, tx_count: usize) {
        let mut root = self.state_root.lock().unwrap();
        let mut hasher = Sha256::new();
        hasher.update(root.as_bytes());
        hasher.update(block_hash.as_bytes());
        hasher.update(tx_count.to_be_bytes());

        *root = format!("0x{:x}", hasher.finalize());
        *self.last_updated.lock().unwrap() = Utc::now();

        tracing::debug!("Nexus state updated. New root: {}", *root);
    }

    pub fn generate_proof(&self, key: &str) -> (String, String) {
        let root = self.get_state_root();
        let mut hasher = Sha256::new();
        hasher.update(root.as_bytes());
        hasher.update(key.as_bytes());
        let proof = format!("0x{:x}", hasher.finalize());
        (root, proof)
    }
}
