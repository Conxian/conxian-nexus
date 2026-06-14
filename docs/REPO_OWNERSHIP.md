# Repo ownership

## Purpose

`conxian-nexus` is a protocol-first "Glass Node" and proof layer. Its role is the primary observation, synchronization, and verification point for multi-chain state transitions (Tier 1 Chain Families).

## This repo owns

- protocol-first "Glass Node" implementation
- multi-chain state monitoring and normalization (Bitcoin, EVM, Cosmos)
- cryptographic state root commitments (MMR) and proof generation
- verifiable service interfaces (REST/gRPC) for off-chain state
- Lightning Resilience and Recovery Layer (SRL-1)

## This repo does not own

- canonical network adapters (handled by `conxian-gateway`)
- raw transaction construction for target chains
- protocol identity (handled by `lib-conxian-core`)
- reference-client UI behavior

## Boundary rule

If the concern is about direct network transport, RPC adaptation, or raw transaction assembly, it belongs in `conxian-gateway`. If the concern is about observing state, verifying proofs, or maintaining a verifiable synchronization layer across Tier 1 chains, it belongs here.

## Strategic role

Core Protocol Component (Glass Node).