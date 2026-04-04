use conxian_nexus::state::NexusState;
use std::sync::Arc;

#[tokio::test]
async fn test_state_transition_flow() {
    let state = Arc::new(NexusState::new());
    let initial_root = state.get_state_root();

    // Simulate updating state directly
    state.update_state("block1", 5);
    let new_root = state.get_state_root();

    assert_ne!(initial_root, new_root);

    let (root, proof_json) = state.generate_proof("block1");
    assert_eq!(root, new_root);
    // proof_json should be a valid JSON representation of MerkleProof
    let proof: conxian_nexus::state::MerkleProof =
        serde_json::from_str(&proof_json).expect("Proof should be valid JSON");
    assert_eq!(proof.leaf, "block1");
}

#[tokio::test]
async fn test_merkle_proof_verification() {
    let state = Arc::new(NexusState::new());

    state.update_state("tx1", 1);
    state.update_state("tx2", 1);
    state.update_state("tx3", 1);
    state.update_state("tx4", 1);

    let proof = state
        .generate_merkle_proof("tx2")
        .expect("Proof should be generated");

    assert_eq!(proof.leaf, "tx2");
    assert!(conxian_nexus::state::verify_merkle_proof(&proof));

    // Test with invalid root
    let mut invalid_proof = proof.clone();
    invalid_proof.root = "0x0000".to_string();
    assert!(!conxian_nexus::state::verify_merkle_proof(&invalid_proof));
}

#[tokio::test]
async fn test_leaf_to_root_verification() {
    let state = Arc::new(NexusState::new());
    let leaves = vec!["a", "b", "c", "d", "e"];
    for leaf in &leaves {
        state.update_state(leaf, 1);
    }

    for leaf in &leaves {
        let proof = state.generate_merkle_proof(leaf).expect("Proof exists");
        assert!(
            conxian_nexus::state::verify_merkle_proof(&proof),
            "Failed to verify leaf: {}",
            leaf
        );
    }
}

#[tokio::test]
async fn test_root_to_leaf_consistency() {
    let state = Arc::new(NexusState::new());
    state.update_state("data1", 1);
    let root1 = state.get_state_root();

    state.update_state("data2", 1);
    let root2 = state.get_state_root();

    assert_ne!(root1, root2);

    let proof1 = state.generate_merkle_proof("data1").unwrap();
    assert_eq!(proof1.root, root2); // Proof should be against the latest root
    assert!(conxian_nexus::state::verify_merkle_proof(&proof1));
}

#[tokio::test]
async fn test_mmr_proof_consistency() {
    let state = Arc::new(NexusState::new());
    let leaves = vec![
        "tx1".to_string(),
        "tx2".to_string(),
        "tx3".to_string(),
        "tx4".to_string(),
    ];
    state.update_state_batch(&leaves);

    // Leaf 0 in 4-leaf MMR (all in one tree, pos 6 is peak)
    let (pos, siblings) = state.get_mmr_proof_metadata(0).unwrap();
    assert_eq!(pos, 0);
    // For 4 leaves, nodes are 0,1->2, 3,4->5, 2,5->6.
    // Siblings for 0 are [1, 5].
    assert_eq!(siblings, vec![1, 5]);

    // Leaf 2 in 4-leaf MMR: pos 3, siblings [4, 2].
    let (pos2, siblings2) = state.get_mmr_proof_metadata(2).unwrap();
    assert_eq!(pos2, 3);
    assert_eq!(siblings2, vec![4, 2]);

    // Leaf 3 in 4-leaf MMR: pos 4, siblings [3, 2].
    let (pos3, siblings3) = state.get_mmr_proof_metadata(3).unwrap();
    assert_eq!(pos3, 4);
    assert_eq!(siblings3, vec![3, 2]);

    // Check with 3 leaves
    let state3 = Arc::new(NexusState::new());
    state3.update_state_batch(&vec!["a".into(), "b".into(), "c".into()]);
    // 0, 1 -> 2
    // 3 (leaf 2)
    // Peaks: 2, 3
    let (pos_peak, siblings_peak) = state3.get_mmr_proof_metadata(2).unwrap();
    assert_eq!(pos_peak, 3);
    assert!(siblings_peak.is_empty()); // It is a peak
}
