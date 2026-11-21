//! Byte-parity with the enclave SDK `ReplayBinding`.
//!
//! The golden digest was produced by the real SDK
//! (`conxius-enclave-sdk::enclave::replay_guard::ReplayBinding` with the same
//! inputs). If this test fails, the Nexus binding has drifted from the SDK and a
//! Nexus record would no longer describe the same replay binding.

use conxian_nexus::storage::replay_binding::ReplayBinding;

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn matches_sdk_golden_digest() {
    let binding = ReplayBinding::builder()
        .provider("aws-nitro")
        .proof_subject("release-signer")
        .proof_mechanism("nitro-attestation")
        .nonce(&[1u8; 32])
        .operation_digest([2u8; 32])
        .purpose("release")
        .policy_digest([3u8; 32])
        .key_identity(&[4u8; 32])
        .evidence_digest([5u8; 32])
        .proof_id("proof-1")
        .build()
        .expect("binding builds");

    assert_eq!(
        hex(binding.digest().expect("digest")),
        "89c669a4313f9a21fafad5f11447c994be94f7164450212420c2f41f64c35809",
        "Nexus replay binding digest must match the enclave SDK"
    );
    assert_eq!(
        binding.encoded_key().expect("key"),
        "CONXIAN-REPLAY-BINDING/v1:89c669a4313f9a21fafad5f11447c994be94f7164450212420c2f41f64c35809"
    );
}
