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
    tree_levels: Mutex<Vec<Vec<[u8; 32]>>>,
    state_root: Mutex<String>,
    mmr: Mutex<MMRFoundation>,
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
            tree_levels: Mutex::new(Vec::new()),
            state_root: Mutex::new(initial_root),
            mmr: Mutex::new(MMRFoundation::new()),
            last_updated: Mutex::new(Utc::now()),
        }
    }

    pub fn get_state_root(&self) -> String {
        self.state_root.lock().unwrap().clone()
    }

    pub fn get_mmr_root(&self) -> String {
        self.mmr.lock().unwrap().get_root()
    }

    pub fn get_mmr_state(&self) -> (Vec<[u8; 32]>, usize) {
        let mmr = self.mmr.lock().unwrap();
        (mmr.peaks.clone(), mmr.size)
    }

    pub fn update_state(&self, data: &str, _tx_count: usize) {
        self.update_state_batch(&[data.to_string()]);
    }

    pub fn update_state_batch(&self, data: &[String]) {
        let mut leaves = self.leaves.lock().unwrap();
        let mut mmr = self.mmr.lock().unwrap();

        for item in data {
            leaves.push(item.clone());
            mmr.add_leaf(item.as_bytes());
        }

        self.rebuild_tree(&leaves);
        *self.last_updated.lock().unwrap() = Utc::now();

        tracing::debug!("Nexus state updated. New root: {}", self.get_state_root());
    }

    pub fn set_initial_leaves(&self, new_leaves: Vec<String>) {
        let mut leaves = self.leaves.lock().unwrap();
        let mut mmr = self.mmr.lock().unwrap();

        *leaves = new_leaves;
        *mmr = MMRFoundation::new();
        for leaf in leaves.iter() {
            mmr.add_leaf(leaf.as_bytes());
        }

        self.rebuild_tree(&leaves);
        *self.last_updated.lock().unwrap() = Utc::now();
        tracing::info!(
            "Nexus state initialized with {} leaves. Root: {}, MMR Root: {}",
            leaves.len(),
            self.get_state_root(),
            mmr.get_root()
        );
    }

    pub fn set_mmr_state(&self, peaks: Vec<[u8; 32]>, size: usize) {
        let mut mmr = self.mmr.lock().unwrap();
        mmr.peaks = peaks;
        mmr.size = size;
        tracing::debug!("MMR state updated manually. New root: {}", mmr.get_root());
    }

    fn rebuild_tree(&self, leaves: &[String]) {
        if leaves.is_empty() {
            *self.state_root.lock().unwrap() = "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
            *self.tree_levels.lock().unwrap() = Vec::new();
            return;
        }

        let mut levels = Vec::new();
        let mut current_level: Vec<[u8; 32]> = leaves
            .iter()
            .map(|l| {
                let mut hasher = Sha256::new();
                hasher.update(l.as_bytes());
                hasher.finalize().into()
            })
            .collect();

        levels.push(current_level.clone());

        while current_level.len() > 1 {
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
            levels.push(current_level.clone());
        }

        *self.state_root.lock().unwrap() = format!("0x{}", hex::encode(current_level[0]));
        *self.tree_levels.lock().unwrap() = levels;
    }

    pub fn calculate_root(&self, leaves: &[String]) -> String {
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
                    hasher.update(chunk[0]);
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
        let levels = self.tree_levels.lock().unwrap();
        let index = leaves.iter().position(|l| l == key)?;

        if levels.is_empty() {
            return None;
        }

        let mut path = Vec::new();
        let mut idx = index;

        for level in &levels[..levels.len() - 1] {
            let sibling_idx = if idx % 2 == 0 {
                if idx + 1 < level.len() {
                    idx + 1
                } else {
                    idx
                }
            } else {
                idx - 1
            };

            path.push((
                format!("0x{}", hex::encode(level[sibling_idx])),
                idx % 2 == 0,
            ));
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

/// Minimal Merkle Mountain Range (MMR) foundation for future persistence logic.
/// See roadmap 4.1 in docs/PRD.md.
pub struct MMRFoundation {
    pub peaks: Vec<[u8; 32]>,
    pub size: usize,
}

impl MMRFoundation {
    pub fn new() -> Self {
        Self { peaks: Vec::new(), size: 0 }
    }

    pub fn add_leaf(&mut self, leaf: &[u8]) {
        let mut current_hash: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(leaf);
            hasher.finalize().into()
        };

        let mut pos = self.size;

        // Simple MMR logic: merge peaks of the same height
        while pos & 1 == 1 {
            let peak = self.peaks.pop().expect("Peak must exist if bit is set");
            let mut hasher = Sha256::new();
            hasher.update(peak);
            hasher.update(current_hash);
            current_hash = hasher.finalize().into();
            pos >>= 1;
        }

        self.peaks.push(current_hash);
        self.size += 1;
    }

    pub fn get_root(&self) -> String {
        if self.peaks.is_empty() {
            return "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
        }

        let mut root_hash = self.peaks[0];
        for i in 1..self.peaks.len() {
            let mut hasher = Sha256::new();
            hasher.update(self.peaks[i]);
            hasher.update(root_hash);
            root_hash = hasher.finalize().into();
        }

        format!("0x{}", hex::encode(root_hash))
    }
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
    fn test_update_state_batch() {
        let state = NexusState::new();
        state.update_state_batch(&["tx1".to_string(), "tx2".to_string()]);
        let root = state.get_state_root();
        assert_ne!(root, "0x0000000000000000000000000000000000000000000000000000000000000000");
        assert_ne!(state.get_mmr_root(), "0x0000000000000000000000000000000000000000000000000000000000000000");
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

    #[test]
    fn test_mmr_foundation() {
        let mut mmr = MMRFoundation::new();
        mmr.add_leaf(b"leaf1");
        let root1 = mmr.get_root();
        assert_ne!(root1, "0x0000000000000000000000000000000000000000000000000000000000000000");

        mmr.add_leaf(b"leaf2");
        let root2 = mmr.get_root();
        assert_ne!(root1, root2);
    }
}
