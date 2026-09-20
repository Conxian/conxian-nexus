//! Integration tests for multi-chain drift detection in the nexus-safety module.
//!
//! Nexus monitors drift between its processed state and the authoritative L1 tip
//! across the chains it observes. Safety mode is triggered only when drift is
//! strictly greater than `NexusSafety::MAX_DRIFT_BLOCKS` (2 blocks by default).
//! These tests exercise the pure drift arithmetic and threshold logic without
//! requiring a live Redis/PostgreSQL backend.

use conxian_nexus::safety::NexusSafety;

#[test]
fn drift_is_zero_when_heights_are_equal() {
    assert_eq!(NexusSafety::calculate_drift(42, 42), 0);
    assert_eq!(NexusSafety::calculate_drift(0, 0), 0);
}

#[test]
fn drift_measures_positive_lag_only() {
    // Processed height behind the tip: positive lag.
    assert_eq!(NexusSafety::calculate_drift(100, 97), 3);
    assert_eq!(NexusSafety::calculate_drift(100, 98), 2);
    assert_eq!(NexusSafety::calculate_drift(100, 99), 1);
    // Processed height equal to or ahead of the tip: no negative drift.
    assert_eq!(NexusSafety::calculate_drift(100, 100), 0);
    assert_eq!(NexusSafety::calculate_drift(100, 101), 0);
}

#[test]
fn safety_mode_is_not_triggered_within_two_blocks() {
    // The default threshold is 2 blocks; drift <= 2 stays healthy.
    assert!(!NexusSafety::drift_exceeded(
        0,
        NexusSafety::MAX_DRIFT_BLOCKS
    ));
    assert!(!NexusSafety::drift_exceeded(
        1,
        NexusSafety::MAX_DRIFT_BLOCKS
    ));
    assert!(!NexusSafety::drift_exceeded(
        2,
        NexusSafety::MAX_DRIFT_BLOCKS
    ));
}

#[test]
fn safety_mode_is_triggered_beyond_two_blocks() {
    // Any drift strictly greater than 2 blocks must trigger safety mode.
    for delta in 3..=64 {
        assert!(
            NexusSafety::drift_exceeded(delta, NexusSafety::MAX_DRIFT_BLOCKS),
            "drift of {delta} blocks must exceed the 2-block threshold"
        );
    }
}

#[test]
fn threshold_is_strict_and_not_inclusive() {
    // Exactly max_drift is healthy; one block more triggers.
    assert!(!NexusSafety::drift_exceeded(2, 2));
    assert!(NexusSafety::drift_exceeded(3, 2));
}

#[test]
fn threshold_honors_non_default_max_drift() {
    // A larger threshold allows more lag before triggering.
    assert!(!NexusSafety::drift_exceeded(10, 10));
    assert!(NexusSafety::drift_exceeded(11, 10));
    // A zero threshold triggers on any positive drift.
    assert!(!NexusSafety::drift_exceeded(0, 0));
    assert!(NexusSafety::drift_exceeded(1, 0));
}

#[test]
fn default_max_drift_is_two_blocks() {
    assert_eq!(NexusSafety::MAX_DRIFT_BLOCKS, 2);
}
