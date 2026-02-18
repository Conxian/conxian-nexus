# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-02-17
### Added
- Initial implementation of Conxian Nexus (Glass Node).
- nexus-sync module for Stacks event ingestion.
- FSOC Sequencer for MEV mitigation.
- Sovereign Handoff safety monitor.
- REST and gRPC interfaces.
- PostgreSQL and Redis persistence layers.
- **Robust Merkle Tree implementation** for state root tracking and proof generation.
- **Full Wallet functionality** in lib-conxian-core using k256 (ECDSA).
- **Comprehensive /v1/status endpoint** for system monitoring.
- **Real-time drift simulation** and recovery logic in NexusSafety.
- **Integration tests** for "Root to leaf | leaf to root" verification.
