use ark_bls12_381::{Bls12_381, Fr};
use ark_crypto_primitives::snark::{CircuitSpecificSetupSNARK, SNARK};
use ark_ff::UniformRand;
use ark_groth16::Groth16;
use ark_relations::{
    gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError, Variable},
    lc,
};
use ark_serialize::CanonicalSerialize;
use ark_std::rand::SeedableRng;
use conxian_nexus::config::Config;
use conxian_nexus::executor::bitvm::{BitVMAdapter, BitVMTransition};
use conxian_nexus::storage::Storage;
use std::sync::Arc;

#[tokio::test]
async fn test_cryptographic_bitvm_verification() {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let adapter = BitVMAdapter::new(storage);

    let mut rng = ark_std::rand::rngs::StdRng::seed_from_u64(42);

    struct DummyCircuit {
        x: Option<Fr>,
    }
    impl ConstraintSynthesizer<Fr> for DummyCircuit {
        fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
            let x = cs.new_witness_variable(|| self.x.ok_or(SynthesisError::AssignmentMissing))?;
            let res = cs.new_input_variable(|| {
                let x_val = self.x.ok_or(SynthesisError::AssignmentMissing)?;
                Ok(x_val + x_val)
            })?;

            cs.enforce_r1cs_constraint(|| lc![x, x], || lc![Variable::One], || lc![res])?;
            Ok(())
        }
    }

    let (pk, vk) = Groth16::<Bls12_381>::setup(DummyCircuit { x: None }, &mut rng).unwrap();
    let x = Fr::rand(&mut rng);
    let proof = Groth16::<Bls12_381>::prove(&pk, DummyCircuit { x: Some(x) }, &mut rng).unwrap();
    let public_input = x + x;

    let mut proof_bytes = Vec::new();
    proof.serialize_compressed(&mut proof_bytes).unwrap();
    let mut vk_bytes = Vec::new();
    vk.serialize_compressed(&mut vk_bytes).unwrap();
    let mut input_bytes = Vec::new();
    public_input.serialize_compressed(&mut input_bytes).unwrap();

    let transition = BitVMTransition {
        prev_state_root: "0x0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        next_state_root: "0x0000000000000000000000000000000000000000000000000000000000000001"
            .to_string(),
        proof_bytes: hex::encode(proof_bytes),
        vk_bytes: hex::encode(vk_bytes),
        public_inputs: vec![hex::encode(input_bytes)],
        trace_id: format!("test_trace_{}", uuid::Uuid::new_v4()),
    };

    let result = adapter.verify_transition(&transition).await.unwrap();
    assert!(result.valid);
    assert!(result.message.contains("cryptographically verified"));
}
