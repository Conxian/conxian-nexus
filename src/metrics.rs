//! Prometheus instrumentation for fixed-cardinality Nexus metrics.

use crate::sync::bip110::ObservedSizePolicyAssessment;
use lazy_static::lazy_static;
use prometheus::{opts, Encoder, IntCounterVec, IntGauge, Registry, TextEncoder};

/// Metric for BIP-110 observed-size assessment classifications.
pub const BIP110_OBSERVATIONS_ASSESSED_METRIC: &str = "nexus_bip110_observations_assessed_total";
/// Metric for BIP-110 observed-size violations by fixed rule.
pub const BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC: &str =
    "nexus_bip110_observed_size_violations_total";
/// Gauge for whether a BIP-110 observation backend is available.
pub const BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC: &str =
    "nexus_bip110_observation_backend_available";
/// Prometheus text exposition content type returned by the REST endpoint.
pub const BIP110_PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4";

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

struct Bip110Metrics {
    registry: Registry,
    observations_assessed: IntCounterVec,
    observed_size_violations: IntCounterVec,
    observation_backend_available: IntGauge,
}

impl Bip110Metrics {
    fn new() -> prometheus::Result<Self> {
        let registry = Registry::new();
        let observations_assessed = IntCounterVec::new(
            opts!(
                BIP110_OBSERVATIONS_ASSESSED_METRIC,
                "Number of BIP-110 observed-size assessments by classification"
            ),
            &["classification"],
        )?;
        let observed_size_violations = IntCounterVec::new(
            opts!(
                BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC,
                "Number of BIP-110 observed-size violations by fixed rule"
            ),
            &["rule"],
        )?;
        let observation_backend_available = IntGauge::with_opts(opts!(
            BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC,
            "Whether a BIP-110 observation backend is available (0 or 1)"
        ))?;

        registry.register(Box::new(observations_assessed.clone()))?;
        registry.register(Box::new(observed_size_violations.clone()))?;
        registry.register(Box::new(observation_backend_available.clone()))?;

        for classification in CLASSIFICATION_LABELS {
            observations_assessed.with_label_values(&[classification]);
        }
        for rule in RULE_LABELS {
            observed_size_violations.with_label_values(&[rule]);
        }
        observation_backend_available.set(0);

        Ok(Self {
            registry,
            observations_assessed,
            observed_size_violations,
            observation_backend_available,
        })
    }

    fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }

    fn encode(&self) -> prometheus::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        TextEncoder::new().encode(&self.gather(), &mut buffer)?;
        Ok(buffer)
    }

    fn record_assessment(&self, assessment: &ObservedSizePolicyAssessment) {
        self.observations_assessed
            .with_label_values(&[assessment.classification.as_label()])
            .inc();

        for violation in &assessment.violations {
            self.observed_size_violations
                .with_label_values(&[violation.rule.as_label()])
                .inc();
        }
    }

    fn set_backend_available(&self, available: bool) {
        self.observation_backend_available.set(i64::from(available));
    }
}

lazy_static! {
    /// Private registry containing only the intentionally exposed BIP-110 metrics.
    static ref BIP110_METRICS: Bip110Metrics =
        Bip110Metrics::new().expect("create dedicated BIP-110 metrics registry");
}

/// Initializes all fixed BIP-110 metric series at zero.
pub fn init_bip110_metrics() {
    lazy_static::initialize(&BIP110_METRICS);
}

/// Gathers only the dedicated BIP-110 metric families.
pub fn gather_bip110_metrics() -> Vec<prometheus::proto::MetricFamily> {
    BIP110_METRICS.gather()
}

/// Encodes only the dedicated BIP-110 registry in Prometheus text format.
pub fn encode_bip110_metrics() -> prometheus::Result<Vec<u8>> {
    BIP110_METRICS.encode()
}

/// Records an observed-size assessment without changing the assessment.
pub fn record_bip110_assessment(assessment: &ObservedSizePolicyAssessment) {
    BIP110_METRICS.record_assessment(assessment);
}

/// Sets whether a future observation backend is available to the recorder.
pub fn set_bip110_observation_backend_available(available: bool) {
    BIP110_METRICS.set_backend_available(available);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::bip110::{
        assess_observed_size_policy, ObservedSizeClassification, ObservedSizeItem,
        ObservedSizeMetadata,
    };

    fn family<'a>(
        families: &'a [prometheus::proto::MetricFamily],
        name: &str,
    ) -> &'a prometheus::proto::MetricFamily {
        families
            .iter()
            .find(|family| family.name() == name)
            .unwrap_or_else(|| panic!("missing metric family {name}"))
    }

    fn label_value<'a>(metric: &'a prometheus::proto::Metric, name: &str) -> Option<&'a str> {
        metric
            .get_label()
            .iter()
            .find(|label| label.name() == name)
            .map(|label| label.value())
    }

    #[test]
    fn fixed_cardinality_series_are_preinitialized_to_zero() {
        let metrics = Bip110Metrics::new().expect("isolated metrics should initialize");
        let families = metrics.gather();
        assert_eq!(families.len(), 3);

        let assessments = family(&families, BIP110_OBSERVATIONS_ASSESSED_METRIC);
        assert_eq!(assessments.get_metric().len(), CLASSIFICATION_LABELS.len());
        for classification in CLASSIFICATION_LABELS {
            let metric = assessments
                .get_metric()
                .iter()
                .find(|metric| label_value(metric, "classification") == Some(classification))
                .unwrap_or_else(|| panic!("missing classification {classification}"));
            assert_eq!(metric.get_counter().value(), 0.0);
        }

        let violations = family(&families, BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC);
        assert_eq!(violations.get_metric().len(), RULE_LABELS.len());
        for rule in RULE_LABELS {
            let metric = violations
                .get_metric()
                .iter()
                .find(|metric| label_value(metric, "rule") == Some(rule))
                .unwrap_or_else(|| panic!("missing rule {rule}"));
            assert_eq!(metric.get_counter().value(), 0.0);
        }

        let backend = family(&families, BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC);
        assert_eq!(backend.get_metric()[0].get_gauge().value(), 0.0);
    }

    #[test]
    fn isolated_registry_records_assessments_and_backend_availability() {
        let metrics = Bip110Metrics::new().expect("isolated metrics should initialize");
        let assessment = assess_observed_size_policy(&ObservedSizeMetadata::complete(vec![
            ObservedSizeItem::NonOpReturnScriptPubkey { size: 35 },
        ]));
        assert_eq!(
            assessment.classification,
            ObservedSizeClassification::ExceedsObservedSizeLimits
        );

        metrics.record_assessment(&assessment);
        metrics.set_backend_available(true);
        let encoded = String::from_utf8(metrics.encode().expect("metrics should encode"))
            .expect("text format should be UTF-8");

        assert!(encoded.contains(
            "nexus_bip110_observations_assessed_total{classification=\"exceeds_observed_size_limits\"} 1"
        ));
        assert!(encoded.contains(
            "nexus_bip110_observed_size_violations_total{rule=\"non_op_return_script_pubkey\"} 1"
        ));
        assert!(encoded.contains("nexus_bip110_observation_backend_available 1"));
    }

    #[test]
    fn dedicated_registry_does_not_use_or_expose_default_process_metrics() {
        let metrics = Bip110Metrics::new().expect("isolated metrics should initialize");
        let encoded = String::from_utf8(metrics.encode().expect("metrics should encode"))
            .expect("text format should be UTF-8");

        assert!(encoded.contains(BIP110_OBSERVATIONS_ASSESSED_METRIC));
        assert!(!encoded.contains("process_"));
        assert!(!prometheus::gather().iter().any(|family| {
            family.name() == BIP110_OBSERVATIONS_ASSESSED_METRIC
                || family.name() == BIP110_OBSERVED_SIZE_VIOLATIONS_METRIC
                || family.name() == BIP110_OBSERVATION_BACKEND_AVAILABLE_METRIC
        }));
    }
}
