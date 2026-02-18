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

    let (root, proof) = state.generate_proof("user_key");
    assert_eq!(root, new_root);
    assert!(proof.starts_with("0x"));
}
