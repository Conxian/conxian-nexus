# Qualify live scoring and SLA diagnostics

Refs: Conxian/conxian-business#1429 and Conxian/conxian-business#1390.

The new WPS and multi-cloud SLA diagnostic surfaces lack qualified event and
provider observations. Keep their routes and response models but return HTTP 503
until source-bound evidence and the relevant policy are implemented and accepted.
Do not synthesize health, settled volume, financial actuals or a valuation from
missing observations. Do not alter billing tiers, fees or allocation policy.

Acceptance: both handlers return unavailable without querying an absent database;
existing analytics and billing behavior passes its regression suites. Future live
integration must have authenticated source identity, exact asset units, sampling
windows, finality/replay/durability and explicit financial-policy acceptance.

Run the existing read-only Rust workflow for dev pushes and PRs as well as
staged/main, so feature integration receives actual formatting, strict lint,
build and workspace regression checks before promotion. Existing jobs and
permissions are preserved; no branch-protection or acceptance rule is changed.

Native CI exposed stale numeric Lightning coverage ranges after source movement.
Resolve the same billing/auth/quota, Nostr, DLC and REST blocks from unique source
anchors, and include the full Lightning recovery adapter rather than truncating it
at line 140. Missing or ambiguous anchors fail the gate. Exercise recovery refusal,
ambiguous-payment reconciliation and independent pending/split clocks. Enforce the
documented 90 percent minimum; retain Bitcoin's existing 92 percent requirement.
