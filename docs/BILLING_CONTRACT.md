# Pricing architecture: see Conxian/conxian-business ADR-009 — nexus tiers are
# gateway entitlements (signature-volume seats), not a separate bill.

# Shared billing configuration integration

Tracks Business [#1343](https://github.com/Conxian/conxian-business/issues/1343)
and [#1390](https://github.com/Conxian/conxian-business/issues/1390).

Nexus retains `api::billing::SubscriptionTier` as its own public Rust enum with
the same serde values, parsing behavior and API paths. Its quota/feature
configuration delegates through an explicit mapping to Core's versioned
definitions. Existing upgrade amounts and invoice expiry are re-exported from
the same pinned Core source. Payment, tenant authentication, metering and key
enforcement remain Nexus-owned; they are not moved into Core.

`api::billing::billing_catalog()` exposes configuration only. Gateway managed
service charges (USD cents), Nexus signature quotas/satoshi upgrade amounts,
Market settlement fees and payment-backed entitlement remain distinct.
`enterprise_fee_cap()` preserves ADR-004 eligibility behavior and does not prove
payment, an expiry/renewal term, Gateway subscription status or provider support.
There is no automatic cross-product entitlement mapping.

The manifest, lockfile and dependency-policy/compliance scripts identify the
same exact Core commit. Existing behavioral fixtures and new catalog
conformance checks cover quotas, tier wire values, feature eligibility and
upgrade amounts. The independent published-version companion adapter remains
on its explicit SDK =2.0.17 contract upstream; no production qualification or
new signing feature is implied by the development dependency update.

The separate sprint-exit economics/pricing/upstream-sustainability research is
Business [#1419](https://github.com/Conxian/conxian-business/issues/1419).
Commercial pricing changes, durable cross-product entitlement and reviewed
release acceptance remain open with their existing owners.
