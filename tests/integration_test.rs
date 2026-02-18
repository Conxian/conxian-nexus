use std::sync::Arc;
use conxian_nexus::state::NexusState;

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
    let proof: conxian_nexus::state::MerkleProof = serde_json::from_str(&proof_json).expect("Proof should be valid JSON");
    assert_eq!(proof.leaf, "block1");
}

#[tokio::test]
async fn test_merkle_proof_verification() {
    let state = Arc::new(NexusState::new());

    state.update_state("tx1", 1);
    state.update_state("tx2", 1);
    state.update_state("tx3", 1);
    state.update_state("tx4", 1);

    let proof = state.generate_merkle_proof("tx2").expect("Proof should be generated");

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
        assert!(conxian_nexus::state::verify_merkle_proof(&proof), "Failed to verify leaf: {}", leaf);
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
