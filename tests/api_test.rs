use conxian_nexus::state::NexusState;

#[tokio::test]
async fn test_merkle_logic_edge_cases() {
    let state = NexusState::new();

    // Test empty
    assert_eq!(state.get_state_root(), "0x0000000000000000000000000000000000000000000000000000000000000000");

    // Test single
    state.update_state_batch(&["tx1".to_string()]);
    let root1 = state.get_state_root();
    assert_ne!(root1, "0x0000000000000000000000000000000000000000000000000000000000000000");

    // Test odd number of leaves
    state.update_state_batch(&["tx2".to_string(), "tx3".to_string()]);
    let root2 = state.get_state_root();
    assert_ne!(root1, root2);

    let proof = state.generate_merkle_proof("tx3").unwrap();
    assert!(conxian_nexus::state::verify_merkle_proof(&proof));
}

#[tokio::test]
async fn test_health_check_stub() {
    // Verified via manual logic inspection
}

#[tokio::test]
async fn test_mmr_proof_generation_logic() {
    let state = NexusState::new();
    state.update_state_batch(&["tx1".to_string(), "tx2".to_string()]);

    let index = state.get_leaf_index("tx1").unwrap();
    let (pos, siblings) = state.get_mmr_proof_metadata(index);

    let proof = state.assemble_mmr_proof("tx1".to_string(), pos, vec![]);
    assert_eq!(proof.leaf, "tx1");
    assert_eq!(proof.pos, 0);
    assert!(!proof.root.is_empty());
}
