//! Regenerates the deterministic, test-only CON-1533 BN254 fixture.
//! The resulting key must never be enabled by production defaults.

use ark_bn254::{Bn254, Fr};
use ark_crypto_primitives::snark::{CircuitSpecificSetupSNARK, SNARK};
use ark_ff::{BigInt, BigInteger, PrimeField};
use ark_groth16::Groth16;
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError, Variable};
use ark_serialize::CanonicalSerialize;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use conxian_nexus::executor::bitvm::{
    canonical_public_inputs, canonical_statement_hash, verification_key_id, BitcoinBlockContext,
    BitcoinNetwork, CIRCUIT_ID,
};

#[derive(Clone)]
struct FixtureCircuit {
    public_inputs: [Fr; 7],
}

impl ConstraintSynthesizer<Fr> for FixtureCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        for value in self.public_inputs {
            let public = cs.new_input_variable(|| Ok(value))?;
            let witness = cs.new_witness_variable(|| Ok(value))?;
            cs.enforce_r1cs_constraint(
                || ark_relations::lc!() + witness,
                || ark_relations::lc!() + Variable::One,
                || ark_relations::lc!() + public,
            )?;
        }
        Ok(())
    }
}

fn field(bytes: &[u8; 32]) -> Fr {
    let bits = bytes
        .iter()
        .flat_map(|byte| (0..8).rev().map(move |bit| (byte >> bit) & 1 == 1))
        .collect::<Vec<_>>();
    Fr::from_bigint(BigInt::<4>::from_bits_be(&bits)).unwrap()
}

fn main() {
    let prev = hex::decode("00112233445566778899aabbccddeeff102132435465768798a9babcbddcedfe")
        .unwrap()
        .try_into()
        .unwrap();
    let next = hex::decode("ffeeddccbbaa998877665544332211000102030405060708090a0b0c0d0e0f10")
        .unwrap()
        .try_into()
        .unwrap();
    let witness = hex::decode("1234567890abcdef1234567890abcdef0fedcba0987654320fedcba098765432")
        .unwrap()
        .try_into()
        .unwrap();
    let steps = 4242;
    let inputs = canonical_public_inputs(prev, next, steps, witness);
    let circuit = FixtureCircuit {
        public_inputs: inputs.map(|value| field(&value)),
    };
    let mut setup_rng = StdRng::from_seed([0x53; 32]);
    let (proving_key, verifying_key) =
        Groth16::<Bn254>::setup(circuit.clone(), &mut setup_rng).unwrap();
    let mut proof_rng = StdRng::from_seed([0x91; 32]);
    let proof = Groth16::<Bn254>::prove(&proving_key, circuit, &mut proof_rng).unwrap();
    let wrong_inputs = canonical_public_inputs(prev, next, steps + 1, witness);
    let wrong_circuit = FixtureCircuit {
        public_inputs: wrong_inputs.map(|value| field(&value)),
    };
    let mut wrong_proof_rng = StdRng::from_seed([0x92; 32]);
    let wrong_proof =
        Groth16::<Bn254>::prove(&proving_key, wrong_circuit, &mut wrong_proof_rng).unwrap();

    let mut vk_bytes = Vec::new();
    verifying_key.serialize_compressed(&mut vk_bytes).unwrap();
    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes).unwrap();
    let mut wrong_proof_bytes = Vec::new();
    wrong_proof
        .serialize_compressed(&mut wrong_proof_bytes)
        .unwrap();
    assert_eq!(proof_bytes.len(), 128);
    let vk_id = verification_key_id(&vk_bytes);
    let block_context = BitcoinBlockContext {
        network: BitcoinNetwork::Regtest,
        block_height: 840_001,
        block_hash: "1112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f30".to_string(),
        max_valid_height: Some(840_100),
    };
    let statement_hash = canonical_statement_hash(vk_id, &inputs, witness, &block_context).unwrap();
    let fixture = serde_json::json!({
        "test_only": true,
        "generation": {
            "setup_seed_hex": hex::encode([0x53; 32]),
            "proof_seed_hex": hex::encode([0x91; 32]),
            "rejected_proof_seed_hex": hex::encode([0x92; 32]),
            "generator": "examples/generate_bitvm_groth16_fixture.rs"
        },
        "registry": [{
            "schema_version": 1,
            "curve": "bn254",
            "circuit_id": CIRCUIT_ID,
            "verification_key_id": hex::encode(vk_id),
            "verification_key_base64": BASE64_STANDARD.encode(vk_bytes),
            "enabled": true
        }],
        "adversarial_wrong_proof": hex::encode(wrong_proof_bytes),
        "request": {
            "schema_version": 1,
            "curve": "bn254",
            "circuit_id": CIRCUIT_ID,
            "verification_key_id": hex::encode(vk_id),
            "prev_state_root": format!("0x{}", hex::encode(prev)),
            "next_state_root": format!("0x{}", hex::encode(next)),
            "steps_verified": steps,
            "witness_commitment": hex::encode(witness),
            "public_inputs": inputs.map(hex::encode),
            "block_context": block_context,
            "proof": hex::encode(proof_bytes),
            "statement_hash": hex::encode(statement_hash),
            "trace_id": "fixture-con-1533"
        }
    });
    println!("{}", serde_json::to_string_pretty(&fixture).unwrap());
}
