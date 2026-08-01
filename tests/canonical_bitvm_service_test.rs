use ark_bn254::{Bn254, Fr};
use ark_crypto_primitives::snark::{CircuitSpecificSetupSNARK, SNARK};
use ark_ff::PrimeField;
use ark_groth16::Groth16;
use ark_relations::{
    gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError, Variable},
    lc,
};
use ark_serialize::CanonicalSerialize;
use ark_std::rand::SeedableRng;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::{
    BitvmGroth16TrustedRecordConfig, BitvmGroth16TrustedRegistryConfig, Config,
    NEXUS_PUBLIC_INPUT_LAYOUT_V1,
};
use conxian_nexus::executor::bitvm_groth16::{
    derive_public_inputs, statement_hash, BitcoinBlockContext, BitcoinNetwork,
    CanonicalStateTransitionVerifier, GatewayGroth16Envelope, Groth16Curve, NexusStateTransition,
    PublicInputLayout, TrustedVerificationKeyConfig, TrustedVerificationKeyRegistry,
    VerificationKeyId, GROTH16_SCHEMA_VERSION, NEXUS_STATE_TRANSITION_CIRCUIT_ID,
    NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
};
use conxian_nexus::executor::canonical_bitvm::{
    AuditPersistOutcome, CanonicalBitvmAuditError, CanonicalBitvmAuditRecord,
    CanonicalBitvmReceiptStore, CanonicalBitvmService, CanonicalBitvmServiceError,
    TrustedBitcoinHeightProvider, TrustedHeightError,
};
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use futures_util::future::BoxFuture;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

/// Fixture-only equality circuit. It validates the boundary, not production
/// state-transition semantics, and is never loaded by runtime configuration.
#[derive(Clone)]
struct PublicInputEqualityCircuit {
    values: [Fr; NEXUS_STATE_TRANSITION_PUBLIC_INPUTS],
}

impl ConstraintSynthesizer<Fr> for PublicInputEqualityCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        for value in self.values {
            let public = cs.new_input_variable(|| Ok(value))?;
            let witness = cs.new_witness_variable(|| Ok(value))?;
            cs.enforce_r1cs_constraint(|| lc![public], || lc![Variable::One], || lc![witness])?;
        }
        Ok(())
    }
}

struct Fixture {
    transition: NexusStateTransition,
    envelope: GatewayGroth16Envelope,
    vk_config: TrustedVerificationKeyConfig,
}

fn fixture() -> Fixture {
    let transition = NexusStateTransition {
        prev_state_root: [0x11; 32],
        next_state_root: [0x22; 32],
    };
    let block_context = BitcoinBlockContext {
        network: BitcoinNetwork::Regtest,
        block_height: 840_000,
        block_hash: [0x33; 32],
        max_valid_height: Some(840_144),
    };
    let witness_commitment = [0x44; 32];
    let public_inputs = derive_public_inputs(&transition, &block_context, witness_commitment)
        .unwrap()
        .to_vec();
    let values: [Fr; NEXUS_STATE_TRANSITION_PUBLIC_INPUTS] = public_inputs
        .iter()
        .map(|value| Fr::from_be_bytes_mod_order(value.as_bytes()))
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let mut rng = ark_std::rand::rngs::StdRng::seed_from_u64(0x0001_6902);
    let (pk, vk) = Groth16::<Bn254>::setup(
        PublicInputEqualityCircuit {
            values: [Fr::from(0u64); NEXUS_STATE_TRANSITION_PUBLIC_INPUTS],
        },
        &mut rng,
    )
    .unwrap();
    let proof =
        Groth16::<Bn254>::prove(&pk, PublicInputEqualityCircuit { values }, &mut rng).unwrap();
    let mut vk_bytes = Vec::new();
    vk.serialize_compressed(&mut vk_bytes).unwrap();
    let verification_key_id = VerificationKeyId::from_key_bytes(&vk_bytes).unwrap();
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes).unwrap();
    let mut envelope = GatewayGroth16Envelope {
        schema_version: GROTH16_SCHEMA_VERSION,
        curve: Groth16Curve::Bn254,
        circuit_id: NEXUS_STATE_TRANSITION_CIRCUIT_ID.to_owned(),
        verification_key_id,
        public_inputs,
        witness_commitment,
        block_context,
        proof: proof_bytes,
        statement_hash: [0; 32],
    };
    envelope.statement_hash = statement_hash(&envelope).unwrap();
    Fixture {
        transition,
        envelope,
        vk_config: TrustedVerificationKeyConfig {
            schema_version: GROTH16_SCHEMA_VERSION,
            curve: Groth16Curve::Bn254,
            circuit_id: NEXUS_STATE_TRANSITION_CIRCUIT_ID.to_owned(),
            verification_key_id,
            public_input_count: NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
            public_input_layout: PublicInputLayout::NexusStateTransitionV1,
            enabled: true,
            verification_key_bytes: vk_bytes,
        },
    }
}

fn verifier(config: TrustedVerificationKeyConfig) -> Arc<CanonicalStateTransitionVerifier> {
    let mut registry = TrustedVerificationKeyRegistry::default();
    registry.register(config).unwrap();
    Arc::new(CanonicalStateTransitionVerifier::new(Arc::new(registry)))
}

struct FixedHeight(Result<u64, TrustedHeightError>);

impl TrustedBitcoinHeightProvider for FixedHeight {
    fn current_height(&self) -> BoxFuture<'_, Result<u64, TrustedHeightError>> {
        Box::pin(async { self.0.clone() })
    }
}

#[derive(Default)]
struct MemoryStore {
    records: Mutex<HashMap<String, CanonicalBitvmAuditRecord>>,
    fail: bool,
    conflict: bool,
}

impl CanonicalBitvmReceiptStore for MemoryStore {
    fn persist_immutable(
        &self,
        record: CanonicalBitvmAuditRecord,
    ) -> BoxFuture<'_, Result<AuditPersistOutcome, CanonicalBitvmAuditError>> {
        Box::pin(async move {
            if self.fail {
                return Err(CanonicalBitvmAuditError::Unavailable);
            }
            if self.conflict {
                return Err(CanonicalBitvmAuditError::IntegrityConflict);
            }
            let mut records = self.records.lock().unwrap();
            if let Some(existing) = records.get(&record.receipt_id) {
                let mut existing = existing.clone();
                let candidate = record;
                existing.verified_at = candidate.verified_at;
                if existing == candidate {
                    Ok(AuditPersistOutcome::Existing)
                } else {
                    Err(CanonicalBitvmAuditError::IntegrityConflict)
                }
            } else {
                records.insert(record.receipt_id.clone(), record);
                Ok(AuditPersistOutcome::Created)
            }
        })
    }
}

fn service(
    fixture: &Fixture,
    height: Result<u64, TrustedHeightError>,
    store: Arc<dyn CanonicalBitvmReceiptStore>,
) -> Arc<CanonicalBitvmService> {
    Arc::new(CanonicalBitvmService::new(
        verifier(fixture.vk_config.clone()),
        BitcoinNetwork::Regtest,
        Arc::new(FixedHeight(height)),
        store,
    ))
}

fn envelope_json(envelope: &GatewayGroth16Envelope) -> Value {
    json!({
        "schema_version": envelope.schema_version,
        "curve": "bn254",
        "circuit_id": envelope.circuit_id,
        "verification_key_id": hex::encode(envelope.verification_key_id.0),
        "public_inputs": envelope.public_inputs.iter().map(|value| hex::encode(value.as_bytes())).collect::<Vec<_>>(),
        "witness_commitment": hex::encode(envelope.witness_commitment),
        "block_context": {
            "network": "regtest",
            "block_height": envelope.block_context.block_height,
            "block_hash": hex::encode(envelope.block_context.block_hash),
            "max_valid_height": envelope.block_context.max_valid_height,
        },
        "proof": hex::encode(&envelope.proof),
        "statement_hash": hex::encode(envelope.statement_hash),
    })
}

fn registry_config(fixture: &Fixture) -> BitvmGroth16TrustedRegistryConfig {
    BitvmGroth16TrustedRegistryConfig {
        expected_bitcoin_network: "regtest".to_owned(),
        records: vec![BitvmGroth16TrustedRecordConfig {
            schema_version: GROTH16_SCHEMA_VERSION,
            curve: "bn254".to_owned(),
            circuit_id: NEXUS_STATE_TRANSITION_CIRCUIT_ID.to_owned(),
            verification_key_id: hex::encode(fixture.vk_config.verification_key_id.0),
            public_input_count: NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
            public_input_layout: NEXUS_PUBLIC_INPUT_LAYOUT_V1.to_owned(),
            enabled: true,
            verification_key_base64: BASE64_STANDARD
                .encode(&fixture.vk_config.verification_key_bytes),
        }],
    }
}

fn test_app(canonical_service: Option<Arc<CanonicalBitvmService>>) -> axum::Router {
    let config = Arc::new(Config::default_test());
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let mut executor =
        NexusExecutor::new(storage.clone(), RGBRolloutMode::Disabled, HashSet::new());
    if let Some(service) = canonical_service {
        executor = executor.with_canonical_bitvm_service(service);
    }
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));
    app_router(
        storage,
        Arc::new(NexusState::new()),
        Arc::new(executor),
        None,
        tableland,
        None,
        None,
        config,
    )
}

#[test]
fn runtime_registry_config_builds_and_rejects_bad_material() {
    let fixture = fixture();
    let (network, _) = registry_config(&fixture).build_registry().unwrap();
    assert_eq!(network, BitcoinNetwork::Regtest);

    let mut unknown_field = serde_json::to_value(registry_config(&fixture)).unwrap();
    unknown_field["unexpected"] = json!(true);
    assert!(serde_json::from_value::<BitvmGroth16TrustedRegistryConfig>(unknown_field).is_err());

    let mut bad_base64 = registry_config(&fixture);
    bad_base64.records[0].verification_key_base64 = "***".to_owned();
    assert!(bad_base64.build_registry().is_err());

    let mut uppercase_id = registry_config(&fixture);
    uppercase_id.records[0].verification_key_id =
        uppercase_id.records[0].verification_key_id.to_uppercase();
    assert!(uppercase_id.build_registry().is_err());

    let mut conflict = registry_config(&fixture);
    let mut second = conflict.records[0].clone();
    second.enabled = false;
    conflict.records.push(second);
    assert!(conflict.build_registry().is_err());
}

#[tokio::test]
async fn service_persists_before_success_and_exact_retry_is_idempotent() {
    let fixture = fixture();
    let store = Arc::new(MemoryStore::default());
    let service = service(&fixture, Ok(840_100), store.clone());
    let first = service
        .verify_and_persist(&fixture.transition, &fixture.envelope)
        .await
        .unwrap();
    let second = service
        .verify_and_persist(&fixture.transition, &fixture.envelope)
        .await
        .unwrap();
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.receipt_id, second.receipt_id);
    assert_eq!(store.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn service_fails_closed_for_audit_network_and_height_errors() {
    let fixture = fixture();
    let failing = Arc::new(MemoryStore {
        fail: true,
        ..MemoryStore::default()
    });
    assert!(matches!(
        service(&fixture, Ok(840_100), failing)
            .verify_and_persist(&fixture.transition, &fixture.envelope)
            .await,
        Err(CanonicalBitvmServiceError::Audit(
            CanonicalBitvmAuditError::Unavailable
        ))
    ));
    let conflict = Arc::new(MemoryStore {
        conflict: true,
        ..MemoryStore::default()
    });
    assert!(matches!(
        service(&fixture, Ok(840_100), conflict)
            .verify_and_persist(&fixture.transition, &fixture.envelope)
            .await,
        Err(CanonicalBitvmServiceError::Audit(
            CanonicalBitvmAuditError::IntegrityConflict
        ))
    ));
    assert!(matches!(
        service(
            &fixture,
            Err(TrustedHeightError::Unavailable),
            Arc::new(MemoryStore::default())
        )
        .verify_and_persist(&fixture.transition, &fixture.envelope)
        .await,
        Err(CanonicalBitvmServiceError::TrustedHeight(
            TrustedHeightError::Unavailable
        ))
    ));

    let mut wrong_network = fixture.envelope.clone();
    wrong_network.block_context.network = BitcoinNetwork::Mainnet;
    assert!(matches!(
        service(&fixture, Ok(840_100), Arc::new(MemoryStore::default()))
            .verify_and_persist(&fixture.transition, &wrong_network)
            .await,
        Err(CanonicalBitvmServiceError::NetworkMismatch { .. })
    ));
}

#[tokio::test]
async fn canonical_http_success_unavailable_and_body_limit_are_typed() {
    let fixture = fixture();
    let payload = json!({
        "previous_state_root": hex::encode(fixture.transition.prev_state_root),
        "next_state_root": hex::encode(fixture.transition.next_state_root),
        "envelope": envelope_json(&fixture.envelope),
    });
    let response = test_app(Some(service(
        &fixture,
        Ok(840_100),
        Arc::new(MemoryStore::default()),
    )))
    .oneshot(
        Request::builder()
            .method("POST")
            .uri("/v1/bitvm2/verify-state-transition")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "verified");
    assert!(body.get("confidence").is_none());
    assert!(body.get("proof").is_none());

    let unavailable = test_app(None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bitvm2/verify-state-transition")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unavailable.status(), StatusCode::NOT_IMPLEMENTED);

    let oversized = test_app(None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/bitvm2/verify-state-transition")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    "{{\"previous_state_root\":\"{}\",\"next_state_root\":\"{}\",\"envelope\":{{\"padding\":\"{}\"}}}}",
                    "00".repeat(32),
                    "00".repeat(32),
                    "x".repeat(17 * 1024)
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let oversized_body: Value =
        serde_json::from_slice(&to_bytes(oversized.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    assert_eq!(oversized_body["error"]["code"], "malformed_payload");
    assert_eq!(
        oversized_body["error"]["message"],
        "request payload is malformed"
    );
}
