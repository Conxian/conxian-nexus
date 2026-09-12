use conxian_nexus::orchestrator::roast::{RoastRoundStatus, RoastSession};
use conxian_nexus::verification::{
    FrostVerificationPayload, FrostVerifier, OpCatVerificationPayload, OpCatVerifier,
    ZkcpVerificationPayload, ZkcpVerifier,
};

#[test]
fn test_frost_verifier_success() {
    let payload = FrostVerificationPayload {
        message: "Test message for FROST threshold signature".to_string(),
        group_public_key: "021111111111111111111111111111111111111111111111111111111111111111".to_string(),
        threshold: 2,
        total_participants: 3,
        signature: "22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222".to_string(),
        participant_ids: vec![1, 2],
    };

    let res = FrostVerifier::verify(&payload);
    assert!(res.valid);
    assert!(res.error_message.is_none());
}

#[test]
fn test_frost_verifier_insufficient_shares() {
    let payload = FrostVerificationPayload {
        message: "Test message".to_string(),
        group_public_key: "021111111111111111111111111111111111111111111111111111111111111111".to_string(),
        threshold: 3,
        total_participants: 5,
        signature: "22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222".to_string(),
        participant_ids: vec![1, 2],
    };

    let res = FrostVerifier::verify(&payload);
    assert!(!res.valid);
    assert!(res
        .error_message
        .unwrap()
        .contains("Insufficient participant signature shares"));
}

#[test]
fn test_roast_orchestrator_flow() {
    let mut session = RoastSession::new("session_001".to_string(), 2, 3);
    assert_eq!(session.status, RoastRoundStatus::Initializing);

    let res1 = session.submit_share(1, vec![0xaa; 32]).unwrap();
    assert!(!res1);
    assert_eq!(session.status, RoastRoundStatus::CollectingSignatureShares);

    let res2 = session.submit_share(2, vec![0xbb; 32]).unwrap();
    assert!(res2);
    assert_eq!(session.status, RoastRoundStatus::Completed);
}

#[test]
fn test_zkcp_verifier() {
    let payload = ZkcpVerificationPayload {
        proof_hex: "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20".to_string(),
        public_inputs_hex: vec!["00".to_string()],
        expected_hash: "abcd".to_string(),
    };

    let res = ZkcpVerifier::verify(&payload);
    assert!(res.valid);
}

#[test]
fn test_op_cat_verifier() {
    let payload = OpCatVerificationPayload {
        script_elements_hex: vec!["0102".to_string(), "0304".to_string()],
        max_stack_size_bytes: 520,
        recursion_depth_limit: 16,
        target_state_hash: "hash".to_string(),
    };

    let res = OpCatVerifier::verify(&payload);
    assert!(res.valid);
    assert_eq!(res.concatenated_length, 4);
}
