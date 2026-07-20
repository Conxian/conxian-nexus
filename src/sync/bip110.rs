//! Partial, metadata-only assessment of the observed BIP-110 size rules.
//!
//! This module deliberately accepts caller-supplied sizes instead of parsing
//! raw Bitcoin transactions or connecting to a Bitcoin backend. It is a
//! conservative policy assessment, not consensus validation. It does not
//! implement deployment or activation state, UTXO grandfathering, Taproot
//! execution rules, proof-of-work or header verification, transaction
//! inclusion proofs, or a completeness guarantee for the supplied metadata.

use serde::{Deserialize, Serialize};

/// Maximum observed OP_PUSHDATA* payload size in bytes for this partial policy.
pub const MAX_PUSHDATA_SIZE: u64 = 256;
/// Maximum observed OP_RETURN script size in bytes for this partial policy.
pub const MAX_OP_RETURN_SCRIPT_SIZE: u64 = 83;
/// Maximum observed non-OP_RETURN scriptPubKey size in bytes for this partial policy.
pub const MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE: u64 = 34;
/// Maximum observed witness element size in bytes for this partial policy.
pub const MAX_WITNESS_ELEMENT_SIZE: u64 = 256;

/// Whether the caller supplied usable observation metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationAvailability {
    /// The supplied metadata can be assessed by this module.
    Available,
    /// The observation backend or required metadata is unavailable.
    Unavailable,
}

/// Caller-supplied sizes used by the partial BIP-110 assessment.
///
/// OP_RETURN scripts and other scriptPubKeys are intentionally represented by
/// separate vectors. This prevents the OP_RETURN exception from being
/// incorrectly assessed against the non-OP_RETURN 34-byte limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSizeMetadata {
    /// Whether the observation backend supplied usable metadata.
    pub availability: ObservationAvailability,
    /// Size of each observed OP_RETURN output script, in bytes.
    #[serde(default)]
    pub op_return_script_sizes: Vec<u64>,
    /// Size of each observed non-OP_RETURN output scriptPubKey, in bytes.
    #[serde(default)]
    pub non_op_return_script_pubkey_sizes: Vec<u64>,
    /// Size of each observed OP_PUSHDATA* payload, in bytes.
    #[serde(default)]
    pub pushdata_sizes: Vec<u64>,
    /// Size of each observed witness element, in bytes.
    #[serde(default)]
    pub witness_element_sizes: Vec<u64>,
}

impl ObservedSizeMetadata {
    /// Creates an available observation from caller-supplied size metadata.
    pub fn available(
        op_return_script_sizes: Vec<u64>,
        non_op_return_script_pubkey_sizes: Vec<u64>,
        pushdata_sizes: Vec<u64>,
        witness_element_sizes: Vec<u64>,
    ) -> Self {
        Self {
            availability: ObservationAvailability::Available,
            op_return_script_sizes,
            non_op_return_script_pubkey_sizes,
            pushdata_sizes,
            witness_element_sizes,
        }
    }

    /// Creates an unavailable observation.
    pub fn unavailable() -> Self {
        Self {
            availability: ObservationAvailability::Unavailable,
            op_return_script_sizes: Vec::new(),
            non_op_return_script_pubkey_sizes: Vec::new(),
            pushdata_sizes: Vec::new(),
            witness_element_sizes: Vec::new(),
        }
    }

    /// Returns whether the observation can be assessed.
    pub fn is_available(&self) -> bool {
        self.availability == ObservationAvailability::Available
    }
}

impl Default for ObservedSizeMetadata {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// One of the fixed, low-cardinality observed-size rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSizeRule {
    /// OP_PUSHDATA* payload size.
    Pushdata,
    /// OP_RETURN output script size.
    OpReturnScript,
    /// Non-OP_RETURN output scriptPubKey size.
    NonOpReturnScriptPubkey,
    /// Witness element size.
    WitnessElement,
}

impl ObservedSizeRule {
    /// Returns the fixed Prometheus-safe label for this rule.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Pushdata => "pushdata",
            Self::OpReturnScript => "op_return_script",
            Self::NonOpReturnScriptPubkey => "non_op_return_script_pubkey",
            Self::WitnessElement => "witness_element",
        }
    }

    /// Returns the limit used for this observed-size rule.
    pub const fn limit(self) -> u64 {
        match self {
            Self::Pushdata => MAX_PUSHDATA_SIZE,
            Self::OpReturnScript => MAX_OP_RETURN_SCRIPT_SIZE,
            Self::NonOpReturnScriptPubkey => MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE,
            Self::WitnessElement => MAX_WITNESS_ELEMENT_SIZE,
        }
    }
}

/// A deterministic observed-size violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSizeViolation {
    /// The fixed rule that was exceeded.
    pub rule: ObservedSizeRule,
    /// The observed size in bytes.
    pub observed_size: u64,
    /// The rule limit in bytes.
    pub limit: u64,
    /// Zero-based position within the rule's corresponding input vector.
    pub index: usize,
}

/// Conservative classification for the partial observed-size assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSizeClassification {
    /// Every supplied observed size was within the four partial policy limits.
    WithinObservedSizeLimits,
    /// One or more supplied observed sizes exceeded a partial policy limit.
    ExceedsObservedSizeLimits,
    /// The required observation metadata was unavailable.
    Unknown,
}

impl ObservedSizeClassification {
    /// Returns the fixed Prometheus-safe label for this classification.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::WithinObservedSizeLimits => "within_observed_size_limits",
            Self::ExceedsObservedSizeLimits => "exceeds_observed_size_limits",
            Self::Unknown => "unknown",
        }
    }
}

/// Result of the partial, metadata-only observed-size assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSizePolicyAssessment {
    /// Conservative classification of the supplied observation.
    pub classification: ObservedSizeClassification,
    /// All deterministic violations, in canonical rule and input order.
    pub violations: Vec<ObservedSizeViolation>,
}

impl ObservedSizePolicyAssessment {
    /// Returns the fixed Prometheus-safe label for this assessment.
    pub const fn classification_label(&self) -> &'static str {
        self.classification.as_label()
    }
}

/// Assesses only the observed BIP-110 size limits represented by metadata.
///
/// The function does not parse transactions, infer omitted fields, consult a
/// Bitcoin node, or decide whether a transaction or block is valid under
/// consensus. An unavailable observation is always classified as `unknown`.
/// Violations are emitted in deterministic order: OP_RETURN scripts,
/// non-OP_RETURN scriptPubKeys, pushdata payloads, then witness elements; each
/// rule preserves the order of its corresponding input vector.
pub fn assess_observed_size_policy(
    metadata: &ObservedSizeMetadata,
) -> ObservedSizePolicyAssessment {
    if !metadata.is_available() {
        return ObservedSizePolicyAssessment {
            classification: ObservedSizeClassification::Unknown,
            violations: Vec::new(),
        };
    }

    let mut violations = Vec::new();
    append_violations(
        &mut violations,
        ObservedSizeRule::OpReturnScript,
        &metadata.op_return_script_sizes,
    );
    append_violations(
        &mut violations,
        ObservedSizeRule::NonOpReturnScriptPubkey,
        &metadata.non_op_return_script_pubkey_sizes,
    );
    append_violations(
        &mut violations,
        ObservedSizeRule::Pushdata,
        &metadata.pushdata_sizes,
    );
    append_violations(
        &mut violations,
        ObservedSizeRule::WitnessElement,
        &metadata.witness_element_sizes,
    );

    let classification = if violations.is_empty() {
        ObservedSizeClassification::WithinObservedSizeLimits
    } else {
        ObservedSizeClassification::ExceedsObservedSizeLimits
    };

    ObservedSizePolicyAssessment {
        classification,
        violations,
    }
}

fn append_violations(
    violations: &mut Vec<ObservedSizeViolation>,
    rule: ObservedSizeRule,
    observed_sizes: &[u64],
) {
    let limit = rule.limit();
    violations.extend(
        observed_sizes
            .iter()
            .enumerate()
            .filter(|(_, observed_size)| **observed_size > limit)
            .map(|(index, observed_size)| ObservedSizeViolation {
                rule,
                observed_size: *observed_size,
                limit,
                index,
            }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_observation(rule: ObservedSizeRule, size: u64) -> ObservedSizeMetadata {
        let mut metadata =
            ObservedSizeMetadata::available(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        match rule {
            ObservedSizeRule::Pushdata => metadata.pushdata_sizes.push(size),
            ObservedSizeRule::OpReturnScript => metadata.op_return_script_sizes.push(size),
            ObservedSizeRule::NonOpReturnScriptPubkey => {
                metadata.non_op_return_script_pubkey_sizes.push(size)
            }
            ObservedSizeRule::WitnessElement => metadata.witness_element_sizes.push(size),
        }
        metadata
    }

    #[test]
    fn exact_boundaries_are_within_and_one_above_exceeds() {
        for rule in [
            ObservedSizeRule::Pushdata,
            ObservedSizeRule::OpReturnScript,
            ObservedSizeRule::NonOpReturnScriptPubkey,
            ObservedSizeRule::WitnessElement,
        ] {
            let within = assess_observed_size_policy(&single_observation(rule, rule.limit()));
            assert_eq!(
                within.classification,
                ObservedSizeClassification::WithinObservedSizeLimits
            );
            assert!(within.violations.is_empty());

            let exceeds = assess_observed_size_policy(&single_observation(rule, rule.limit() + 1));
            assert_eq!(
                exceeds.classification,
                ObservedSizeClassification::ExceedsObservedSizeLimits
            );
            assert_eq!(
                exceeds.violations,
                vec![ObservedSizeViolation {
                    rule,
                    observed_size: rule.limit() + 1,
                    limit: rule.limit(),
                    index: 0,
                }]
            );
        }
    }

    #[test]
    fn multiple_inputs_return_all_violations_in_deterministic_order() {
        let metadata = ObservedSizeMetadata::available(
            vec![MAX_OP_RETURN_SCRIPT_SIZE + 1, MAX_OP_RETURN_SCRIPT_SIZE + 2],
            vec![MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE + 1],
            vec![MAX_PUSHDATA_SIZE + 1],
            vec![MAX_WITNESS_ELEMENT_SIZE + 1, MAX_WITNESS_ELEMENT_SIZE + 2],
        );

        let assessment = assess_observed_size_policy(&metadata);

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::ExceedsObservedSizeLimits
        );
        assert_eq!(
            assessment
                .violations
                .iter()
                .map(|violation| (violation.rule, violation.observed_size, violation.index))
                .collect::<Vec<_>>(),
            vec![
                (ObservedSizeRule::OpReturnScript, 84, 0),
                (ObservedSizeRule::OpReturnScript, 85, 1),
                (ObservedSizeRule::NonOpReturnScriptPubkey, 35, 0),
                (ObservedSizeRule::Pushdata, 257, 0),
                (ObservedSizeRule::WitnessElement, 257, 0),
                (ObservedSizeRule::WitnessElement, 258, 1),
            ]
        );
    }

    #[test]
    fn op_return_exception_is_not_assessed_against_non_op_return_limit() {
        let metadata = ObservedSizeMetadata::available(
            vec![MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE + 1],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let assessment = assess_observed_size_policy(&metadata);

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::WithinObservedSizeLimits
        );
        assert!(assessment.violations.is_empty());
    }

    #[test]
    fn unavailable_observation_is_unknown_without_violations() {
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::unavailable());

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::Unknown
        );
        assert!(assessment.violations.is_empty());
    }
}
