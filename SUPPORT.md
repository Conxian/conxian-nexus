# Conxian Nexus Support Guidance

Conxian Nexus is non-custodial, sovereign proof infrastructure operating on a fail-closed model. Public support expectations, severity classifications, and response SLAs are standardized according to the Support Expectations & Escalation SLA Matrix in [README.md](./README.md#support-expectations--escalation-sla-matrix) and summarized below.

## Support Tiers & Escalation Matrix

| Severity Level | Target Response | Target Resolution | Escalation Channel |
|---|---|---|---|
| **L3 - Critical** (Downtime, attestation failure, potential vulnerability) | < 2 Hours | < 12 Hours | Email [security@conxian-labs.com](mailto:security@conxian-labs.com) or PGP dispatch. |
| **L2 - Major** (Adapter sync drift > 2 blocks, Safety Mode, SRL-1 blocks) | < 6 Hours | < 24 Hours | Open a Technical Issue on GitHub; routed to `@Conxian/security-team`. |
| **L1 - Minor** (Local config, docs ambiguity, non-critical CLI/telemetry) | < 24 Hours | < 48 Hours | Open a General Issue on GitHub; routed to community maintainers. |

## Where to Get Help

- **General Support & Configuration:** Open a GitHub issue using the appropriate template.
- **Protocol or Workflow Bugs:** Submit a GitHub issue using the Bug Report template.
- **Governance or Policy Requests:** Submit a GitHub issue using the Governance Request template.
- **Operational Inquiries:** Email [support@conxian-labs.com](mailto:support@conxian-labs.com).

## Security Vulnerabilities

Do **not** report vulnerabilities in public GitHub issues.

Follow the private vulnerability reporting procedure detailed in [SECURITY.md](./SECURITY.md) (GitHub private advisories or direct email to `security@conxian-labs.com`).
