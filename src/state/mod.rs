use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: String,
    pub path: Vec<(String, bool)>, // (hash, is_left)
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MMRProof {
    pub leaf: String,
    pub pos: u64,
    pub siblings: Vec<(u64, String)>, // (pos, hash)
    pub peaks: Vec<String>,
    pub root: String,
}

pub struct NexusState {
    pub state_root: Mutex<String>,
    pub leaves: Mutex<Vec<String>>,
    pub tree_levels: Mutex<Vec<Vec<[u8; 32]>>>,
    pub mmr: Mutex<MMRFoundation>,
}

impl Default for NexusState {
    fn default() -> Self {
        Self::new()
    }
}

impl NexusState {
    pub fn new() -> Self {
        Self {
            state_root: Mutex::new(
                "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            ),
            leaves: Mutex::new(Vec::new()),
            tree_levels: Mutex::new(Vec::new()),
            mmr: Mutex::new(MMRFoundation::new()),
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

    pub fn update_state(&self, tx_id: &str, _height: u64) {
        self.update_state_batch(&[tx_id.to_string()]);
    }

    pub fn update_state_batch(&self, tx_ids: &[String]) -> Vec<(u64, [u8; 32])> {
        let mut leaves = self.leaves.lock().unwrap();
        leaves.extend_from_slice(tx_ids);
        self.rebuild_tree(&leaves);

        let mut mmr = self.mmr.lock().unwrap();
        let mut added_nodes = Vec::new();
        for tx_id in tx_ids {
            let nodes = mmr.add_leaf(tx_id.as_bytes());
            added_nodes.extend(nodes);
        }
        added_nodes
    }

    pub fn set_initial_leaves(&self, leaves: Vec<String>) {
        let mut internal_leaves = self.leaves.lock().unwrap();
        *internal_leaves = leaves.clone();
        self.rebuild_tree(&internal_leaves);

        let mut mmr = self.mmr.lock().unwrap();
        mmr.peaks = Vec::new();
        mmr.size = 0;
        mmr.node_count = 0;
        for leaf in &leaves {
            mmr.add_leaf(leaf.as_bytes());
        }

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
            *self.state_root.lock().unwrap() =
                "0x0000000000000000000000000000000000000000000000000000000000000000".to_string();
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
            let mut next_level = Vec::with_capacity(current_level.len().div_ceil(2));
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

    pub fn generate_proof(&self, key: &str) -> (String, String) {
        match self.generate_merkle_proof(key) {
            Some(proof) => {
                let proof_json = serde_json::to_string(&proof).unwrap_or_default();
                (proof.root.clone(), proof_json)
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

    pub fn get_leaf_index(&self, tx_id: &str) -> Option<usize> {
        self.leaves.lock().unwrap().iter().position(|l| l == tx_id)
    }

    pub fn get_mmr_proof_metadata(&self, leaf_index: usize) -> (u64, Vec<u64>) {
        let mut pos = 0u64;
        let siblings = Vec::new();

        for i in 0..leaf_index {
            pos += 1;
            let mut s = i;
            while s & 1 == 1 {
                pos += 1;
                s >>= 1;
            }
        }

        (pos, siblings)
    }

    pub fn assemble_mmr_proof(
        &self,
        leaf: String,
        pos: u64,
        siblings: Vec<(u64, String)>,
    ) -> MMRProof {
        let mmr = self.mmr.lock().unwrap();
        let peaks = mmr
            .peaks
            .iter()
            .map(|p| format!("0x{}", hex::encode(p)))
            .collect();
        MMRProof {
            leaf,
            pos,
            siblings,
            peaks,
            root: mmr.get_root(),
        }
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

impl Default for MMRFoundation {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MMRFoundation {
    pub peaks: Vec<[u8; 32]>,
    pub size: usize,
    pub node_count: u64,
}

impl MMRFoundation {
    pub fn new() -> Self {
        Self {
            peaks: Vec::new(),
            size: 0,
            node_count: 0,
        }
    }

    pub fn add_leaf(&mut self, leaf: &[u8]) -> Vec<(u64, [u8; 32])> {
        let mut hasher = Sha256::new();
        hasher.update(leaf);
        let mut current_hash: [u8; 32] = hasher.finalize().into();

        let mut added_nodes = Vec::new();
        let leaf_pos = self.node_count;
        added_nodes.push((leaf_pos, current_hash));
        self.node_count += 1;

        let mut pos = self.size;

        while pos & 1 == 1 {
            let peak = self.peaks.pop().expect("Peak must exist if bit is set");
            let mut hasher = Sha256::new();
            hasher.update(peak);
            hasher.update(current_hash);
            current_hash = hasher.finalize().into();

            let internal_pos = self.node_count;
            added_nodes.push((internal_pos, current_hash));
            self.node_count += 1;

            pos >>= 1;
        }

        self.peaks.push(current_hash);
        self.size += 1;
        added_nodes
    }

    pub fn get_root(&self) -> String {
        if self.peaks.is_empty() {
            return "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string();
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
        assert_eq!(
            state.get_state_root(),
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn test_update_state_batch() {
        let state = NexusState::new();
        state.update_state_batch(&["tx1".to_string(), "tx2".to_string()]);
        let root = state.get_state_root();
        assert_ne!(
            root,
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn test_merkle_proof_verification() {
        let state = NexusState::new();
        let leaves = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        state.set_initial_leaves(leaves);

        let proof = state.generate_merkle_proof("b").unwrap();
        assert!(verify_merkle_proof(&proof));
    }

    #[test]
    fn test_mmr_metadata_calculation() {
        let state = NexusState::new();
        let (pos0, _) = state.get_mmr_proof_metadata(0);
        let (pos1, _) = state.get_mmr_proof_metadata(1);
        let (pos2, _) = state.get_mmr_proof_metadata(2);

        assert_eq!(pos0, 0);
        assert_eq!(pos1, 1);
        assert_eq!(pos2, 3);
    }
}
