//! G5 cross-repo conformance: nexus `SubscriptionTier` (CON-24) ↔ ADR-004
//! `enterprise_subscription_cap` (lib-conxian-core `src/fee.rs`).
//!
//! Retain the local legacy vectors while also checking the current shared Core
//! billing catalog. Eligibility configuration does not establish payment.

use conxian_nexus::api::billing::SubscriptionTier;
use serde_json::Value;

#[test]
fn shared_catalog_matches_nexus_public_configuration_without_granting_authority() {
    use conxian_nexus::api::billing::{
        billing_catalog, ENTERPRISE_UPGRADE_SATS, INVOICE_EXPIRY_SECONDS, PRO_UPGRADE_SATS,
    };
    let catalog = billing_catalog();
    assert_eq!(catalog.nexus.upgrade_unit, "satoshis");
    assert_eq!(catalog.gateway.unit, "usd_cents");
    assert_eq!(catalog.nexus.invoice_expiry_seconds, INVOICE_EXPIRY_SECONDS);
    assert!(!catalog.confers_entitlement);
    assert_eq!(catalog.cross_product_entitlement, "unavailable");
    for row in catalog.nexus.tiers {
        let wire = serde_json::to_value(row.tier).unwrap();
        let local = SubscriptionTier::parse(wire.as_str().unwrap()).unwrap();
        assert_eq!(serde_json::to_value(local).unwrap(), wire);
        assert_eq!(local.signature_limit(), row.signature_limit);
        assert_eq!(
            local.enterprise_fee_cap(),
            row.enterprise_settlement_cap_eligible
        );
        assert_eq!(local.can_use_dlc(), row.tier.can_use_dlc());
        assert_eq!(local.can_use_zkml(), row.tier.can_use_zkml());
        assert_eq!(local.can_use_tableland(), row.tier.can_use_tableland());
        assert_eq!(local.can_use_bitvm(), row.tier.can_use_bitvm());
        assert_eq!(
            row.upgrade_amount,
            match local {
                SubscriptionTier::Free => 0,
                SubscriptionTier::Pro => PRO_UPGRADE_SATS,
                SubscriptionTier::Enterprise => ENTERPRISE_UPGRADE_SATS,
            }
        );
    }
}

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
