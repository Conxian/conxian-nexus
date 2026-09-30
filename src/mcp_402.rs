//! MCP-402 server-side payment gate.
//!
//! Mirrors `conxian_market`'s `mcp_402.ts` facade on the Nexus proof layer: an
//! MCP tool invocation is mapped to an x402 (HTTP 402 Payment Required)
//! demand, the payment is verified — structurally against the receipt and,
//! where a signed proof is supplied, cryptographically via
//! [`crate::verification::X402PaymentVerifier`] — and the tool dispatch is
//! gated fail-closed on the result.
//!
//! Nexus observes and proves; it does not settle or custody funds, so this
//! module reuses the existing cryptographic verifier rather than re-deriving
//! signature or settlement rules.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::verification::{X402PaymentPayload, X402PaymentVerifier};

/// Canonical x402 scheme token used by MCP-402 demands.
pub const MCP402_SCHEME: &str = "x402";
/// Settlement currency for MCP-402 demands (satoshis).
pub const MCP402_CURRENCY: &str = "sats";

/// Job context an MCP tool invocation is billed against (mirrors `JobCard`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJobContext {
    pub id: String,
    pub title: String,
    pub description: String,
    pub bounty_sats: u64,
    #[serde(default)]
    pub deadline: Option<u64>,
}

/// An MCP tool call gated by an x402 payment demand (mirrors `McpToolCall`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    pub job: McpJobContext,
    pub payer_did: String,
}

/// x402 payment demand for an MCP tool invocation (mirrors `X402PaymentDemand`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mcp402Demand {
    pub scheme: String,
    pub amount_sats: u64,
    pub currency: String,
    pub payment_pointer: String,
    pub resource_id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub rail: Option<String>,
}

/// x402 payment receipt proving payment against a prior demand
/// (mirrors `X402PaymentReceipt`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mcp402Receipt {
    pub demand_id: String,
    pub transaction_id: String,
    pub amount_sats: u64,
    pub paid_at: u64,
    pub payer_did: String,
}

/// Result of gating an MCP tool call on an x402 payment
/// (mirrors `Mcp402PaymentGateResult`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mcp402PaymentGateResult {
    pub authorized: bool,
    pub demand: Mcp402Demand,
    #[serde(default)]
    pub receipt: Option<Mcp402Receipt>,
    #[serde(default)]
    pub authorization_token: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl Mcp402PaymentGateResult {
    fn denied(demand: Mcp402Demand, reason: impl Into<String>) -> Self {
        Self {
            authorized: false,
            demand,
            receipt: None,
            authorization_token: None,
            reason: Some(reason.into()),
        }
    }
}

/// Server-side MCP-402 facade: demand -> verify -> dispatch gate.
#[derive(Debug, Clone)]
pub struct Mcp402Facade {
    settled: Arc<Mutex<HashSet<String>>>,
    verifier: X402PaymentVerifier,
}

impl Default for Mcp402Facade {
    fn default() -> Self {
        Self::new()
    }
}

impl Mcp402Facade {
    pub fn new() -> Self {
        Self {
            settled: Arc::new(Mutex::new(HashSet::new())),
            verifier: X402PaymentVerifier::new(),
        }
    }

    /// Build the x402 payment demand for an MCP tool invocation.
    ///
    /// The zero-bounty fail-closed boundary lives in [`Self::authorize`] and
    /// [`Self::authorize_with_proof`]; this builder mirrors the market's
    /// `jobCardToDemand` field mapping without rejecting on its own.
    pub fn demand(&self, call: &McpToolCall) -> Mcp402Demand {
        let description = if call.job.description.is_empty() {
            call.job.title.clone()
        } else {
            call.job.description.clone()
        };
        Mcp402Demand {
            scheme: MCP402_SCHEME.to_string(),
            amount_sats: call.job.bounty_sats,
            currency: MCP402_CURRENCY.to_string(),
            payment_pointer: format!("$conxian.com/market/job/{}", call.job.id),
            resource_id: call.job.id.clone(),
            description: Some(description),
            expires_at: call.job.deadline,
            rail: None,
        }
    }

    /// Structurally validate a receipt against its demand
    /// (mirrors `verifyPaymentReceipt`).
    pub fn verify_receipt(&self, demand: &Mcp402Demand, receipt: &Mcp402Receipt) -> bool {
        if receipt.demand_id != demand.resource_id {
            return false;
        }
        if receipt.amount_sats != demand.amount_sats {
            return false;
        }
        if receipt.payer_did.is_empty() || receipt.transaction_id.is_empty() {
            return false;
        }
        if let Some(expires_at) = demand.expires_at {
            if receipt.paid_at > expires_at {
                return false;
            }
        }
        true
    }

    /// Gate dispatch on a structurally valid receipt (fail-closed).
    pub fn authorize(
        &self,
        call: &McpToolCall,
        receipt: &Mcp402Receipt,
    ) -> Mcp402PaymentGateResult {
        let demand = self.demand(call);
        if call.job.bounty_sats == 0 {
            return Mcp402PaymentGateResult::denied(
                demand,
                "bounty must be greater than zero satoshis",
            );
        }
        if !self.verify_receipt(&demand, receipt) {
            return Mcp402PaymentGateResult::denied(demand, "invalid or mismatched x402 receipt");
        }
        self.settled
            .lock()
            .expect("mcp-402 settled set poisoned")
            .insert(receipt.demand_id.clone());
        Mcp402PaymentGateResult {
            authorized: true,
            demand,
            receipt: Some(receipt.clone()),
            authorization_token: None,
            reason: None,
        }
    }

    /// Gate dispatch on a structurally valid receipt AND a cryptographically
    /// verified x402 payment proof. Issues the verifier's canonical
    /// authorization token on success.
    pub fn authorize_with_proof(
        &self,
        call: &McpToolCall,
        receipt: &Mcp402Receipt,
        proof: &X402PaymentPayload,
    ) -> Mcp402PaymentGateResult {
        let demand = self.demand(call);
        if call.job.bounty_sats == 0 {
            return Mcp402PaymentGateResult::denied(
                demand,
                "bounty must be greater than zero satoshis",
            );
        }
        if !self.verify_receipt(&demand, receipt) {
            return Mcp402PaymentGateResult::denied(demand, "invalid or mismatched x402 receipt");
        }

        let verified = match self.verifier.verify_payment_proof(proof) {
            Ok(res) => res,
            Err(e) => {
                return Mcp402PaymentGateResult::denied(
                    demand,
                    format!("x402 proof verification failed: {e}"),
                )
            }
        };

        // Bind the cryptographically valid proof to this specific invocation.
        if proof.payer.trim() != receipt.payer_did.trim() {
            return Mcp402PaymentGateResult::denied(
                demand,
                "proof payer does not match receipt payer",
            );
        }
        if proof.amount_sats < demand.amount_sats {
            return Mcp402PaymentGateResult::denied(
                demand,
                "proof amount does not cover the demand",
            );
        }

        self.settled
            .lock()
            .expect("mcp-402 settled set poisoned")
            .insert(receipt.demand_id.clone());
        Mcp402PaymentGateResult {
            authorized: true,
            demand,
            receipt: Some(receipt.clone()),
            authorization_token: Some(verified.authorization_token),
            reason: None,
        }
    }

    /// Whether a demand id has already been settled (replay guard).
    pub fn is_settled(&self, demand_id: &str) -> bool {
        self.settled
            .lock()
            .expect("mcp-402 settled set poisoned")
            .contains(demand_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verification::X402_VERIFIER_ID;
    use k256::schnorr::SigningKey;
    use sha2::{Digest, Sha256};

    fn job() -> McpJobContext {
        McpJobContext {
            id: "job-1".to_string(),
            title: "Build adapter".to_string(),
            description: "Build a chain adapter".to_string(),
            bounty_sats: 1000,
            deadline: Some(1750005000),
        }
    }

    fn call() -> McpToolCall {
        McpToolCall {
            tool: "submit_job".to_string(),
            arguments: serde_json::json!({}),
            job: job(),
            payer_did: "did:conxian:payer:1".to_string(),
        }
    }

    fn receipt() -> Mcp402Receipt {
        Mcp402Receipt {
            demand_id: "job-1".to_string(),
            transaction_id: "tx-1".to_string(),
            amount_sats: 1000,
            paid_at: 1750000100,
            payer_did: "did:conxian:payer:1".to_string(),
        }
    }

    /// Build a cryptographically valid x402 proof bound to the given payer/amount.
    fn signed_proof(payer: &str, amount_sats: u64) -> X402PaymentPayload {
        let signing_key = SigningKey::from_bytes(&[0x07u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();
        let scheme = "x402";
        let network = "bitcoin_mainnet";
        let payee = "bc1qpayee987654321";
        let nonce = "nonce_1234567890abcdef";
        let timestamp = 1750000000;
        let expires_at = 1750003600;

        let canonical_msg = format!(
            "x402:v2:{}:{}:{}:{}:{}:{}:{}",
            scheme, network, amount_sats, payer, payee, nonce, timestamp
        );
        let digest: [u8; 32] = Sha256::digest(canonical_msg.as_bytes()).into();
        let signature = signing_key.sign_raw(&digest, &[0x09u8; 32]).unwrap();

        X402PaymentPayload {
            protocol_id: X402_VERIFIER_ID.to_string(),
            scheme: scheme.to_string(),
            network: network.to_string(),
            amount_sats,
            payer: payer.to_string(),
            payee: payee.to_string(),
            nonce: nonce.to_string(),
            timestamp,
            expires_at,
            public_key: hex::encode(verifying_key.to_bytes()),
            signature: hex::encode(signature.to_bytes()),
            payment_hash: None,
        }
    }

    #[test]
    fn gates_dispatch_on_a_valid_receipt() {
        let facade = Mcp402Facade::new();
        let result = facade.authorize(&call(), &receipt());
        assert!(result.authorized);
        assert!(facade.is_settled("job-1"));
    }

    #[test]
    fn fails_closed_on_an_amount_mismatch() {
        let facade = Mcp402Facade::new();
        let result = facade.authorize(
            &call(),
            &Mcp402Receipt {
                amount_sats: 1,
                ..receipt()
            },
        );
        assert!(!result.authorized);
        assert!(result.reason.unwrap().contains("receipt"));
        assert!(!facade.is_settled("job-1"));
    }

    #[test]
    fn fails_closed_on_a_zero_bounty() {
        let facade = Mcp402Facade::new();
        let mut call = call();
        call.job.bounty_sats = 0;
        let result = facade.authorize(&call, &receipt());
        assert!(!result.authorized);
        assert!(!facade.is_settled("job-1"));
    }

    #[test]
    fn authorize_with_proof_gates_on_a_cryptographically_valid_proof() {
        let facade = Mcp402Facade::new();
        let proof = signed_proof("did:conxian:payer:1", 1000);
        let result = facade.authorize_with_proof(&call(), &receipt(), &proof);
        assert!(result.authorized);
        assert!(result.authorization_token.unwrap().starts_with("x402:v2:"));
        assert!(facade.is_settled("job-1"));
    }

    #[test]
    fn authorize_with_proof_rejects_an_invalid_signature() {
        let facade = Mcp402Facade::new();
        let mut proof = signed_proof("did:conxian:payer:1", 1000);
        let mut sig = hex::decode(&proof.signature).unwrap();
        sig[0] ^= 0xFF;
        proof.signature = hex::encode(sig);
        let result = facade.authorize_with_proof(&call(), &receipt(), &proof);
        assert!(!result.authorized);
        assert!(result.reason.unwrap().contains("verification failed"));
    }

    #[test]
    fn authorize_with_proof_rejects_underpayment() {
        let facade = Mcp402Facade::new();
        let proof = signed_proof("did:conxian:payer:1", 500);
        let result = facade.authorize_with_proof(&call(), &receipt(), &proof);
        assert!(!result.authorized);
        assert!(result.reason.unwrap().contains("does not cover"));
    }

    #[test]
    fn verify_receipt_rejects_a_mismatched_demand_id() {
        let facade = Mcp402Facade::new();
        let demand = facade.demand(&call());
        let r = Mcp402Receipt {
            demand_id: "job-other".to_string(),
            ..receipt()
        };
        assert!(!facade.verify_receipt(&demand, &r));
    }
}
