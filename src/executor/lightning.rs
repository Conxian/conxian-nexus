use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::fmt;

/// Failure taxonomy for Lightning payments.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LightningFailureType {
    /// Permanent failure (e.g., invalid invoice, no route).
    Permanent,
    /// Transient failure (e.g., temporary node failure, timeout).
    Transient,
    /// Indeterminate state (e.g., payment in flight but no confirmation).
    Indeterminate,
}

impl fmt::Display for LightningFailureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Permanent => "permanent",
            Self::Transient => "transient",
            Self::Indeterminate => "indeterminate",
        };
        write!(f, "{}", s)
    }
}

/// Lifecycle states for a Lightning payment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LightningPaymentStatus {
    Pending,
    Succeeded,
    Failed,
    Recovering,
}

/// Intent model for a Lightning payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub payment_id: String,
    pub payment_hash: String,
    pub amount_msat: u64,
    pub status: LightningPaymentStatus,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
}

/// Event model for payment lifecycle tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentEvent {
    pub event_id: String,
    pub payment_id: String,
    pub status: LightningPaymentStatus,
    pub failure_type: Option<LightningFailureType>,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<String>,
}

/// Resilience and Recovery Adapter for Lightning Network.
pub struct LightningResilienceAdapter {
    // In a real implementation, this would likely hold a reference to storage or a relay.
}

impl LightningResilienceAdapter {
    pub fn new() -> Self {
        Self {}
    }

    /// Validates a state transition for a payment intent.
    pub fn validate_transition(
        &self,
        current: LightningPaymentStatus,
        next: LightningPaymentStatus,
    ) -> bool {
        match (current, next) {
            (LightningPaymentStatus::Pending, LightningPaymentStatus::Succeeded) => true,
            (LightningPaymentStatus::Pending, LightningPaymentStatus::Failed) => true,
            (LightningPaymentStatus::Pending, LightningPaymentStatus::Recovering) => true,
            (LightningPaymentStatus::Recovering, LightningPaymentStatus::Succeeded) => true,
            (LightningPaymentStatus::Recovering, LightningPaymentStatus::Failed) => true,
            (LightningPaymentStatus::Failed, LightningPaymentStatus::Recovering) => true,
            _ => false,
        }
    }

    /// Determines the failure taxonomy from a raw error or reason.
    pub fn categorize_failure(&self, reason: &str) -> LightningFailureType {
        if reason.contains("no_route") || reason.contains("invalid_invoice") {
            LightningFailureType::Permanent
        } else if reason.contains("timeout") || reason.contains("mpp_timeout") {
            LightningFailureType::Indeterminate
        } else {
            LightningFailureType::Transient
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_transition() {
        let adapter = LightningResilienceAdapter::new();
        assert!(adapter.validate_transition(LightningPaymentStatus::Pending, LightningPaymentStatus::Succeeded));
        assert!(adapter.validate_transition(LightningPaymentStatus::Pending, LightningPaymentStatus::Failed));
        assert!(adapter.validate_transition(LightningPaymentStatus::Failed, LightningPaymentStatus::Recovering));
        assert!(!adapter.validate_transition(LightningPaymentStatus::Succeeded, LightningPaymentStatus::Pending));
    }

    #[test]
    fn test_categorize_failure() {
        let adapter = LightningResilienceAdapter::new();
        assert_eq!(adapter.categorize_failure("no_route to node"), LightningFailureType::Permanent);
        assert_eq!(adapter.categorize_failure("mpp_timeout occurred"), LightningFailureType::Indeterminate);
        assert_eq!(adapter.categorize_failure("temporary_node_failure"), LightningFailureType::Transient);
    }
}
