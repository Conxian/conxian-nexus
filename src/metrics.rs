//! Prometheus instrumentation for fixed-cardinality Nexus metrics.

use crate::sync::bip110::ObservedSizePolicyAssessment;
use lazy_static::lazy_static;
use prometheus::{opts, register_int_counter_vec, register_int_gauge, IntCounterVec, IntGauge};
use std::sync::Once;

/// Metric for partial BIP-110 observed-size assessment classifications.
pub const BIP110_OBSERVATIONS_ASSESSED_METRIC: &str = "nexus_bip110_observations_assessed_total";
/// Metric for partial BIP-110 observed-size violations by fixed rule.
pub const BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC: &str =
    "nexus_bip110_observed_size_violations_total";
/// Gauge for whether a BIP-110 observation backend is available.
pub const BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC: &str =
    "nexus_bip110_observation_backend_available";

const CLASSIFICATION_LABELS: [&str; 3] = [
    "within_observed_size_limits",
    "exceeds_observed_size_limits",
    "unknown",
];

const RULE_LABELS: [&str; 4] = [
    "pushdata",
    "op_return_script",
    "non_op_return_script_pubkey",
    "witness_element",
];

lazy_static! {
    static ref BIP110_OBSERVATIONS_ASSESSED: IntCounterVec = register_int_counter_vec!(
        opts!(
            BIP110_OBSERVATIONS_ASSESSED_METRIC,
            "Number of partial BIP-110 observed-size assessments by classification"
        ),
        &["classification"]
    )
    .expect("register BIP-110 observation assessment metric");
    static ref BIP110_OBSERVED_SIZE_VIOLATIONS: IntCounterVec = register_int_counter_vec!(
        opts!(
            BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC,
            "Number of partial BIP-110 observed-size violations by fixed rule"
        ),
        &["rule"]
    )
    .expect("register BIP-110 observed-size violation metric");
    static ref BIP110_OBSERVATION_BACKEND_AVAILABLE: IntGauge = register_int_gauge!(opts!(
        BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC,
        "Whether a BIP-110 observation backend is available (0 or 1)"
    ))
    .expect("register BIP-110 observation backend gauge");
}

static INIT_BIP110_METRICS: Once = Once::new();

/// Registers all fixed BIP-110 metric label values and initializes the
/// observation backend gauge to unavailable.
pub fn init_bip110_metrics() {
    INIT_BIP110_METRICS.call_once(|| {
        for classification in CLASSIFICATION_LABELS {
            let _ = BIP110_OBSERVATIONS_ASSESSED.with_label_values(&[classification]);
        }
        for rule in RULE_LABELS {
            let _ = BIP110_OBSERVED_SIZE_VIOLATIONS.with_label_values(&[rule]);
        }
        BIP110_OBSERVATION_BACKEND_AVAILABLE.set(0);
    });
}

/// Records a pure observed-size assessment without changing the assessment.
pub fn record_bip110_assessment(assessment: &ObservedSizePolicyAssessment) {
    init_bip110_metrics();

    BIP110_OBSERVATIONS_ASSESSED
        .with_label_values(&[assessment.classification_label()])
        .inc();

    for violation in &assessment.violations {
        BIP110_OBSERVED_SIZE_VIOLATIONS
            .with_label_values(&[violation.rule.as_label()])
            .inc();
    }
}

/// Sets whether a future observation backend is available to the recorder.
pub fn set_bip110_observation_backend_available(available: bool) {
    init_bip110_metrics();
    BIP110_OBSERVATION_BACKEND_AVAILABLE.set(i64::from(available));
}

/// Returns the fixed labels used for BIP-110 assessment classifications.
#[cfg(test)]
fn classification_labels() -> &'static [&'static str] {
    &CLASSIFICATION_LABELS
}

/// Returns the fixed labels used for BIP-110 violation rules.
#[cfg(test)]
fn rule_labels() -> &'static [&'static str] {
    &RULE_LABELS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::bip110::{
        assess_observed_size_policy, ObservedSizeClassification, ObservedSizeMetadata,
    };

    #[test]
    fn fixed_labels_are_registered_and_recordable() {
        init_bip110_metrics();
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::available(
            Vec::new(),
            vec![35],
            Vec::new(),
            Vec::new(),
        ));
        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::ExceedsObservedSizeLimits
        );
        record_bip110_assessment(&assessment);

        let families = prometheus::gather();
        let assessment_family = families
            .iter()
            .find(|family| family.name() == BIP110_OBSERVATIONS_ASSESSED_METRIC)
            .expect("assessment metric family should be registered");
        let violation_family = families
            .iter()
            .find(|family| family.name() == BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC)
            .expect("violation metric family should be registered");

        for classification in classification_labels() {
            assert!(assessment_family.get_metric().iter().any(|metric| {
                metric.get_label().iter().any(|label| {
                    label.name() == "classification" && label.value() == *classification
                })
            }));
        }
        for rule in rule_labels() {
            assert!(violation_family.get_metric().iter().any(|metric| {
                metric
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "rule" && label.value() == *rule)
            }));
        }
    }
}
