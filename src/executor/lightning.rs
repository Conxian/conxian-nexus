use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    /// MPP-specific partial failure (some paths failed, requires split-recovery).
    MppPartial,
}

impl fmt::Display for LightningFailureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Permanent => "permanent",
            Self::Transient => "transient",
            Self::Indeterminate => "indeterminate",
            Self::MppPartial => "mpp_partial",
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
    /// Payment is being split or rebalanced via MPP.
    MppSplitting,
}

impl fmt::Display for LightningPaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Recovering => "recovering",
            Self::MppSplitting => "mpp_splitting",
        };
        write!(f, "{}", s)
    }
}

/// Intent model for a Lightning payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub payment_id: String,
    pub payment_hash: String,
    pub amount_msat: u64,
    pub status: LightningPaymentStatus,
    pub failure_type: Option<LightningFailureType>,
    pub retry_count: i32,
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

/// Resilience and Recovery Adapter for Lightning Network (SRL-1).
pub struct LightningResilienceAdapter {
    // In a real implementation, this would likely hold a reference to storage or a relay.
}

impl Default for LightningResilienceAdapter {
    fn default() -> Self {
        Self::new()
    }
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
        use LightningPaymentStatus::*;
        matches!(
            (current, next),
            (Pending, Succeeded)
                | (Pending, Failed)
                | (Pending, Recovering)
                | (Pending, MppSplitting)
                | (MppSplitting, Succeeded)
                | (MppSplitting, Failed)
                | (MppSplitting, Recovering)
                | (Recovering, Succeeded)
                | (Recovering, Failed)
                | (Recovering, MppSplitting)
                | (Failed, Recovering)
                | (Failed, MppSplitting)
        )
    }

    /// Determines the failure taxonomy from a raw error or reason.
    pub fn categorize_failure(&self, reason: &str) -> LightningFailureType {
        if reason.contains("no_route") || reason.contains("invalid_invoice") {
            LightningFailureType::Permanent
        } else if reason.contains("mpp_partial_failure") || reason.contains("split_error") {
            LightningFailureType::MppPartial
        } else if reason.contains("timeout") || reason.contains("mpp_timeout") {
            LightningFailureType::Indeterminate
        } else {
            LightningFailureType::Transient
        }
    }

    /// Determines if a payment intent should trigger recovery based on its current state.
    pub fn should_recover(&self, intent: &PaymentIntent) -> bool {
        use LightningPaymentStatus::*;
        use LightningFailureType::*;

        match (intent.status, intent.failure_type) {
            (Recovering, _) => true,
            (Failed, Some(Transient)) if intent.retry_count < 3 => true,
            (Failed, Some(MppPartial)) => true,
            (Pending, _) => {
                let age = Utc::now() - intent.created_at;
                age.num_seconds() > 300 // 5 minutes
            }
            (MppSplitting, _) => {
                let age = Utc::now() - intent.last_updated_at;
                age.num_seconds() > 600 // 10 minutes
            }
            _ => false,
        }
    }

    /// Logic for processing recovery for different failure types.
    pub fn process_recovery(&self, intent: &mut PaymentIntent) -> Option<&'static str> {
        use LightningFailureType::*;
        use LightningPaymentStatus::*;

        if !self.should_recover(intent) {
            return None;
        }

        match intent.failure_type {
            Some(Transient) => {
                intent.status = Recovering;
                intent.retry_count += 1;
                intent.last_updated_at = Utc::now();
                Some("retry_initiated")
            }
            Some(MppPartial) => {
                intent.status = MppSplitting;
                intent.last_updated_at = Utc::now();
                Some("split_recovery_triggered")
            }
            Some(Indeterminate) => {
                // For Indeterminate, we typically stay in Pending/Recovering until reconciliation
                intent.status = Recovering;
                intent.last_updated_at = Utc::now();
                Some("reconciliation_requested")
            }
            _ if intent.status == Pending => {
                intent.status = Recovering;
                intent.last_updated_at = Utc::now();
                Some("stale_payment_recovery")
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_transition() {
        let adapter = LightningResilienceAdapter::new();
        assert!(adapter.validate_transition(
            LightningPaymentStatus::Pending,
            LightningPaymentStatus::MppSplitting
        ));
        assert!(adapter.validate_transition(
            LightningPaymentStatus::MppSplitting,
            LightningPaymentStatus::Succeeded
        ));
        assert!(adapter.validate_transition(
            LightningPaymentStatus::Failed,
            LightningPaymentStatus::MppSplitting
        ));
        assert!(!adapter.validate_transition(
            LightningPaymentStatus::Succeeded,
            LightningPaymentStatus::Pending
        ));
    }

    #[test]
    fn test_categorize_failure() {
        let adapter = LightningResilienceAdapter::new();
        assert_eq!(
            adapter.categorize_failure("no_route to node"),
            LightningFailureType::Permanent
        );
        assert_eq!(
            adapter.categorize_failure("mpp_partial_failure: path 2 failed"),
            LightningFailureType::MppPartial
        );
        assert_eq!(
            adapter.categorize_failure("mpp_timeout occurred"),
            LightningFailureType::Indeterminate
        );
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn test_failure_type_display() {
        assert_eq!(format!("{}", LightningFailureType::Permanent), "permanent");
        assert_eq!(format!("{}", LightningFailureType::Transient), "transient");
        assert_eq!(
            format!("{}", LightningFailureType::Indeterminate),
            "indeterminate"
        );
        assert_eq!(
            format!("{}", LightningFailureType::MppPartial),
            "mpp_partial"
        );
    }

    #[test]
    fn test_adapter_default() {
        let _adapter = LightningResilienceAdapter::default();
    }

    #[test]
    fn test_more_transitions() {
        let adapter = LightningResilienceAdapter::new();
        use LightningPaymentStatus::*;
        assert!(adapter.validate_transition(Recovering, MppSplitting));
        assert!(adapter.validate_transition(MppSplitting, Recovering));
        assert!(adapter.validate_transition(MppSplitting, Failed));
        assert!(adapter.validate_transition(Failed, Recovering));
    }

    #[test]
    fn test_categorize_split_error() {
        let adapter = LightningResilienceAdapter::new();
        assert_eq!(
            adapter.categorize_failure("split_error: amount too high"),
            LightningFailureType::MppPartial
        );
        assert_eq!(
            adapter.categorize_failure("random error"),
            LightningFailureType::Transient
        );
    }

    #[test]
    fn test_should_recover() {
        let adapter = LightningResilienceAdapter::new();
        let mut intent = PaymentIntent {
            payment_id: "p1".to_string(),
            payment_hash: "h1".to_string(),
            amount_msat: 1000,
            status: LightningPaymentStatus::Failed,
            failure_type: Some(LightningFailureType::Transient),
            retry_count: 0,
            created_at: Utc::now(),
            last_updated_at: Utc::now(),
        };

        assert!(adapter.should_recover(&intent));

        intent.retry_count = 3;
        assert!(!adapter.should_recover(&intent));

        intent.status = LightningPaymentStatus::Failed;
        intent.failure_type = Some(LightningFailureType::MppPartial);
        assert!(adapter.should_recover(&intent));
    }

    #[test]
    fn test_process_recovery_transient() {
        let adapter = LightningResilienceAdapter::new();
        let mut intent = PaymentIntent {
            payment_id: "p1".to_string(),
            payment_hash: "h1".to_string(),
            amount_msat: 1000,
            status: LightningPaymentStatus::Failed,
            failure_type: Some(LightningFailureType::Transient),
            retry_count: 0,
            created_at: Utc::now(),
            last_updated_at: Utc::now(),
        };

        let result = adapter.process_recovery(&mut intent);
        assert_eq!(result, Some("retry_initiated"));
        assert_eq!(intent.status, LightningPaymentStatus::Recovering);
        assert_eq!(intent.retry_count, 1);
    }
}
