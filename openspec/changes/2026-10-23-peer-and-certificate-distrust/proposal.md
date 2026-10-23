# Patch peer reconciliation allocation and certificate distrust dependencies

Owner: Nexus #439 / #418; portfolio Conxian/conxian-business#1390. Source baseline: dev 57f6092ffe89f0316167268c6ba84b11163d2d29.

## Problem and acceptance

The current staged-to-main candidate #447 still fails its actual dependency audit on four advisories: negentropy 0.5.1 permits a peer-controlled allocation before validating an IdList payload; rustls-webpki 0.103.15 may accept distrusted intermediates; webpki-roots 1.0.9 and webpki-root-certs 1.0.9 retain distrusted emSign trust. Source metadata and a successful historical audit are insufficient.

Update only these existing lock entries to patched stable versions: negentropy 0.5.2, rustls-webpki 0.103.16, webpki-roots 1.0.10 and webpki-root-certs 1.0.10. Update the two explicit references to webpki-roots 1.0.9. Preserve all 502 package records, dependency edges other than those version references, git pins, manifests, features and security exceptions. Keep the compatibility wrapper webpki-roots 0.26.11. Verify actual crate checksums, locked resolution, formatting, strict Clippy and applicable tests; retain separate disposable-database conformance and runtime acceptance gates.

## Options considered

- Selected: upstream stable patch releases. They retain existing semver lines and dependency burden; published MSRVs (negentropy 1.51; the TLS packages 1.71) fit the repository's Rust 1.98.1 baseline. No new hosted resource or runtime cost is introduced.
- Rejected: advisory ignores, disabling certificate validation or suppressing reconciliation errors. These weaken security and cannot satisfy acceptance.
- Deferred: replacing TLS or Nostr/reconciliation stacks. This adds compatibility and maintenance burden without evidence that supported patch releases are insufficient.

## Security and scope

The upstream negentropy fix validates the declared ID count against remaining bytes before allocation and returns ParseEndsPrematurely for truncated messages. A message-size cap or catch_unwind alone cannot contain allocation aborts. Certificate parsing success does not replace signature or distrust validation. No new trust roots, keys, custody authority, production resource or provider qualification is granted.

This ordinary dev repair leaves #447 immutable. Require exact-head CI and genuine independent review, then normal dev -> staged -> soak -> complete applicable main acceptance. Existing audit ignores and warnings remain separate outstanding dispositions; no claim of zero findings, deployed remediation or release acceptance follows from a patch.

## Primary sources (read 23 October 2026)

- https://github.com/RustSec/advisory-db/blob/main/crates/negentropy/RUSTSEC-2026-0365.md
- https://github.com/nostrdevkit/negentropy/pull/14
- https://github.com/RustSec/advisory-db/blob/main/crates/rustls-webpki/RUSTSEC-2026-0382.md
- https://github.com/RustSec/advisory-db/blob/main/crates/webpki-roots/RUSTSEC-2026-0381.md
- https://github.com/RustSec/advisory-db/blob/main/crates/webpki-root-certs/RUSTSEC-2026-0383.md
- https://crates.io/api/v1/crates/negentropy/0.5.2
- https://crates.io/api/v1/crates/rustls-webpki/0.103.16
- https://crates.io/api/v1/crates/webpki-roots/1.0.10
- https://crates.io/api/v1/crates/webpki-root-certs/1.0.10
