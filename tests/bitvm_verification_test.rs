use ark_bn254::Fr;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use conxian_nexus::config::BitVmVerificationKeyConfig;
use conxian_nexus::executor::bitvm::{
    canonical_statement_hash, verification_key_id, ArkworksBn254Verifier, BitVMAdapter,
    BitVMAuditRecord, BitVMAuditSink, BitVMError, BitVMTransition, Bn254Verifier,
    RegisteredVerificationKey, VerificationKeyRegistry, BN254_SCALAR_MODULUS,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct Fixture {
    registry: Vec<BitVmVerificationKeyConfig>,
    request: BitVMTransition,
    adversarial_wrong_proof: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/bitvm_groth16_v1.json")).unwrap()
}

#[derive(Default)]
struct MemoryAudit {
    records: Mutex<Vec<BitVMAuditRecord>>,
    fail: bool,
}

#[async_trait]
impl BitVMAuditSink for MemoryAudit {
    async fn persist(&self, record: &BitVMAuditRecord) -> Result<(), BitVMError> {
        if self.fail {
            return Err(BitVMError::AuditPersistence);
        }
        self.records.lock().unwrap().push(record.clone());
        Ok(())
    }
}

struct AcceptingVerifier;

#[async_trait]
impl Bn254Verifier for AcceptingVerifier {
    async fn verify(
        &self,
        _key: &RegisteredVerificationKey,
        _public_inputs: &[Fr],
        _proof_bytes: &[u8],
    ) -> Result<bool, BitVMError> {
        Ok(true)
    }
}

fn registry(entries: &[BitVmVerificationKeyConfig]) -> VerificationKeyRegistry {
    VerificationKeyRegistry::from_config(entries).unwrap()
}

fn adapter(
    fixture: &Fixture,
    verifier: Arc<dyn Bn254Verifier>,
    audit: Arc<dyn BitVMAuditSink>,
) -> BitVMAdapter {
    BitVMAdapter::with_components(registry(&fixture.registry), verifier, audit)
}

fn refresh_statement_hash(request: &mut BitVMTransition) {
    let vk_id: [u8; 32] = hex::decode(&request.verification_key_id)
        .unwrap()
        .try_into()
        .unwrap();
    let inputs: [[u8; 32]; 7] = request
        .public_inputs
        .iter()
        .map(|value| hex::decode(value).unwrap().try_into().unwrap())
        .collect::<Vec<[u8; 32]>>()
        .try_into()
        .unwrap();
    let witness: [u8; 32] = hex::decode(&request.witness_commitment)
        .unwrap()
        .try_into()
        .unwrap();
    request.statement_hash = hex::encode(
        canonical_statement_hash(vk_id, &inputs, witness, &request.block_context).unwrap(),
    );
}

#[tokio::test]
async fn deterministic_fixture_verifies_and_audits_before_success() {
    let fixture = fixture();
    let audit = Arc::new(MemoryAudit::default());
    let adapter = adapter(&fixture, Arc::new(ArkworksBn254Verifier), audit.clone());
    let result = adapter.verify_transition(&fixture.request).await.unwrap();
    assert!(result.valid);
    assert_eq!(result.steps_verified, 4242);
    assert_eq!(result.statement_hash, fixture.request.statement_hash);
    assert_eq!(audit.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn deterministic_well_formed_wrong_proof_is_422() {
    let mut fixture = fixture();
    fixture.request.proof = fixture.adversarial_wrong_proof.clone();
    let adapter = adapter(
        &fixture,
        Arc::new(ArkworksBn254Verifier),
        Arc::new(MemoryAudit::default()),
    );
    let error = adapter
        .verify_transition(&fixture.request)
        .await
        .unwrap_err();
    assert!(matches!(error, BitVMError::InvalidProof));
    assert_eq!(
        error.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn roots_steps_witness_and_public_input_binding_mutations_are_rejected() {
    let fixture = fixture();
    let cases = [
        {
            let mut request = fixture.request.clone();
            request.prev_state_root.replace_range(2..3, "f");
            request
        },
        {
            let mut request = fixture.request.clone();
            request.next_state_root.replace_range(2..3, "0");
            request
        },
        {
            let mut request = fixture.request.clone();
            request.steps_verified += 1;
            request
        },
        {
            let mut request = fixture.request.clone();
            request.witness_commitment.replace_range(0..1, "f");
            request
        },
        {
            let mut request = fixture.request.clone();
            request.public_inputs.swap(0, 1);
            request
        },
        {
            let mut request = fixture.request.clone();
            request.public_inputs[4].replace_range(63..64, "3");
            request
        },
    ];
    let adapter = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit::default()),
    );
    for request in cases {
        assert!(matches!(
            adapter.verify_transition(&request).await.unwrap_err(),
            BitVMError::Binding(_)
        ));
    }
}

#[tokio::test]
async fn public_input_count_and_noncanonical_scalar_are_rejected() {
    let fixture = fixture();
    let adapter = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit::default()),
    );
    let mut count = fixture.request.clone();
    count.public_inputs.pop();
    assert!(matches!(
        adapter.verify_transition(&count).await.unwrap_err(),
        BitVMError::Binding(_)
    ));

    let mut scalar = fixture.request.clone();
    scalar.public_inputs[0] = hex::encode(BN254_SCALAR_MODULUS);
    assert!(matches!(
        adapter.verify_transition(&scalar).await.unwrap_err(),
        BitVMError::Malformed(_)
    ));
}

#[tokio::test]
async fn values_outside_postgres_bigint_audit_range_are_rejected() {
    let fixture = fixture();
    let adapter = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit::default()),
    );
    let mut steps = fixture.request.clone();
    steps.steps_verified = i64::MAX as u64 + 1;
    assert!(matches!(
        adapter.verify_transition(&steps).await.unwrap_err(),
        BitVMError::Malformed(_)
    ));

    let mut anchor = fixture.request.clone();
    anchor.block_context.block_height = i64::MAX as u64 + 1;
    anchor.block_context.max_valid_height = None;
    assert!(matches!(
        adapter.verify_transition(&anchor).await.unwrap_err(),
        BitVMError::Malformed(_)
    ));

    let mut expiry = fixture.request.clone();
    expiry.block_context.max_valid_height = Some(i64::MAX as u64 + 1);
    assert!(matches!(
        adapter.verify_transition(&expiry).await.unwrap_err(),
        BitVMError::Malformed(_)
    ));
}

#[tokio::test]
async fn wrong_unknown_or_disabled_key_fails_closed_with_503() {
    let mut fixture = fixture();
    let mut wrong = fixture.request.clone();
    wrong.verification_key_id = "aa".repeat(32);
    refresh_statement_hash(&mut wrong);
    let active_adapter = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit::default()),
    );
    let error = active_adapter.verify_transition(&wrong).await.unwrap_err();
    assert!(matches!(error, BitVMError::VerificationKeyUnavailable));
    assert_eq!(
        error.status_code(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );

    fixture.registry[0].enabled = false;
    let disabled = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit::default()),
    );
    assert!(matches!(
        disabled
            .verify_transition(&fixture.request)
            .await
            .unwrap_err(),
        BitVMError::VerificationKeyUnavailable
    ));
}

#[tokio::test]
async fn wrong_schema_curve_circuit_and_statement_hash_are_400() {
    let fixture = fixture();
    let adapter = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit::default()),
    );
    let mut requests = Vec::new();
    let mut schema = fixture.request.clone();
    schema.schema_version = 2;
    requests.push(schema);
    let mut curve = fixture.request.clone();
    curve.curve = "bls12-381".to_string();
    requests.push(curve);
    let mut circuit = fixture.request.clone();
    circuit.circuit_id = "wrong-circuit".to_string();
    requests.push(circuit);
    let mut hash = fixture.request.clone();
    hash.statement_hash = "00".repeat(32);
    requests.push(hash);

    for request in requests {
        let error = adapter.verify_transition(&request).await.unwrap_err();
        assert_eq!(error.status_code(), axum::http::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn malformed_and_trailing_proof_encodings_are_400() {
    let fixture = fixture();
    let adapter = adapter(
        &fixture,
        Arc::new(ArkworksBn254Verifier),
        Arc::new(MemoryAudit::default()),
    );
    let mut short = fixture.request.clone();
    short.proof.truncate(short.proof.len() - 2);
    assert_eq!(
        adapter
            .verify_transition(&short)
            .await
            .unwrap_err()
            .status_code(),
        axum::http::StatusCode::BAD_REQUEST
    );
    let mut trailing = fixture.request.clone();
    trailing.proof.push_str("00");
    assert_eq!(
        adapter
            .verify_transition(&trailing)
            .await
            .unwrap_err()
            .status_code(),
        axum::http::StatusCode::BAD_REQUEST
    );
    let mut malformed = fixture.request.clone();
    malformed.proof = "ff".repeat(128);
    assert!(matches!(
        adapter.verify_transition(&malformed).await.unwrap_err(),
        BitVMError::MalformedProof(_)
    ));
}

#[tokio::test]
async fn audit_failure_is_500_and_never_returns_valid_true() {
    let fixture = fixture();
    let adapter = adapter(
        &fixture,
        Arc::new(AcceptingVerifier),
        Arc::new(MemoryAudit {
            records: Mutex::new(Vec::new()),
            fail: true,
        }),
    );
    let error = adapter
        .verify_transition(&fixture.request)
        .await
        .unwrap_err();
    assert!(matches!(error, BitVMError::AuditPersistence));
    assert_eq!(
        error.status_code(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn registry_rejects_wrong_id_circuit_duplicates_and_trailing_vk_bytes() {
    let fixture = fixture();
    let mut wrong_id = fixture.registry.clone();
    wrong_id[0].verification_key_id = "aa".repeat(32);
    assert!(VerificationKeyRegistry::from_config(&wrong_id).is_err());

    let mut wrong_circuit = fixture.registry.clone();
    wrong_circuit[0].circuit_id = "wrong-circuit".to_string();
    assert!(VerificationKeyRegistry::from_config(&wrong_circuit).is_err());

    let duplicates = vec![fixture.registry[0].clone(), fixture.registry[0].clone()];
    assert!(VerificationKeyRegistry::from_config(&duplicates).is_err());

    let mut trailing = fixture.registry.clone();
    let mut bytes = BASE64_STANDARD
        .decode(&trailing[0].verification_key_base64)
        .unwrap();
    bytes.push(0);
    trailing[0].verification_key_id = hex::encode(verification_key_id(&bytes));
    trailing[0].verification_key_base64 = BASE64_STANDARD.encode(bytes);
    assert!(VerificationKeyRegistry::from_config(&trailing).is_err());
}
