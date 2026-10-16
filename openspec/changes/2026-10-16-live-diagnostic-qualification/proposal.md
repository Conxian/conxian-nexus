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
