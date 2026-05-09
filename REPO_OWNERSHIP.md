# Repo ownership

## Purpose

`conxian-nexus` is a supporting repo in the Conxian builder platform. Its allowed role is an external API facade or interoperability layer above the canonical adapter layer.

## This repo owns

- external-facing API facade behavior when distinct from raw adapter logic
- interoperability service boundaries that package lower-level gateway capabilities
- partner-facing or developer-facing API composition when intentionally higher-level than direct gateway adapters

## This repo does not own

- canonical network adapters
- provider-specific integration logic that belongs in `conxian-gateway`
- shared-core ownership
- protocol identity
- reference-client UI behavior

## Boundary rule

If the concern is about direct Bitcoin mainnet, Lightning, Stacks, Rootstock, or Liquid adapter behavior, it belongs in `conxian-gateway`. If the concern is about a higher-level API surface that packages those capabilities for external consumers, it may belong here.

## Strategic role

Supporting repo.