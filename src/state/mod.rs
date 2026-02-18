use std::sync::Mutex;
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerkleProof {
    pub leaf: String,
    pub path: Vec<(String, bool)>, // (hash, is_left)
    pub root: String,
}

pub struct NexusState {
    leaves: Mutex<Vec<String>>,
    state_root: Mutex<String>,
    last_updated: Mutex<DateTime<Utc>>,
}

impl NexusState {
    pub fn new() -> Self {
        let initial_root = "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
        Self {
            leaves: Mutex::new(Vec::new()),
            state_root: Mutex::new(initial_root),
            last_updated: Mutex::new(Utc::now()),
        }
    }

    pub fn get_state_root(&self) -> String {
        self.state_root.lock().unwrap().clone()
    }

    pub fn update_state(&self, data: &str, _tx_count: usize) {
        let mut leaves = self.leaves.lock().unwrap();
        leaves.push(data.to_string());

        let new_root = self.calculate_root(&leaves);
        *self.state_root.lock().unwrap() = new_root;
        *self.last_updated.lock().unwrap() = Utc::now();

        tracing::debug!("Nexus state updated. New root: {}", self.get_state_root());
    }

    fn calculate_root(&self, leaves: &[String]) -> String {
        if leaves.is_empty() {
            return "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
        }

        let mut current_level: Vec<[u8; 32]> = leaves.iter().map(|l| {
            let mut hasher = Sha256::new();
            hasher.update(l.as_bytes());
            hasher.finalize().into()
        }).collect();

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in current_level.chunks(2) {
                let mut hasher = Sha256::new();
                if chunk.len() == 2 {
                    hasher.update(chunk[0]);
                    hasher.update(chunk[1]);
                } else {
                    hasher.update(chunk[0]);
                    hasher.update(chunk[0]); // Duplicate last leaf if odd
                }
                next_level.push(hasher.finalize().into());
            }
            current_level = next_level;
        }

        format!("0x{}", hex::encode(Sha256::digest(&current_level[0])))
    }

    pub fn generate_proof(&self, key: &str) -> (String, String) {
        let leaves = self.leaves.lock().unwrap();
        let index = leaves.iter().position(|l| l == key);

        match index {
            Some(_) => {
                let root = self.get_state_root();
                let mut hasher = Sha256::new();
                hasher.update(key.as_bytes());
                let proof = format!("0x{}", hex::encode(hasher.finalize()));
                (root, proof)
            },
            None => (self.get_state_root(), "0x0".to_string())
        }
    }

    pub fn generate_merkle_proof(&self, key: &str) -> Option<MerkleProof> {
        let leaves = self.leaves.lock().unwrap();
        let index = leaves.iter().position(|l| l == key)?;

        let mut current_level: Vec<[u8; 32]> = leaves.iter().map(|l| {
            let mut hasher = Sha256::new();
            hasher.update(l.as_bytes());
            hasher.finalize().into()
        }).collect();

        let mut path = Vec::new();
        let mut idx = index;

        while current_level.len() > 1 {
            let mut next_level = Vec::new();

            let sibling_idx = if idx % 2 == 0 {
                if idx + 1 < current_level.len() {
                    idx + 1
                } else {
                    idx
                }
            } else {
                idx - 1
            };

            path.push((format!("0x{}", hex::encode(current_level[sibling_idx])), idx % 2 == 0));

            for chunk in current_level.chunks(2) {
                let mut hasher = Sha256::new();
                if chunk.len() == 2 {
                    hasher.update(chunk[0]);
                    hasher.update(chunk[1]);
                } else {
                    hasher.update(chunk[0]);
                    hasher.update(chunk[0]);
                }
                next_level.push(hasher.finalize().into());
            }
            current_level = next_level;
            idx /= 2;
        }

        Some(MerkleProof {
            leaf: key.to_string(),
            path,
            root: self.get_state_root(),
        })
    }
}

pub fn verify_merkle_proof(proof: &MerkleProof) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(proof.leaf.as_bytes());
    let mut current_hash: [u8; 32] = hasher.finalize().into();

    for (sibling_hash_str, is_left) in &proof.path {
        let sibling_hash = hex::decode(sibling_hash_str.trim_start_matches("0x")).unwrap_or_default();
        let mut hasher = Sha256::new();
        if *is_left {
            hasher.update(current_hash);
            hasher.update(sibling_hash);
        } else {
            hasher.update(sibling_hash);
            hasher.update(current_hash);
        }
        current_hash = hasher.finalize().into();
    }

    let final_root = format!("0x{}", hex::encode(Sha256::digest(&current_hash)));
    final_root == proof.root
}
