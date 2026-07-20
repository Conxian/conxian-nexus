//! Prometheus instrumentation for fixed-cardinality Nexus metrics.

use crate::sync::bip110::ObservedSizePolicyAssessment;
use lazy_static::lazy_static;
use prometheus::{opts, IntCounterVec, IntGauge, Registry};
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

const RULE_LABELS: [&str; 5] = [
    "pushdata",
    "op_return_script",
    "non_op_return_script_pubkey",
    "script_argument_witness_item",
    "taproot_control_block",
];

lazy_static! {
    /// Private registry containing only the intentionally exposed BIP-110 metrics.
    static ref BIP110_REGISTRY: Registry = Registry::new();
    static ref BIP110_OBSERVATIONS_ASSESSED: IntCounterVec = {
        let metric = IntCounterVec::new(
            opts!(
                BIP110_OBSERVATIONS_ASSESSED_METRIC,
                "Number of partial BIP-110 observed-size assessments by classification"
            ),
            &["classification"],
        )
        .expect("create BIP-110 observation assessment metric");
        BIP110_REGISTRY
            .register(Box::new(metric.clone()))
            .expect("register BIP-110 observation assessment metric");
        metric
    };
    static ref BIP110_OBSERVED_SIZE_VIOLATIONS: IntCounterVec = {
        let metric = IntCounterVec::new(
            opts!(
                BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC,
                "Number of partial BIP-110 observed-size violations by fixed rule"
            ),
            &["rule"],
        )
        .expect("create BIP-110 observed-size violation metric");
        BIP110_REGISTRY
            .register(Box::new(metric.clone()))
            .expect("register BIP-110 observed-size violation metric");
        metric
    };
    static ref BIP110_OBSERVATION_BACKEND_AVAILABLE: IntGauge = {
        let metric = IntGauge::with_opts(opts!(
            BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC,
            "Whether a BIP-110 observation backend is available (0 or 1)"
        ))
        .expect("create BIP-110 observation backend gauge");
        BIP110_REGISTRY
            .register(Box::new(metric.clone()))
            .expect("register BIP-110 observation backend gauge");
        metric
    };
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

/// Gathers only the dedicated BIP-110 metric families.
pub fn gather_bip110_metrics() -> Vec<prometheus::proto::MetricFamily> {
    init_bip110_metrics();
    BIP110_REGISTRY.gather()
}

/// Records a pure observed-size assessment without changing the assessment.
pub fn record_bip110_assessment(assessment: &ObservedSizePolicyAssessment) {
    init_bip110_metrics();

    BIP110_OBSERVATIONS_ASSESSED
        .with_label_values(&[assessment.classification.as_label()])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::bip110::{
        assess_observed_size_policy, ObservedSizeClassification, ObservedSizeItem,
        ObservedSizeMetadata,
    };
    use prometheus::{Encoder, IntGauge, TextEncoder};

    fn family<'a>(
        families: &'a [prometheus::proto::MetricFamily],
        name: &str,
    ) -> &'a prometheus::proto::MetricFamily {
        families
            .iter()
            .find(|family| family.name() == name)
            .unwrap_or_else(|| panic!("missing metric family {name}"))
    }

    #[test]
    fn fixed_labels_are_registered_and_recordable_in_dedicated_registry() {
        init_bip110_metrics();
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::NonOpReturnScriptPubkey { size: 35 },
        ]));
        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::ExceedsObservedSizeLimits
        );
        record_bip110_assessment(&assessment);

        let families = gather_bip110_metrics();
        assert_eq!(families.len(), 3);

        let assessment_family = family(&families, BIP110_OBSERVATIONS_ASSESSED_METRIC);
        let violation_family = family(&families, BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC);

        for classification in CLASSIFICATION_LABELS {
            assert!(assessment_family.get_metric().iter().any(|metric| {
                metric.get_label().iter().any(|label| {
                    label.name() == "classification" && label.value() == classification
                })
            }));
        }
        for rule in RULE_LABELS {
            assert!(violation_family.get_metric().iter().any(|metric| {
                metric
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "rule" && label.value() == rule)
            }));
        }
    }

    #[test]
    fn repeated_initialization_and_gather_are_safe() {
        init_bip110_metrics();
        init_bip110_metrics();
        let first = gather_bip110_metrics();
        let second = gather_bip110_metrics();

        assert_eq!(
            first.iter().map(|family| family.name()).collect::<Vec<_>>(),
            second
                .iter()
                .map(|family| family.name())
                .collect::<Vec<_>>(),
        );

        let encoder = TextEncoder::new();
        let mut first_bytes = Vec::new();
        let mut second_bytes = Vec::new();
        encoder
            .encode(&first, &mut first_bytes)
            .expect("first gather should encode");
        encoder
            .encode(&second, &mut second_bytes)
            .expect("second gather should encode");
        assert!(!first_bytes.is_empty());
        assert!(!second_bytes.is_empty());
    }

    #[test]
    fn backend_availability_gauge_is_recordable() {
        set_bip110_observation_backend_available(true);
        let enabled_families = gather_bip110_metrics();
        let enabled = family(
            &enabled_families,
            BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC,
        );
        assert_eq!(enabled.get_metric()[0].get_gauge().value(), 1.0);

        set_bip110_observation_backend_available(false);
        let disabled_families = gather_bip110_metrics();
        let disabled = family(
            &disabled_families,
            BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC,
        );
        assert_eq!(disabled.get_metric()[0].get_gauge().value(), 0.0);
    }

    #[test]
    fn dedicated_registry_does_not_collide_with_embedding_registry() {
        let embedding_registry = Registry::new();
        let unrelated_metric = IntGauge::new(
            BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC,
            "same name in an embedding application's registry",
        )
        .expect("unrelated metric should be constructible");
        embedding_registry
            .register(Box::new(unrelated_metric))
            .expect("private BIP-110 registry must not claim embedding registry names");

        assert!(!prometheus::gather().iter().any(|family| {
            family.name() == BIP110_OBSERVATIONS_ASSESSED_METRIC
                || family.name() == BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC
                || family.name() == BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC
        }));
    }
}
