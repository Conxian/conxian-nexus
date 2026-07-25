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
use conxian_nexus::executor::bitvm_groth16::{
    derive_public_inputs, parse_gateway_envelope_json, statement_hash, BitcoinBlockContext,
    BitcoinNetwork, CanonicalGroth16Error, CanonicalStateTransitionVerifier, FieldElement,
    GatewayGroth16Envelope, Groth16Curve, NexusStateTransition, PublicInputLayout,
    TrustedVerificationKeyConfig, TrustedVerificationKeyRegistry, VerificationKeyId,
    BN254_SCALAR_MODULUS, GROTH16_SCHEMA_VERSION, NEXUS_STATE_TRANSITION_CIRCUIT_ID,
    NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// Fixture-only circuit: each of the 12 public inputs is constrained equal to
/// a private witness copy. It tests the verifier boundary and does not claim
/// production state-transition semantics.
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
    let mut prev_state_root = [0x11; 32];
    prev_state_root[16..].fill(0x12);
    let mut next_state_root = [0x21; 32];
    next_state_root[16..].fill(0x22);
    let transition = NexusStateTransition {
        prev_state_root,
        next_state_root,
    };
    let block_context = BitcoinBlockContext {
        network: BitcoinNetwork::Regtest,
        block_height: 840_000,
        block_hash: [0x33; 32],
        max_valid_height: Some(840_144),
    };
    let witness_commitment = [0x44; 32];
    let public_inputs = derive_public_inputs(&transition, &block_context, witness_commitment)
        .expect("fixture inputs")
        .to_vec();
    let values: [Fr; NEXUS_STATE_TRANSITION_PUBLIC_INPUTS] = public_inputs
        .iter()
        .map(|value| Fr::from_be_bytes_mod_order(value.as_bytes()))
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    let mut rng = ark_std::rand::rngs::StdRng::seed_from_u64(0x169);
    let setup_circuit = PublicInputEqualityCircuit {
        values: [Fr::from(0u64); NEXUS_STATE_TRANSITION_PUBLIC_INPUTS],
    };
    let (pk, vk) = Groth16::<Bn254>::setup(setup_circuit, &mut rng).unwrap();
    let proof =
        Groth16::<Bn254>::prove(&pk, PublicInputEqualityCircuit { values }, &mut rng).unwrap();

    let mut vk_bytes = Vec::new();
    vk.serialize_compressed(&mut vk_bytes).unwrap();
    let verification_key_id = VerificationKeyId::from_key_bytes(&vk_bytes).unwrap();
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes).unwrap();
    assert_eq!(proof_bytes.len(), 128);

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

fn verifier(config: TrustedVerificationKeyConfig) -> CanonicalStateTransitionVerifier {
    let mut registry = TrustedVerificationKeyRegistry::default();
    registry.register(config).unwrap();
    CanonicalStateTransitionVerifier::new(Arc::new(registry))
}

fn envelope_json(envelope: &GatewayGroth16Envelope) -> Value {
    json!({
        "schema_version": envelope.schema_version,
        "curve": "bn254",
        "circuit_id": envelope.circuit_id,
        "verification_key_id": hex::encode(envelope.verification_key_id.0),
        "public_inputs": envelope.public_inputs.iter().map(|v| hex::encode(v.as_bytes())).collect::<Vec<_>>(),
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

#[test]
fn valid_fixture_verifies_all_twelve_constrained_inputs() {
    let fixture = fixture();
    let parsed = parse_gateway_envelope_json(envelope_json(&fixture.envelope)).unwrap();
    let receipt = verifier(fixture.vk_config)
        .verify(&fixture.transition, &parsed, 840_100)
        .unwrap();
    assert_eq!(receipt.statement_hash, fixture.envelope.statement_hash);
    assert_eq!(fixture.envelope.public_inputs.len(), 12);
}

#[test]
fn canonical_statement_hash_matches_gateway_schema_v1_vector() {
    let fixture = fixture();
    let mut envelope = fixture.envelope;
    envelope.verification_key_id = VerificationKeyId([0x55; 32]);
    envelope.statement_hash = [0; 32];
    assert_eq!(
        hex::encode(statement_hash(&envelope).unwrap()),
        "bbba23618d970a38070b2b5b58145cf5739f1794a68fe975aaacbc0897e5e6d3"
    );
}

#[test]
fn wrong_trusted_vk_and_circuit_associations_fail_closed() {
    let fixture = fixture();

    let mut wrong_association = fixture.vk_config.clone();
    wrong_association.circuit_id = "different-reviewed-circuit".to_owned();
    let error = verifier(wrong_association)
        .verify(&fixture.transition, &fixture.envelope, 840_100)
        .unwrap_err();
    assert_eq!(
        error,
        CanonicalGroth16Error::VerificationKeyAssociationMismatch
    );

    let mut unknown_key = fixture.envelope.clone();
    unknown_key.verification_key_id.0[0] ^= 1;
    unknown_key.statement_hash = statement_hash(&unknown_key).unwrap();
    assert!(matches!(
        verifier(fixture.vk_config).verify(&fixture.transition, &unknown_key, 840_100),
        Err(CanonicalGroth16Error::VerificationKeyNotFound(_))
    ));
}

#[test]
fn registry_rejects_same_vk_id_under_conflicting_associations() {
    let fixture = fixture();
    let mut registry = TrustedVerificationKeyRegistry::default();
    registry.register(fixture.vk_config.clone()).unwrap();

    assert_eq!(
        registry.register(fixture.vk_config.clone()).unwrap_err(),
        CanonicalGroth16Error::DuplicateVerificationKey
    );

    let mut different_circuit = fixture.vk_config.clone();
    different_circuit.circuit_id = "different-reviewed-circuit".to_owned();
    assert_eq!(
        registry.register(different_circuit).unwrap_err(),
        CanonicalGroth16Error::ConflictingVerificationKeyAssociation
    );

    let mut different_enablement = fixture.vk_config;
    different_enablement.enabled = false;
    assert_eq!(
        registry.register(different_enablement).unwrap_err(),
        CanonicalGroth16Error::ConflictingVerificationKeyAssociation
    );
}

#[test]
fn mutated_proof_and_authenticated_roots_are_rejected() {
    let fixture = fixture();
    let backend = verifier(fixture.vk_config);

    let mut proof = fixture.envelope.clone();
    proof.proof[17] ^= 1;
    assert!(matches!(
        backend.verify(&fixture.transition, &proof, 840_100),
        Err(CanonicalGroth16Error::InvalidProofEncoding(_))
            | Err(CanonicalGroth16Error::InvalidProof)
    ));

    let mut wrong_prev = fixture.transition.clone();
    wrong_prev.prev_state_root[0] ^= 1;
    assert_eq!(
        backend
            .verify(&wrong_prev, &fixture.envelope, 840_100)
            .unwrap_err(),
        CanonicalGroth16Error::PublicInputMismatch { slot: 0 }
    );

    let mut wrong_next = fixture.transition.clone();
    wrong_next.next_state_root[31] ^= 1;
    assert_eq!(
        backend
            .verify(&wrong_next, &fixture.envelope, 840_100)
            .unwrap_err(),
        CanonicalGroth16Error::PublicInputMismatch { slot: 3 }
    );
}

#[test]
fn reordered_or_mutated_inputs_are_rejected_before_pairing() {
    let fixture = fixture();
    let backend = verifier(fixture.vk_config);

    let mut reordered = fixture.envelope.clone();
    reordered.public_inputs.swap(0, 1);
    reordered.statement_hash = statement_hash(&reordered).unwrap();
    assert!(matches!(
        backend.verify(&fixture.transition, &reordered, 840_100),
        Err(CanonicalGroth16Error::PublicInputMismatch { .. })
    ));

    let mut witness_limb = fixture.envelope.clone();
    witness_limb.public_inputs[11] = FieldElement::from_bytes([1; 32]).unwrap();
    witness_limb.statement_hash = statement_hash(&witness_limb).unwrap();
    assert_eq!(
        backend
            .verify(&fixture.transition, &witness_limb, 840_100)
            .unwrap_err(),
        CanonicalGroth16Error::PublicInputMismatch { slot: 11 }
    );
}

#[test]
fn malformed_and_noncanonical_field_encodings_are_rejected() {
    let fixture = fixture();
    let mut malformed = envelope_json(&fixture.envelope);
    malformed["public_inputs"][0] = json!("00".repeat(31));
    assert!(matches!(
        parse_gateway_envelope_json(malformed),
        Err(CanonicalGroth16Error::MalformedEnvelope(_))
    ));

    let mut noncanonical = envelope_json(&fixture.envelope);
    noncanonical["public_inputs"][0] = json!(hex::encode(BN254_SCALAR_MODULUS));
    assert_eq!(
        parse_gateway_envelope_json(noncanonical).unwrap_err(),
        CanonicalGroth16Error::NonCanonicalFieldElement
    );
}

#[test]
fn wrong_curve_version_unknown_fields_and_raw_witness_are_rejected() {
    let fixture = fixture();
    let mut wrong_curve = envelope_json(&fixture.envelope);
    wrong_curve["curve"] = json!("bls12-381");
    assert!(matches!(
        parse_gateway_envelope_json(wrong_curve),
        Err(CanonicalGroth16Error::UnsupportedCurve(_))
    ));

    let mut wrong_version = envelope_json(&fixture.envelope);
    wrong_version["schema_version"] = json!(2);
    assert_eq!(
        parse_gateway_envelope_json(wrong_version).unwrap_err(),
        CanonicalGroth16Error::UnsupportedSchemaVersion(2)
    );

    let mut unknown = envelope_json(&fixture.envelope);
    unknown["optimistic"] = json!(true);
    assert!(matches!(
        parse_gateway_envelope_json(unknown),
        Err(CanonicalGroth16Error::MalformedEnvelope(_))
    ));

    let mut raw_witness = envelope_json(&fixture.envelope);
    raw_witness["raw_witness"] = json!(["secret"]);
    assert_eq!(
        parse_gateway_envelope_json(raw_witness).unwrap_err(),
        CanonicalGroth16Error::RawWitnessProvided
    );
}

#[test]
fn trailing_proof_and_vk_bytes_are_rejected() {
    let fixture = fixture();
    let backend = verifier(fixture.vk_config.clone());
    let mut proof = fixture.envelope.clone();
    proof.proof.push(0);
    assert!(matches!(
        backend.verify(&fixture.transition, &proof, 840_100),
        Err(CanonicalGroth16Error::InvalidProofEncoding(message))
            if message.contains("exactly 128 bytes")
    ));

    let mut trailing_vk = fixture.vk_config;
    trailing_vk.verification_key_bytes.push(0);
    trailing_vk.verification_key_id =
        VerificationKeyId::from_key_bytes(&trailing_vk.verification_key_bytes).unwrap();
    let mut registry = TrustedVerificationKeyRegistry::default();
    assert!(matches!(
        registry.register(trailing_vk),
        Err(CanonicalGroth16Error::InvalidVerificationKey(message))
            if message.contains("trailing bytes")
    ));
}

#[test]
fn proof_width_zero_proof_and_zero_current_height_are_rejected_early() {
    let fixture = fixture();

    let mut short_text = envelope_json(&fixture.envelope);
    short_text["proof"] = json!("00".repeat(127));
    assert!(matches!(
        parse_gateway_envelope_json(short_text),
        Err(CanonicalGroth16Error::InvalidProofEncoding(_))
    ));

    let mut zero_text = envelope_json(&fixture.envelope);
    zero_text["proof"] = json!("00".repeat(128));
    assert_eq!(
        parse_gateway_envelope_json(zero_text).unwrap_err(),
        CanonicalGroth16Error::AllZeroProof
    );

    assert_eq!(
        verifier(fixture.vk_config)
            .verify(&fixture.transition, &fixture.envelope, 0)
            .unwrap_err(),
        CanonicalGroth16Error::InvalidCurrentBlockHeight
    );
}

#[test]
fn statement_and_registry_vk_id_mismatches_are_rejected() {
    let fixture = fixture();
    let backend = verifier(fixture.vk_config.clone());
    let mut stale_hash = fixture.envelope.clone();
    stale_hash.statement_hash[0] ^= 1;
    assert!(matches!(
        backend.verify(&fixture.transition, &stale_hash, 840_100),
        Err(CanonicalGroth16Error::StatementHashMismatch { .. })
    ));

    let mut bad_id = fixture.vk_config;
    bad_id.verification_key_id.0[31] ^= 1;
    let mut registry = TrustedVerificationKeyRegistry::default();
    assert!(matches!(
        registry.register(bad_id),
        Err(CanonicalGroth16Error::VerificationKeyIdMismatch { .. })
    ));
}

#[test]
fn block_context_and_witness_commitment_are_authenticated() {
    let fixture = fixture();
    let backend = verifier(fixture.vk_config);

    assert!(matches!(
        backend.verify(&fixture.transition, &fixture.envelope, 839_999),
        Err(CanonicalGroth16Error::ProofFromFuture { .. })
    ));
    assert!(matches!(
        backend.verify(&fixture.transition, &fixture.envelope, 840_145),
        Err(CanonicalGroth16Error::ProofExpired { .. })
    ));

    let mut commitment = fixture.envelope.clone();
    commitment.witness_commitment[0] ^= 1;
    commitment.statement_hash = statement_hash(&commitment).unwrap();
    assert_eq!(
        backend
            .verify(&fixture.transition, &commitment, 840_100)
            .unwrap_err(),
        CanonicalGroth16Error::PublicInputMismatch { slot: 10 }
    );
}
