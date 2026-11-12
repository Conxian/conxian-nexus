# GitHub business operations workflow — conxian-nexus

**Adopted:** 2026-11-12 · **Authority:** owner decision (Business #1695) ·
**Parent:** [conxian-business#1695](https://github.com/Conxian/conxian-business/issues/1695) ·
**Issue:** [Conxian/conxian-nexus#520](https://github.com/Conxian/conxian-nexus/issues/520)

This repository adopts the organization-wide GitHub business-operations workflow.
The canonical, full definition is
[`conxian-business/docs/operations/GITHUB_BUSINESS_OPERATIONS_WORKFLOW.md`](https://github.com/Conxian/conxian-business/blob/main/docs/operations/GITHUB_BUSINESS_OPERATIONS_WORKFLOW.md).
This file records only how the workflow applies **in this repository**; it does not
restate or override the canonical contract.

## Scope in this repository

Business-linked work is coordinated through this repository's issues and linked to
the owning business issue (`conxian-business#1695` or a child). Implementation and
verification stay in this repository; the business outcome is recorded on the parent.

## Work item contract

Every material issue states: outcome and measurable acceptance; priority
(`P0`–`P3`); accountable owner; current state and dependencies; next action; an
evidence link or restricted-evidence reference; and privacy classification.

## Labels

The org business-ops label taxonomy is applied (`state:*`, `type:*`, `priority:*`,
`evidence:*`, `lifecycle:*`). A label is a coordination state only — it never
proves consent, payment, entitlement, adoption, revenue or SLA acceptance.

## Privacy boundary

Public issues may contain opaque identifiers, stage, owner role, due date,
sanitized outcome and evidence status. They must **not** contain names, personal
contact details, messages, contracts, negotiated pricing, tax identifiers,
bank/payment data or credentials. Use a restricted repository or approved private
system when even sanitized coordination would reveal the counterparty.

## Reuse the template

Open business-operations issues with the
[Business operations work item](../../.github/ISSUE_TEMPLATE/business_operations.yml)
form. It links the parent business issue and enforces the contract above.

## Verify / accept

Attach exact revision, CI status and sanitized runtime evidence; the accountable
owner confirms the business outcome, not merely the merge. A closed issue or a
green check does not establish payment, entitlement or adoption.
