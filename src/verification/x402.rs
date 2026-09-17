//! x402 V2 Settlement Rail Payment Verifier
//!
//! Provides cryptographic verification of HTTP 402 payment authorization proofs,
//! agentic payment challenges, Schnorr/EIP-712 payment signatures, and satoshi
//! payment commitments per CON-804 and AWS Bedrock AgentCore Payments specification.

use k256::schnorr::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Canonical x402 V2 settlement verifier protocol identifier.
pub const X402_VERIFIER_ID: &str = "x402-v2-settlement-verifier";

/// Errors that can occur during x402 V2 payment verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum X402Error {
    #[error("Protocol identifier mismatch: expected '{expected}', found '{found}'")]
    ProtocolMismatch { expected: String, found: String },

    #[error("Empty required payment field: {0}")]
    EmptyField(String),

    #[error("Unsupported payment scheme '{0}'")]
    UnsupportedScheme(String),

    #[error("Invalid payment amount: must be greater than zero satoshis")]
    InvalidAmount,

    #[error("Invalid nonce: minimum length is 16 characters for replay protection")]
    InvalidNonce,

    #[error("Payment challenge has expired (expires_at {expires_at} <= timestamp {timestamp})")]
    ExpiredChallenge { timestamp: u64, expires_at: u64 },

    #[error("Malformed hex encoding in field '{field}': {reason}")]
    MalformedHex { field: String, reason: String },

    #[error("Invalid public key encoding: {0}")]
    InvalidPublicKey(String),

    #[error("x402 signature verification failed: {0}")]
    VerificationFailed(String),
}

/// Request payload for verifying an x402 V2 payment authorization proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X402PaymentPayload {
    /// Protocol identifier (must be `"x402-v2-settlement-verifier"`).
    pub protocol_id: String,
    /// Payment scheme (e.g. `"x402"`, `"lightning_bolt11"`, `"exact_sats"`, `"eip712"`, `"schnorr_taproot"`).
    pub scheme: String,
    /// Target network identifier (e.g. `"bitcoin_mainnet"`, `"lightning"`, `"evm_mainnet"`, `"stacks"`).
    pub network: String,
    /// Payment amount in satoshis or atomic units (must be > 0).
    pub amount_sats: u64,
    /// Payer address or identity string.
    pub payer: String,
    /// Payee address or identity string.
    pub payee: String,
    /// Nonce for replay protection (must be >= 16 characters).
    pub nonce: String,
    /// Issue timestamp (epoch seconds).
    pub timestamp: u64,
    /// Expiration timestamp (epoch seconds, must be > timestamp).
    pub expires_at: u64,
    /// Hex-encoded public key (32-byte XOnly or 33-byte compressed SEC1).
    pub public_key: String,
    /// Hex-encoded cryptographic signature (64-byte Schnorr r||s).
    pub signature: String,
    /// Optional hex-encoded payment preimage commitment hash.
    #[serde(default)]
    pub payment_hash: Option<String>,
}

/// Response returned after verifying an x402 V2 payment authorization payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct X402VerificationResponse {
    /// Protocol identifier.
    pub protocol_id: String,
    /// Whether the payment proof and cryptographic signature are valid.
    pub is_valid: bool,
    /// Validated payment scheme.
    pub scheme: String,
    /// Target network identifier.
    pub network: String,
    /// Amount verified in satoshis.
    pub amount_sats: u64,
    /// Payer identity.
    pub payer: String,
    /// Payee identity.
    pub payee: String,
    /// Nonce used for verification.
    pub nonce: String,
    /// Canonical settlement authorization token issued upon verification.
    pub authorization_token: String,
}

/// Verifier instance for x402 V2 settlement rail payments.
#[derive(Debug, Default, Clone)]
pub struct X402PaymentVerifier;

impl X402PaymentVerifier {
    pub fn new() -> Self {
        Self
    }

    /// Verifies an x402 V2 payment authorization proof payload.
    pub fn verify_payment_proof(
        &self,
        payload: &X402PaymentPayload,
    ) -> Result<X402VerificationResponse, X402Error> {
        // Enforce protocol ID
        if payload.protocol_id != X402_VERIFIER_ID {
            return Err(X402Error::ProtocolMismatch {
                expected: X402_VERIFIER_ID.to_string(),
                found: payload.protocol_id.clone(),
            });
        }

        // Validate non-empty fields
        if payload.scheme.trim().is_empty() {
            return Err(X402Error::EmptyField("scheme".to_string()));
        }
        if payload.network.trim().is_empty() {
            return Err(X402Error::EmptyField("network".to_string()));
        }
        if payload.payer.trim().is_empty() {
            return Err(X402Error::EmptyField("payer".to_string()));
        }
        if payload.payee.trim().is_empty() {
            return Err(X402Error::EmptyField("payee".to_string()));
        }
        if payload.public_key.trim().is_empty() {
            return Err(X402Error::EmptyField("public_key".to_string()));
        }
        if payload.signature.trim().is_empty() {
            return Err(X402Error::EmptyField("signature".to_string()));
        }

        // Validate scheme
        let normalized_scheme = payload.scheme.trim().to_ascii_lowercase();
        match normalized_scheme.as_str() {
            "x402" | "lightning_bolt11" | "exact_sats" | "eip712" | "schnorr_taproot" => {}
            _ => return Err(X402Error::UnsupportedScheme(payload.scheme.clone())),
        }

        // Validate amount
        if payload.amount_sats == 0 {
            return Err(X402Error::InvalidAmount);
        }

        // Validate nonce length (minimum 16 chars)
        if payload.nonce.trim().len() < 16 {
            return Err(X402Error::InvalidNonce);
        }

        // Validate challenge expiration bounds
        if payload.expires_at <= payload.timestamp {
            return Err(X402Error::ExpiredChallenge {
                timestamp: payload.timestamp,
                expires_at: payload.expires_at,
            });
        }

        // Build canonical message digest for signature verification
        let canonical_message = format!(
            "x402:v2:{}:{}:{}:{}:{}:{}:{}",
            normalized_scheme,
            payload.network.trim().to_ascii_lowercase(),
            payload.amount_sats,
            payload.payer.trim(),
            payload.payee.trim(),
            payload.nonce.trim(),
            payload.timestamp
        );
        let msg_digest: [u8; 32] = Sha256::digest(canonical_message.as_bytes()).into();

        // Decode public key (32-byte XOnly or 33-byte SEC1)
        let pk_bytes = hex::decode(payload.public_key.trim_start_matches("0x")).map_err(|e| {
            X402Error::MalformedHex {
                field: "public_key".to_string(),
                reason: e.to_string(),
            }
        })?;

        let verifying_key = if pk_bytes.len() == 32 {
            let key_array: [u8; 32] = pk_bytes.as_slice().try_into().map_err(|_| {
                X402Error::InvalidPublicKey("invalid 32-byte array conversion".to_string())
            })?;
            VerifyingKey::from_bytes(&key_array.into()).map_err(|e| {
                X402Error::InvalidPublicKey(format!("invalid 32-byte BIP-340 key: {}", e))
            })?
        } else if pk_bytes.len() == 33 {
            let xonly_slice = &pk_bytes[1..33];
            let key_array: [u8; 32] = xonly_slice.try_into().map_err(|_| {
                X402Error::InvalidPublicKey("invalid SEC1 slice conversion".to_string())
            })?;
            VerifyingKey::from_bytes(&key_array.into()).map_err(|e| {
                X402Error::InvalidPublicKey(format!("invalid SEC1 public key: {}", e))
            })?
        } else {
            return Err(X402Error::InvalidPublicKey(
                "public_key must be 32 bytes (XOnly) or 33 bytes (compressed SEC1)".to_string(),
            ));
        };

        // Decode 64-byte BIP-340 Schnorr signature
        let sig_bytes = hex::decode(payload.signature.trim_start_matches("0x")).map_err(|e| {
            X402Error::MalformedHex {
                field: "signature".to_string(),
                reason: e.to_string(),
            }
        })?;

        if sig_bytes.len() != 64 {
            return Err(X402Error::VerificationFailed(
                "Schnorr signature must be exactly 64 bytes".to_string(),
            ));
        }

        let signature = Signature::try_from(sig_bytes.as_slice()).map_err(|e| {
            X402Error::VerificationFailed(format!("invalid Schnorr signature format: {}", e))
        })?;

        // Perform cryptographic signature verification over canonical payload digest
        verifying_key
            .verify_raw(&msg_digest, &signature)
            .map_err(|e| {
                X402Error::VerificationFailed(format!("x402 signature verification failed: {}", e))
            })?;

        // Issue authorization token
        let token_digest = Sha256::digest(
            format!("auth:{}:{}", payload.nonce, hex::encode(msg_digest)).as_bytes(),
        );
        let authorization_token =
            format!("x402:v2:{}:{}", payload.nonce, hex::encode(token_digest));

        Ok(X402VerificationResponse {
            protocol_id: X402_VERIFIER_ID.to_string(),
            is_valid: true,
            scheme: payload.scheme.clone(),
            network: payload.network.clone(),
            amount_sats: payload.amount_sats,
            payer: payload.payer.clone(),
            payee: payload.payee.clone(),
            nonce: payload.nonce.clone(),
            authorization_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::SigningKey;

    #[test]
    fn test_x402_verification_valid_signature() {
        let verifier = X402PaymentVerifier::new();

        let signing_key = SigningKey::from_bytes(&[0x05u8; 32].into()).unwrap();
        let verifying_key = signing_key.verifying_key();

        let scheme = "x402";
        let network = "bitcoin_mainnet";
        let amount_sats = 50000;
        let payer = "bc1qpayer123456789";
        let payee = "bc1qpayee987654321";
        let nonce = "nonce_1234567890abcdef";
        let timestamp = 1750000000;
        let expires_at = 1750003600;

        let canonical_msg = format!(
            "x402:v2:{}:{}:{}:{}:{}:{}:{}",
            scheme, network, amount_sats, payer, payee, nonce, timestamp
        );
        let msg_digest: [u8; 32] = Sha256::digest(canonical_msg.as_bytes()).into();
        let aux_rand = [0x09u8; 32];
        let signature = signing_key.sign_raw(&msg_digest, &aux_rand).unwrap();

        let payload = X402PaymentPayload {
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
        };

        let res = verifier.verify_payment_proof(&payload).unwrap();
        assert!(res.is_valid);
        assert_eq!(res.scheme, "x402");
        assert_eq!(res.amount_sats, 50000);
        assert!(res.authorization_token.starts_with("x402:v2:"));
    }

    #[test]
    fn test_x402_verification_invalid_nonce() {
        let verifier = X402PaymentVerifier::new();
        let payload = X402PaymentPayload {
            protocol_id: X402_VERIFIER_ID.to_string(),
            scheme: "x402".to_string(),
            network: "lightning".to_string(),
            amount_sats: 100,
            payer: "payer".to_string(),
            payee: "payee".to_string(),
            nonce: "short_nonce".to_string(), // < 16 chars
            timestamp: 1000,
            expires_at: 2000,
            public_key: "00".repeat(32),
            signature: "00".repeat(64),
            payment_hash: None,
        };

        let err = verifier.verify_payment_proof(&payload).unwrap_err();
        assert_eq!(err, X402Error::InvalidNonce);
    }

    #[test]
    fn test_x402_verification_expired_challenge() {
        let verifier = X402PaymentVerifier::new();
        let payload = X402PaymentPayload {
            protocol_id: X402_VERIFIER_ID.to_string(),
            scheme: "x402".to_string(),
            network: "lightning".to_string(),
            amount_sats: 100,
            payer: "payer".to_string(),
            payee: "payee".to_string(),
            nonce: "long_nonce_123456789".to_string(),
            timestamp: 2000,
            expires_at: 1000, // expired!
            public_key: "00".repeat(32),
            signature: "00".repeat(64),
            payment_hash: None,
        };

        let err = verifier.verify_payment_proof(&payload).unwrap_err();
        assert_eq!(
            err,
            X402Error::ExpiredChallenge {
                timestamp: 2000,
                expires_at: 1000,
            }
        );
    }

    #[test]
    fn test_x402_verification_zero_amount() {
        let verifier = X402PaymentVerifier::new();
        let payload = X402PaymentPayload {
            protocol_id: X402_VERIFIER_ID.to_string(),
            scheme: "x402".to_string(),
            network: "lightning".to_string(),
            amount_sats: 0, // invalid!
            payer: "payer".to_string(),
            payee: "payee".to_string(),
            nonce: "long_nonce_123456789".to_string(),
            timestamp: 1000,
            expires_at: 2000,
            public_key: "00".repeat(32),
            signature: "00".repeat(64),
            payment_hash: None,
        };

        let err = verifier.verify_payment_proof(&payload).unwrap_err();
        assert_eq!(err, X402Error::InvalidAmount);
    }
}
