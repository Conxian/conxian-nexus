//! Partial, metadata-only assessment of the observed BIP-110 size rules.
//!
//! This module deliberately accepts caller-supplied typed observations instead
//! of parsing raw Bitcoin transactions or connecting to a Bitcoin backend. It
//! is a conservative policy assessment, not consensus validation. It does not
//! implement deployment or activation state, UTXO grandfathering, Taproot
//! execution rules, proof-of-work or header verification, transaction
//! inclusion proofs, or a completeness guarantee beyond the explicit metadata
//! coverage supplied by the caller.
//!
//! The size exemptions represented here are only exemptions from the modeled
//! size checks. They do not imply that a transaction is valid under all of
//! BIP-110 or under Bitcoin consensus rules.

use serde::{Deserialize, Serialize};

/// Maximum observed OP_PUSHDATA* payload size in bytes for this partial policy.
pub const MAX_PUSHDATA_SIZE: u64 = 256;
/// Maximum observed OP_RETURN script size in bytes for this partial policy.
pub const MAX_OP_RETURN_SCRIPT_SIZE: u64 = 83;
/// Maximum observed non-OP_RETURN scriptPubKey size in bytes for this partial policy.
pub const MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE: u64 = 34;
/// Maximum script-argument witness item size in bytes for this partial policy.
pub const MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE: u64 = 256;
/// Maximum Taproot control-block size in bytes for this partial policy.
pub const MAX_TAPROOT_CONTROL_BLOCK_SIZE: u64 = 257;

/// Compatibility alias for callers that used the previous untyped name.
#[deprecated(note = "use MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE")]
pub const MAX_WITNESS_ELEMENT_SIZE: u64 = MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE;

/// Whether the caller supplied usable observation metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationAvailability {
    /// The observation backend supplied metadata to this module.
    Available,
    /// The observation backend or required metadata is unavailable.
    #[default]
    Unavailable,
}

/// Whether the supplied metadata covers all modeled observations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationCoverage {
    /// The caller asserts that all modeled categories in the assessed scope were observed.
    Complete,
    /// One or more modeled categories may be absent from the supplied metadata.
    #[default]
    Incomplete,
}

/// Categories that this partial assessor deliberately does not model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedObservedCategory {
    /// A witness version other than the modeled witness categories.
    UnknownWitnessVersion,
    /// A Taproot annex item, which is not assessed here.
    TaprootAnnex,
    /// Tapscript execution semantics, which are not assessed here.
    TapscriptExecution,
    /// Any other observed category not represented by this partial assessor.
    Other,
}

/// A typed observed item supplied to the partial BIP-110 assessor.
///
/// The typed variants prevent data that is exempt from one rule from being
/// accidentally assessed against another rule. In particular, BIP16
/// redeemScript pushes are distinct from rule-limited pushdata; witness
/// scripts and Tapleaf scripts are distinct from script-argument witness
/// items; and Taproot control blocks have their own 257-byte limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum ObservedSizeItem {
    /// An OP_RETURN output script subject to the 83-byte limit.
    OpReturnScript { size: u64 },
    /// A non-OP_RETURN output scriptPubKey subject to the 34-byte limit.
    NonOpReturnScriptPubkey { size: u64 },
    /// A rule-limited OP_PUSHDATA* payload subject to the 256-byte limit.
    Pushdata { size: u64 },
    /// A BIP16 redeemScript push, exempt from this 256-byte pushdata check.
    Bip16RedeemScriptPush { size: u64 },
    /// A script-argument witness item subject to the 256-byte limit.
    ScriptArgumentWitnessItem { size: u64 },
    /// A witness script, exempt from this partial 256-byte size check.
    WitnessScript { size: u64 },
    /// A Tapleaf script, exempt from this partial 256-byte size check.
    TapleafScript { size: u64 },
    /// A Taproot control block subject to the 257-byte limit.
    TaprootControlBlock { size: u64 },
    /// An observed category that prevents a complete within-limits conclusion.
    Unsupported { kind: UnsupportedObservedCategory },
}

impl ObservedSizeItem {
    fn modeled_rule(&self) -> Option<ObservedSizeRule> {
        match self {
            Self::OpReturnScript { .. } => Some(ObservedSizeRule::OpReturnScript),
            Self::NonOpReturnScriptPubkey { .. } => Some(ObservedSizeRule::NonOpReturnScriptPubkey),
            Self::Pushdata { .. } => Some(ObservedSizeRule::Pushdata),
            Self::ScriptArgumentWitnessItem { .. } => {
                Some(ObservedSizeRule::ScriptArgumentWitnessItem)
            }
            Self::TaprootControlBlock { .. } => Some(ObservedSizeRule::TaprootControlBlock),
            Self::Bip16RedeemScriptPush { .. }
            | Self::WitnessScript { .. }
            | Self::TapleafScript { .. }
            | Self::Unsupported { .. } => None,
        }
    }

    fn size(&self) -> Option<u64> {
        match self {
            Self::OpReturnScript { size }
            | Self::NonOpReturnScriptPubkey { size }
            | Self::Pushdata { size }
            | Self::Bip16RedeemScriptPush { size }
            | Self::ScriptArgumentWitnessItem { size }
            | Self::WitnessScript { size }
            | Self::TapleafScript { size }
            | Self::TaprootControlBlock { size } => Some(*size),
            Self::Unsupported { .. } => None,
        }
    }

    fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }
}

/// Caller-supplied typed observations used by the partial BIP-110 assessment.
///
/// `coverage` is an explicit fail-safe signal. `Complete` means the caller
/// asserts that every modeled category in scope was supplied, while
/// `Incomplete` means that an absence of an item cannot be interpreted as
/// evidence that the corresponding rule was satisfied. Empty observations
/// remain `Unknown`, even when availability and coverage are otherwise marked
/// as usable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSizeMetadata {
    /// Whether the observation backend supplied usable metadata.
    #[serde(default)]
    pub availability: ObservationAvailability,
    /// Whether all modeled categories in the assessed scope are covered.
    #[serde(default)]
    pub coverage: ObservationCoverage,
    /// Typed observed items in caller-provided order.
    #[serde(default)]
    pub observations: Vec<ObservedSizeItem>,
}

impl ObservedSizeMetadata {
    /// Creates complete, available metadata from typed observations.
    pub fn complete(observations: Vec<ObservedSizeItem>) -> Self {
        Self {
            availability: ObservationAvailability::Available,
            coverage: ObservationCoverage::Complete,
            observations,
        }
    }

    /// Creates complete, available metadata from typed observations.
    pub fn available(observations: Vec<ObservedSizeItem>) -> Self {
        Self::complete(observations)
    }

    /// Creates metadata that is available but does not cover all modeled categories.
    pub fn incomplete(observations: Vec<ObservedSizeItem>) -> Self {
        Self {
            availability: ObservationAvailability::Available,
            coverage: ObservationCoverage::Incomplete,
            observations,
        }
    }

    /// Creates unavailable metadata.
    pub fn unavailable() -> Self {
        Self {
            availability: ObservationAvailability::Unavailable,
            coverage: ObservationCoverage::Incomplete,
            observations: Vec::new(),
        }
    }

    /// Creates metadata with explicit availability and coverage indicators.
    pub fn new(
        availability: ObservationAvailability,
        coverage: ObservationCoverage,
        observations: Vec<ObservedSizeItem>,
    ) -> Self {
        Self {
            availability,
            coverage,
            observations,
        }
    }

    /// Returns whether the observation backend supplied metadata.
    pub fn is_available(&self) -> bool {
        self.availability == ObservationAvailability::Available
    }

    /// Returns whether the caller asserts complete coverage of modeled categories.
    pub fn is_complete(&self) -> bool {
        self.coverage == ObservationCoverage::Complete
    }

    fn can_classify_within_limits(&self) -> bool {
        self.is_available()
            && self.is_complete()
            && !self.observations.is_empty()
            && self
                .observations
                .iter()
                .all(|observation| !observation.is_unsupported())
    }
}

impl Default for ObservedSizeMetadata {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// A fixed modeled BIP-110 size rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSizeRule {
    /// Rule-limited OP_PUSHDATA* payload.
    Pushdata,
    /// OP_RETURN output script.
    OpReturnScript,
    /// Non-OP_RETURN output scriptPubKey.
    NonOpReturnScriptPubkey,
    /// Script-argument witness item.
    ScriptArgumentWitnessItem,
    /// Taproot control block.
    TaprootControlBlock,
}

impl ObservedSizeRule {
    /// Compatibility alias for the previous untyped witness rule name.
    #[deprecated(note = "use ScriptArgumentWitnessItem")]
    #[allow(non_upper_case_globals)]
    pub const WitnessElement: Self = Self::ScriptArgumentWitnessItem;

    /// Stable low-cardinality label for this rule.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Pushdata => "pushdata",
            Self::OpReturnScript => "op_return_script",
            Self::NonOpReturnScriptPubkey => "non_op_return_script_pubkey",
            Self::ScriptArgumentWitnessItem => "script_argument_witness_item",
            Self::TaprootControlBlock => "taproot_control_block",
        }
    }

    /// Maximum size for this modeled rule.
    pub const fn limit(self) -> u64 {
        match self {
            Self::Pushdata => MAX_PUSHDATA_SIZE,
            Self::OpReturnScript => MAX_OP_RETURN_SCRIPT_SIZE,
            Self::NonOpReturnScriptPubkey => MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE,
            Self::ScriptArgumentWitnessItem => MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE,
            Self::TaprootControlBlock => MAX_TAPROOT_CONTROL_BLOCK_SIZE,
        }
    }
}

/// A definite violation of one modeled size rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSizeViolation {
    /// The fixed rule that was exceeded.
    pub rule: ObservedSizeRule,
    /// The observed serialized size in bytes.
    pub observed_size: u64,
    /// The maximum serialized size for `rule` in this partial policy.
    pub limit: u64,
    /// Zero-based index into [`ObservedSizeMetadata::observations`].
    pub index: u64,
}

/// Classification of the modeled observed-size rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSizeClassification {
    /// Every supplied modeled observation was within its rule limit, and the
    /// metadata asserted complete non-empty coverage.
    WithinObservedSizeLimits,
    /// At least one definite modeled rule violation was observed.
    ExceedsObservedSizeLimits,
    /// The metadata was unavailable, incomplete, empty, or contained an
    /// unsupported category, so a within-limits conclusion is not justified.
    Unknown,
}

impl ObservedSizeClassification {
    /// Stable metric label for this classification.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::WithinObservedSizeLimits => "within_observed_size_limits",
            Self::ExceedsObservedSizeLimits => "exceeds_observed_size_limits",
            Self::Unknown => "unknown",
        }
    }
}

/// Result of the partial BIP-110 observed-size assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSizePolicyAssessment {
    /// Overall conservative classification.
    pub classification: ObservedSizeClassification,
    /// Explicit completeness indicator copied from the supplied metadata.
    pub coverage: ObservationCoverage,
    /// All definite modeled violations in deterministic rule order.
    pub violations: Vec<ObservedSizeViolation>,
}

impl ObservedSizePolicyAssessment {
    /// Stable metric label for this assessment's classification.
    pub const fn classification_label(&self) -> &'static str {
        self.classification.as_label()
    }
}

const RULE_ORDER: [ObservedSizeRule; 5] = [
    ObservedSizeRule::OpReturnScript,
    ObservedSizeRule::NonOpReturnScriptPubkey,
    ObservedSizeRule::Pushdata,
    ObservedSizeRule::ScriptArgumentWitnessItem,
    ObservedSizeRule::TaprootControlBlock,
];

/// Assesses the typed, caller-supplied observations against the modeled rules.
///
/// Definite violations are returned even when metadata is incomplete or
/// contains unsupported categories. Without such a violation, incomplete,
/// unavailable, empty, or unsupported observations fail safe to `Unknown`.
/// Exempt categories are intentionally ignored by these particular size rules;
/// that exemption does not establish overall BIP-110 or consensus validity.
pub fn assess_observed_size_policy(
    metadata: &ObservedSizeMetadata,
) -> ObservedSizePolicyAssessment {
    let mut violations = Vec::new();

    for rule in RULE_ORDER {
        let limit = rule.limit();
        for (index, observation) in metadata.observations.iter().enumerate() {
            if observation.modeled_rule() != Some(rule) {
                continue;
            }

            let observed_size = observation
                .size()
                .expect("modeled observation categories always carry a size");

            if observed_size > limit {
                violations.push(ObservedSizeViolation {
                    rule,
                    observed_size,
                    limit,
                    index: u64::try_from(index).expect("observation index fits in u64"),
                });
            }
        }
    }

    let classification = if !violations.is_empty() {
        ObservedSizeClassification::ExceedsObservedSizeLimits
    } else if metadata.can_classify_within_limits() {
        ObservedSizeClassification::WithinObservedSizeLimits
    } else {
        ObservedSizeClassification::Unknown
    };

    ObservedSizePolicyAssessment {
        classification,
        coverage: metadata.coverage,
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(rule: ObservedSizeRule, size: u64) -> ObservedSizeItem {
        match rule {
            ObservedSizeRule::Pushdata => ObservedSizeItem::Pushdata { size },
            ObservedSizeRule::OpReturnScript => ObservedSizeItem::OpReturnScript { size },
            ObservedSizeRule::NonOpReturnScriptPubkey => {
                ObservedSizeItem::NonOpReturnScriptPubkey { size }
            }
            ObservedSizeRule::ScriptArgumentWitnessItem => {
                ObservedSizeItem::ScriptArgumentWitnessItem { size }
            }
            ObservedSizeRule::TaprootControlBlock => ObservedSizeItem::TaprootControlBlock { size },
        }
    }

    fn complete_item(rule: ObservedSizeRule, size: u64) -> ObservedSizeMetadata {
        ObservedSizeMetadata::complete(vec![item(rule, size)])
    }

    #[test]
    fn prior_rule_boundaries_are_inclusive() {
        for (rule, limit) in [
            (ObservedSizeRule::Pushdata, MAX_PUSHDATA_SIZE),
            (ObservedSizeRule::OpReturnScript, MAX_OP_RETURN_SCRIPT_SIZE),
            (
                ObservedSizeRule::NonOpReturnScriptPubkey,
                MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE,
            ),
            (
                ObservedSizeRule::ScriptArgumentWitnessItem,
                MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE,
            ),
        ] {
            let assessment = assess_observed_size_policy(&complete_item(rule, limit));
            assert_eq!(
                assessment.classification,
                ObservedSizeClassification::WithinObservedSizeLimits
            );
            assert!(assessment.violations.is_empty());

            let assessment = assess_observed_size_policy(&complete_item(rule, limit + 1));
            assert_eq!(
                assessment.classification,
                ObservedSizeClassification::ExceedsObservedSizeLimits
            );
            assert_eq!(assessment.violations.len(), 1);
            assert_eq!(assessment.violations[0].index, 0);
            assert_eq!(assessment.violations[0].observed_size, limit + 1);
        }
    }

    #[test]
    fn op_return_exception_is_not_assessed_as_non_op_return() {
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::OpReturnScript { size: 83 },
            ObservedSizeItem::NonOpReturnScriptPubkey { size: 34 },
        ]));

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::WithinObservedSizeLimits
        );
        assert!(assessment.violations.is_empty());
    }

    #[test]
    fn redeem_script_push_larger_than_256_is_exempt_from_pushdata_rule() {
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::Bip16RedeemScriptPush { size: 257 },
        ]));

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::WithinObservedSizeLimits
        );
        assert!(assessment.violations.is_empty());
    }

    #[test]
    fn witness_script_and_tapleaf_larger_than_256_are_exempt_from_this_check() {
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::WitnessScript { size: 257 },
            ObservedSizeItem::TapleafScript { size: 258 },
        ]));

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::WithinObservedSizeLimits
        );
        assert!(assessment.violations.is_empty());
    }

    #[test]
    fn taproot_control_block_boundary_is_inclusive() {
        let within = assess_observed_size_policy(&complete_item(
            ObservedSizeRule::TaprootControlBlock,
            MAX_TAPROOT_CONTROL_BLOCK_SIZE,
        ));
        assert_eq!(
            within.classification,
            ObservedSizeClassification::WithinObservedSizeLimits
        );

        let exceeds = assess_observed_size_policy(&complete_item(
            ObservedSizeRule::TaprootControlBlock,
            MAX_TAPROOT_CONTROL_BLOCK_SIZE + 1,
        ));
        assert_eq!(
            exceeds.classification,
            ObservedSizeClassification::ExceedsObservedSizeLimits
        );
        assert_eq!(
            exceeds.violations[0].rule,
            ObservedSizeRule::TaprootControlBlock
        );
    }

    #[test]
    fn incomplete_empty_and_unavailable_metadata_fail_safe_to_unknown() {
        for metadata in [
            ObservedSizeMetadata::complete(Vec::new()),
            ObservedSizeMetadata::incomplete(vec![ObservedSizeItem::Pushdata { size: 1 }]),
            ObservedSizeMetadata::unavailable(),
        ] {
            let assessment = assess_observed_size_policy(&metadata);
            assert_eq!(
                assessment.classification,
                ObservedSizeClassification::Unknown
            );
            assert_eq!(assessment.coverage, metadata.coverage);
            assert!(assessment.violations.is_empty());
        }
    }

    #[test]
    fn unsupported_category_fails_safe_to_unknown() {
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::Pushdata { size: 1 },
            ObservedSizeItem::Unsupported {
                kind: UnsupportedObservedCategory::UnknownWitnessVersion,
            },
        ]));

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::Unknown
        );
        assert!(assessment.violations.is_empty());
    }

    #[test]
    fn definite_violation_wins_over_incomplete_or_unsupported_metadata() {
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::incomplete(vec![
            ObservedSizeItem::Pushdata {
                size: MAX_PUSHDATA_SIZE + 1,
            },
            ObservedSizeItem::Unsupported {
                kind: UnsupportedObservedCategory::Other,
            },
        ]));

        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::ExceedsObservedSizeLimits
        );
        assert_eq!(assessment.violations.len(), 1);
    }

    #[test]
    fn all_definite_violations_have_deterministic_rule_order() {
        let metadata = ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::TaprootControlBlock {
                size: MAX_TAPROOT_CONTROL_BLOCK_SIZE + 1,
            },
            ObservedSizeItem::Pushdata {
                size: MAX_PUSHDATA_SIZE + 1,
            },
            ObservedSizeItem::OpReturnScript {
                size: MAX_OP_RETURN_SCRIPT_SIZE + 1,
            },
            ObservedSizeItem::ScriptArgumentWitnessItem {
                size: MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE + 1,
            },
            ObservedSizeItem::NonOpReturnScriptPubkey {
                size: MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE + 1,
            },
        ]);

        let assessment = assess_observed_size_policy(&metadata);
        assert_eq!(
            assessment
                .violations
                .iter()
                .map(|violation| violation.rule)
                .collect::<Vec<_>>(),
            vec![
                ObservedSizeRule::OpReturnScript,
                ObservedSizeRule::NonOpReturnScriptPubkey,
                ObservedSizeRule::Pushdata,
                ObservedSizeRule::ScriptArgumentWitnessItem,
                ObservedSizeRule::TaprootControlBlock,
            ]
        );
        assert_eq!(
            assessment
                .violations
                .iter()
                .map(|violation| violation.index)
                .collect::<Vec<_>>(),
            vec![2, 4, 1, 3, 0]
        );
    }

    #[test]
    fn public_serde_model_uses_u64_indices_and_tagged_categories() {
        let assessment = assess_observed_size_policy(&complete_item(
            ObservedSizeRule::Pushdata,
            MAX_PUSHDATA_SIZE + 1,
        ));
        let encoded = serde_json::to_value(&assessment).expect("assessment should serialize");

        assert_eq!(encoded["violations"][0]["index"], 0);
        assert_eq!(
            serde_json::to_value(ObservedSizeItem::TaprootControlBlock { size: 257 })
                .expect("item should serialize")["category"],
            "taproot_control_block"
        );

        let decoded: ObservedSizePolicyAssessment =
            serde_json::from_value(encoded).expect("assessment should deserialize");
        assert_eq!(decoded, assessment);
    }

    #[test]
    #[allow(deprecated)]
    fn public_defaults_helpers_and_typed_categories_are_explicit() {
        assert_eq!(
            ObservationAvailability::default(),
            ObservationAvailability::Unavailable
        );
        assert_eq!(
            ObservationCoverage::default(),
            ObservationCoverage::Incomplete
        );

        let available =
            ObservedSizeMetadata::available(vec![ObservedSizeItem::Pushdata { size: 1 }]);
        assert!(available.is_available());
        assert!(available.is_complete());

        let explicit = ObservedSizeMetadata::new(
            ObservationAvailability::Available,
            ObservationCoverage::Complete,
            vec![ObservedSizeItem::TaprootControlBlock { size: 257 }],
        );
        assert!(explicit.is_available());
        assert!(explicit.is_complete());

        let default_metadata: ObservedSizeMetadata =
            serde_json::from_value(serde_json::json!({})).expect("defaults should deserialize");
        assert_eq!(default_metadata, ObservedSizeMetadata::unavailable());

        let typed_items = [
            (
                ObservedSizeItem::OpReturnScript { size: 1 },
                Some(ObservedSizeRule::OpReturnScript),
                Some(1),
            ),
            (
                ObservedSizeItem::NonOpReturnScriptPubkey { size: 2 },
                Some(ObservedSizeRule::NonOpReturnScriptPubkey),
                Some(2),
            ),
            (
                ObservedSizeItem::Pushdata { size: 3 },
                Some(ObservedSizeRule::Pushdata),
                Some(3),
            ),
            (
                ObservedSizeItem::Bip16RedeemScriptPush { size: 4 },
                None,
                Some(4),
            ),
            (
                ObservedSizeItem::ScriptArgumentWitnessItem { size: 5 },
                Some(ObservedSizeRule::ScriptArgumentWitnessItem),
                Some(5),
            ),
            (ObservedSizeItem::WitnessScript { size: 6 }, None, Some(6)),
            (ObservedSizeItem::TapleafScript { size: 7 }, None, Some(7)),
            (
                ObservedSizeItem::TaprootControlBlock { size: 8 },
                Some(ObservedSizeRule::TaprootControlBlock),
                Some(8),
            ),
        ];
        for (item, expected_rule, expected_size) in typed_items {
            assert_eq!(item.modeled_rule(), expected_rule);
            assert_eq!(item.size(), expected_size);
            assert!(!item.is_unsupported());
        }

        for kind in [
            UnsupportedObservedCategory::UnknownWitnessVersion,
            UnsupportedObservedCategory::TaprootAnnex,
            UnsupportedObservedCategory::TapscriptExecution,
            UnsupportedObservedCategory::Other,
        ] {
            let item = ObservedSizeItem::Unsupported { kind };
            assert_eq!(item.modeled_rule(), None);
            assert_eq!(item.size(), None);
            assert!(item.is_unsupported());
        }

        for (rule, label, limit) in [
            (ObservedSizeRule::Pushdata, "pushdata", MAX_PUSHDATA_SIZE),
            (
                ObservedSizeRule::OpReturnScript,
                "op_return_script",
                MAX_OP_RETURN_SCRIPT_SIZE,
            ),
            (
                ObservedSizeRule::NonOpReturnScriptPubkey,
                "non_op_return_script_pubkey",
                MAX_NON_OP_RETURN_SCRIPT_PUBKEY_SIZE,
            ),
            (
                ObservedSizeRule::ScriptArgumentWitnessItem,
                "script_argument_witness_item",
                MAX_SCRIPT_ARGUMENT_WITNESS_ITEM_SIZE,
            ),
            (
                ObservedSizeRule::TaprootControlBlock,
                "taproot_control_block",
                MAX_TAPROOT_CONTROL_BLOCK_SIZE,
            ),
        ] {
            assert_eq!(rule.as_label(), label);
            assert_eq!(rule.limit(), limit);
        }
        assert_eq!(
            ObservedSizeRule::WitnessElement,
            ObservedSizeRule::ScriptArgumentWitnessItem
        );

        for (classification, label) in [
            (
                ObservedSizeClassification::WithinObservedSizeLimits,
                "within_observed_size_limits",
            ),
            (
                ObservedSizeClassification::ExceedsObservedSizeLimits,
                "exceeds_observed_size_limits",
            ),
            (ObservedSizeClassification::Unknown, "unknown"),
        ] {
            assert_eq!(classification.as_label(), label);
            let assessment = ObservedSizePolicyAssessment {
                classification,
                coverage: ObservationCoverage::Complete,
                violations: Vec::new(),
            };
            assert_eq!(assessment.classification_label(), label);
        }
    }
}
