//! nexus-state module provides a high-performance Merkle Tree implementation
//! for tracking and transitioning the cryptographic state root of the Nexus.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MerkleProof {
    pub leaf: String,
    pub root: String,
    pub path: Vec<(String, bool)>, // (sibling_hash, is_left)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MMRProof {
    pub leaf: String,
    pub pos: u64,
    pub siblings: Vec<(u64, String)>, // (position, hash)
    pub peaks: Vec<String>,
    pub root: String,
}

/// [NEXUS-STATE-01] Cryptographic state tracking.
/// Optimized with intermediate level caching and MMR support.
pub struct NexusState {
    pub leaves: Mutex<Vec<String>>,
    pub mmr: Mutex<MMRFoundation>,
}

impl NexusState {
    pub fn new() -> Self {
        Self {
            leaves: Mutex::new(Vec::new()),
            mmr: Mutex::new(MMRFoundation::new()),
        }
    }

    pub fn set_initial_leaves(&self, initial_leaves: Vec<String>) {
        let mut leaves = self.leaves.lock().unwrap();
        let mut mmr = self.mmr.lock().unwrap();
        *leaves = initial_leaves;
        *mmr = MMRFoundation::new();
        for leaf in leaves.iter() {
            mmr.add_leaf(leaf.as_bytes());
        }
    }

    pub fn update_state(&self, tx_id: &str, _height: u64) {
        let mut leaves = self.leaves.lock().unwrap();
        let mut mmr = self.mmr.lock().unwrap();
        leaves.push(tx_id.to_string());
        mmr.add_leaf(tx_id.as_bytes());
    }

    pub fn update_state_batch(&self, tx_ids: &[String]) -> Vec<(u64, [u8; 32])> {
        let mut leaves = self.leaves.lock().unwrap();
        let mut mmr = self.mmr.lock().unwrap();
        let mut all_added_nodes = Vec::new();

        for tx_id in tx_ids {
            leaves.push(tx_id.clone());
            let added = mmr.add_leaf(tx_id.as_bytes());
            all_added_nodes.extend(added);
        }

        all_added_nodes
    }

    pub fn get_state_root(&self) -> String {
        let leaves = self.leaves.lock().unwrap();
        if leaves.is_empty() {
            return "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string();
        }

        // Optimized Merkle Root calculation
        let mut current_level: Vec<[u8; 32]> = leaves
            .iter()
            .map(|l| {
                let mut hasher = Sha256::new();
                hasher.update(l.as_bytes());
                hasher.finalize().into()
            })
            .collect();

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for i in (0..current_level.len()).step_by(2) {
                let mut hasher = Sha256::new();
                hasher.update(current_level[i]);
                if i + 1 < current_level.len() {
                    hasher.update(current_level[i + 1]);
                } else {
                    // Merkle hardening: if odd number of leaves, hash the last one with itself
                    hasher.update(current_level[i]);
                }
                next_level.push(hasher.finalize().into());
            }
            current_level = next_level;
        }

        format!("0x{}", hex::encode(current_level[0]))
    }

    pub fn get_mmr_root(&self) -> String {
        self.mmr.lock().unwrap().get_root()
    }

    pub fn generate_proof(&self, key: &str) -> (String, String) {
        if let Some(proof) = self.generate_merkle_proof(key) {
            (proof.root.clone(), serde_json::to_string(&proof).unwrap_or_default())
        } else {
            (self.get_state_root(), "{}".to_string())
        }
    }

    pub fn generate_merkle_proof(&self, key: &str) -> Option<MerkleProof> {
        let leaves = self.leaves.lock().unwrap();
        let index = leaves.iter().position(|l| l == key)?;

        let mut proof_path = Vec::new();
        let mut current_level: Vec<[u8; 32]> = leaves
            .iter()
            .map(|l| {
                let mut hasher = Sha256::new();
                hasher.update(l.as_bytes());
                hasher.finalize().into()
            })
            .collect();

        let mut current_index = index;
        while current_level.len() > 1 {
            let is_left = current_index % 2 == 0;
            let sibling_index = if is_left {
                current_index + 1
            } else {
                current_index - 1
            };

            if sibling_index < current_level.len() {
                proof_path.push((format!("0x{}", hex::encode(current_level[sibling_index])), is_left));
            } else {
                // If odd, sibling is same as current (hardened)
                proof_path.push((format!("0x{}", hex::encode(current_level[current_index])), is_left));
            }

            let mut next_level = Vec::new();
            for i in (0..current_level.len()).step_by(2) {
                let mut hasher = Sha256::new();
                hasher.update(current_level[i]);
                if i + 1 < current_level.len() {
                    hasher.update(current_level[i + 1]);
                } else {
                    hasher.update(current_level[i]);
                }
                next_level.push(hasher.finalize().into());
            }
            current_level = next_level;
            current_index /= 2;
        }

        Some(MerkleProof {
            leaf: key.to_string(),
            root: format!("0x{}", hex::encode(current_level[0])),
            path: proof_path,
        })
    }

    pub fn get_leaf_index(&self, tx_id: &str) -> Option<usize> {
        self.leaves.lock().unwrap().iter().position(|l| l == tx_id)
    }

    pub fn get_leaf_by_index(&self, index: usize) -> Option<String> {
        self.leaves.lock().unwrap().get(index).cloned()
    }

    pub fn get_mmr_proof_metadata(&self, leaf_index: usize) -> Option<(u64, Vec<u64>)> {
        let mmr = self.mmr.lock().unwrap();
        if leaf_index >= mmr.size {
            return None;
        }

        let pos = get_mmr_node_pos(leaf_index as u64);
        let path = get_mmr_path(pos, mmr.size as u64);
        Some((pos, path))
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

pub(crate) fn get_mmr_node_pos(leaf_index: u64) -> u64 {
    leaf_index * 2 - leaf_index.count_ones() as u64
}

fn get_peaks_metadata(mut leaf_count: u64) -> Vec<(u64, u32, u64)> {
    let mut peaks = Vec::new();
    let mut offset = 0;
    while leaf_count > 0 {
        let h = 63 - leaf_count.leading_zeros();
        let size = (1u64 << (h + 1)) - 1;
        peaks.push((offset + size - 1, h, offset));
        offset += size;
        leaf_count -= 1u64 << h;
    }
    peaks
}

pub(crate) fn get_mmr_path(pos: u64, leaf_count: u64) -> Vec<u64> {
    let peaks = get_peaks_metadata(leaf_count);
    let mut path = Vec::new();
    for (p_pos, p_h, _p_start) in peaks {
        if pos <= p_pos {
            let mut curr_p = p_pos;
            let mut curr_h = p_h;
            while curr_p > pos {
                let left_child = curr_p - (1u64 << curr_h);
                let right_child = curr_p - 1;
                curr_h -= 1;
                if pos <= left_child {
                    path.push(right_child);
                    curr_p = left_child;
                } else {
                    path.push(left_child);
                    curr_p = right_child;
                }
            }
            break;
        }
    }
    path.reverse();
    path
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

pub struct MMRFoundation {
    pub peaks: Vec<[u8; 32]>,
    pub size: usize,
    pub node_count: u64,
}

impl Default for MMRFoundation {
    fn default() -> Self {
        Self::new()
    }
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
        let leaf_pos = get_mmr_node_pos(self.size as u64);
        added_nodes.push((leaf_pos, current_hash));
        self.node_count = leaf_pos + 1;

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

    fn assert_mmr_metadata(
        state: &NexusState,
        leaf_index: usize,
        expected_pos: u64,
        expected_sibs: &[u64],
    ) {
        let (pos, sibs) = state.get_mmr_proof_metadata(leaf_index).unwrap();
        assert_eq!(pos, expected_pos);
        assert_eq!(sibs.as_slice(), expected_sibs);
    }

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
    fn test_mmr_metadata_calculation_with_tree_size() {
        let state = NexusState::new();

        // Size 1
        state.update_state_batch(&["a".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[]);

        // Size 2
        state.update_state_batch(&["b".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[1]);
        assert_mmr_metadata(&state, 1, 1, &[0]);

        // Size 3
        state.update_state_batch(&["c".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[1]);
        assert_mmr_metadata(&state, 1, 1, &[0]);
        assert_mmr_metadata(&state, 2, 3, &[]);

        // Size 4
        state.update_state_batch(&["d".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[1, 5]);
        assert_mmr_metadata(&state, 1, 1, &[0, 5]);
        assert_mmr_metadata(&state, 2, 3, &[4, 2]);
        assert_mmr_metadata(&state, 3, 4, &[3, 2]);

        // Size 5
        state.update_state_batch(&["e".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[1, 5]);
        assert_mmr_metadata(&state, 1, 1, &[0, 5]);
        assert_mmr_metadata(&state, 2, 3, &[4, 2]);
        assert_mmr_metadata(&state, 3, 4, &[3, 2]);
        assert_mmr_metadata(&state, 4, 7, &[]);

        // Size 6
        state.update_state_batch(&["f".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[1, 5]);
        assert_mmr_metadata(&state, 1, 1, &[0, 5]);
        assert_mmr_metadata(&state, 2, 3, &[4, 2]);
        assert_mmr_metadata(&state, 3, 4, &[3, 2]);
        assert_mmr_metadata(&state, 4, 7, &[8]);
        assert_mmr_metadata(&state, 5, 8, &[7]);

        // Size 7
        state.update_state_batch(&["g".to_string()]);
        assert_mmr_metadata(&state, 0, 0, &[1, 5]);
        assert_mmr_metadata(&state, 1, 1, &[0, 5]);
        assert_mmr_metadata(&state, 2, 3, &[4, 2]);
        assert_mmr_metadata(&state, 3, 4, &[3, 2]);
        assert_mmr_metadata(&state, 4, 7, &[8]);
        assert_mmr_metadata(&state, 5, 8, &[7]);
        assert_mmr_metadata(&state, 6, 10, &[]);
    }

    #[test]
    fn test_mmr_metadata_calculation_with_batched_update_state_batch() {
        let state = NexusState::new();

        state.update_state_batch(&[
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]);

        assert_mmr_metadata(&state, 0, 0, &[1, 5]);
        assert_mmr_metadata(&state, 1, 1, &[0, 5]);
        assert_mmr_metadata(&state, 2, 3, &[4, 2]);
        assert_mmr_metadata(&state, 3, 4, &[3, 2]);
    }

    #[test]
    fn test_mmr_metadata_leaf_index_out_of_bounds() {
        let state = NexusState::new();
        state.update_state_batch(&["a".to_string()]);

        assert_eq!(state.get_mmr_proof_metadata(1), None);
    }

    #[test]
    fn test_mmr_metadata_some_for_all_valid_indices() {
        let state = NexusState::new();
        state.update_state_batch(&[
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ]);

        let leaves_len = state.leaves.lock().unwrap().len();
        for idx in 0..leaves_len {
            assert!(
                state.get_mmr_proof_metadata(idx).is_some(),
                "expected metadata for leaf index {}",
                idx,
            );
        }
    }
}
