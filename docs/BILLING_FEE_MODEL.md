# Billing ↔ Fee Model Alignment (G5)

This document links the two fee surfaces in the Conxian stack: the nexus
subscription billing tiers and the core per-settlement fee model.

## 1. Two fee surfaces

| Surface | System | What it gates |
|---------|--------|---------------|
| **Subscription tier** (CON-24) | `conxian-nexus` `api::billing::SubscriptionTier` | Monthly signature volume + feature access (DLC, ZKML, BitVM, Tableland) |
| **Per-settlement fee** (ADR-004) | `lib-conxian-core` `src/fee.rs` | Per-settlement price: `max(percentage, flat_floor) × load_factor` |

## 2. The link

An **Enterprise** subscriber has committed to a monthly use subscription
(1M sats/mo). Their per-settlement percentage component is therefore replaced
by the rail flat floor — they are never charged twice. This is encoded as
ADR-004 `FeeOptions::enterprise_subscription_cap`.

Mapping (implemented as `SubscriptionTier::enterprise_fee_cap()`):

| `SubscriptionTier` | `enterprise_subscription_cap` | Signature limit/mo |
|--------------------|-------------------------------|--------------------|
| `Free`             | `false`                       | 50,000             |
| `Pro`              | `false`                       | 500,000            |
| `Enterprise`       | `true`                        | 5,000,000          |

## 3. Cross-repo conformance

`tests/fixtures/billing_fee_conformance.json` is the canonical, byte-identical
conformance vector linking the nexus tiers to ADR-004. It is validated by
`tests/billing_fee_conformance.rs` and mirrored in `lib-conxian-core`.
