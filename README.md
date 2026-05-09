# Conxian Nexus

Conxian Nexus is a supporting interoperability and API facade surface in the Conxian builder platform.

## Role

This repository exists to package lower-level platform capabilities into higher-level external-facing API or interoperability surfaces where that is useful for builders or partners.

## Owns

- higher-level API facade behavior
- interoperability service boundaries above direct adapters
- packaged access to lower-level capability surfaces when a dedicated API layer is justified

## Does not own

- canonical network adapters
- provider-specific integration logic that belongs in `conxian-gateway`
- shared-core ownership
- protocol identity

## Relationship to the rest of the portfolio

- `lib-conxian-core` defines shared capability interfaces and safety primitives
- `conxian-gateway` owns canonical network and provider adapters
- `conxius-enclave-sdk` owns secure signer and device trust abstractions
- `conxius-platform` composes the strategic repos into runtime and validation environments

This repo should stay clearly above the gateway layer rather than becoming a second adapter repository.
