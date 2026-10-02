//! G5 cross-repo conformance: nexus `SubscriptionTier` (CON-24) ↔ ADR-004
//! `enterprise_subscription_cap` (lib-conxian-core `src/fee.rs`).
//!
//! The fixture is the cross-repo behavioral contract and MUST stay
//! byte-identical with the corresponding fixture in `lib-conxian-core`.

use conxian_nexus::api::billing::SubscriptionTier;
use serde_json::Value;

#[test]
fn billing_tier_maps_to_adr004_enterprise_cap() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/billing_fee_conformance.json"))
            .expect("fixture must parse");

    let cases = fixture["cases"]
        .as_array()
        .expect("fixture must contain a cases array");

    assert!(!cases.is_empty(), "fixture must contain at least one case");

    for case in cases {
        let tier = case["subscription_tier"].as_str().expect("tier field");
        let expected_cap = case["enterprise_subscription_cap"]
            .as_bool()
            .expect("cap field");
        let expected_limit = case["signature_limit"].as_u64().expect("limit field");

        let subscription = SubscriptionTier::parse(tier)
            .unwrap_or_else(|| panic!("unknown tier in fixture: {tier}"));

        assert_eq!(
            subscription.enterprise_fee_cap(),
            expected_cap,
            "enterprise_fee_cap mismatch for tier {tier}"
        );
        assert_eq!(
            subscription.signature_limit(),
            expected_limit,
            "signature_limit mismatch for tier {tier}"
        );
    }
}
