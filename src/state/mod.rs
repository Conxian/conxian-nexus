use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

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

impl Default for NexusState {
    fn default() -> Self {
        Self::new()
    }
}

impl NexusState {
    pub fn new() -> Self {
        let initial_root =
            "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
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
        self.update_state_batch(&[data.to_string()]);
    }

    pub fn update_state_batch(&self, data: &[String]) {
        let mut leaves = self.leaves.lock().unwrap();
        for item in data {
            leaves.push(item.clone());
        }

        let new_root = self.calculate_root(&leaves);
        *self.state_root.lock().unwrap() = new_root;
        *self.last_updated.lock().unwrap() = Utc::now();

        tracing::debug!("Nexus state updated. New root: {}", self.get_state_root());
    }

    pub fn set_initial_leaves(&self, new_leaves: Vec<String>) {
        let mut leaves = self.leaves.lock().unwrap();
        *leaves = new_leaves;
        let new_root = self.calculate_root(&leaves);
        *self.state_root.lock().unwrap() = new_root;
        *self.last_updated.lock().unwrap() = Utc::now();
        tracing::info!(
            "Nexus state initialized with {} leaves. Root: {}",
            leaves.len(),
            self.get_state_root()
        );
    }

    /// Optimized Merkle root calculation.
    /// In a production environment with millions of leaves, this would use
    /// an incremental approach or a persistent Merkle Mountain Range.
    fn calculate_root(&self, leaves: &[String]) -> String {
        if leaves.is_empty() {
            return "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string();
        }

        let mut current_level: Vec<[u8; 32]> = leaves
            .iter()
            .map(|l| {
                let mut hasher = Sha256::new();
                hasher.update(l.as_bytes());
                hasher.finalize().into()
            })
            .collect();

        while current_level.len() > 1 {
            let mut next_level = Vec::with_capacity((current_level.len() + 1) / 2);
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

        format!("0x{}", hex::encode(current_level[0]))
    }

    pub fn generate_proof(&self, key: &str) -> (String, String) {
        match self.generate_merkle_proof(key) {
            Some(proof) => {
                let proof_json = serde_json::to_string(&proof).unwrap_or_default();
                (proof.root, proof_json)
            }
            None => (self.get_state_root(), "{}".to_string()),
        }
    }

    pub fn generate_merkle_proof(&self, key: &str) -> Option<MerkleProof> {
        let leaves = self.leaves.lock().unwrap();
        let index = leaves.iter().position(|l| l == key)?;

        let mut current_level: Vec<[u8; 32]> = leaves
            .iter()
            .map(|l| {
                let mut hasher = Sha256::new();
                hasher.update(l.as_bytes());
                hasher.finalize().into()
            })
            .collect();

        let mut path = Vec::new();
        let mut idx = index;

        while current_level.len() > 1 {
            let sibling_idx = if idx % 2 == 0 {
                if idx + 1 < current_level.len() {
                    idx + 1
                } else {
                    idx
                }
            } else {
                idx - 1
            };

            path.push((
                format!("0x{}", hex::encode(current_level[sibling_idx])),
                idx % 2 == 0,
            ));

            let mut next_level = Vec::with_capacity((current_level.len() + 1) / 2);
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
        let sibling_hash = match hex::decode(sibling_hash_str.trim_start_matches("0x")) {
            Ok(h) => h,
            Err(_) => return false,
        };
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

    let final_root = format!("0x{}", hex::encode(current_hash));
    final_root == proof.root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_nexus_state() {
        let state = NexusState::new();
        assert_eq!(state.get_state_root(), "0x0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_calculate_root_empty() {
        let state = NexusState::new();
        assert_eq!(state.calculate_root(&[]), "0x0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_calculate_root_single() {
        let state = NexusState::new();
        let root = state.calculate_root(&["leaf1".to_string()]);
        assert_ne!(root, "0x0000000000000000000000000000000000000000000000000000000000000000");

        let mut hasher = Sha256::new();
        hasher.update("leaf1".as_bytes());
        let expected = format!("0x{}", hex::encode(hasher.finalize()));
        assert_eq!(root, expected);
    }

    #[test]
    fn test_update_state_batch() {
        let state = NexusState::new();
        state.update_state_batch(&["tx1".to_string(), "tx2".to_string()]);
        let root = state.get_state_root();
        assert_ne!(root, "0x0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_merkle_proof_verification_internal() {
        let state = NexusState::new();
        let leaves = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        state.set_initial_leaves(leaves);

        let proof = state.generate_merkle_proof("b").unwrap();
        assert!(verify_merkle_proof(&proof));

        let invalid_proof = MerkleProof {
            leaf: "b".to_string(),
            path: proof.path,
            root: "0xwrong".to_string(),
        };
        assert!(!verify_merkle_proof(&invalid_proof));
    }

    #[test]
    fn test_merkle_proof_odd_leaves() {
        let state = NexusState::new();
        let leaves = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string()];
        state.set_initial_leaves(leaves);

        for leaf in &["a", "b", "c", "d", "e"] {
            let proof = state.generate_merkle_proof(leaf).unwrap();
            assert!(verify_merkle_proof(&proof), "Failed for leaf {}", leaf);
        }
    }

    #[test]
    fn test_merkle_proof_not_found() {
        let state = NexusState::new();
        state.update_state_batch(&["a".to_string()]);
        let proof = state.generate_merkle_proof("non-existent");
        assert!(proof.is_none());
    }
}
