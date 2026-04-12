# lib-conxian-core

Shared library for the Conxian ecosystem, providing core cryptographic primitives, wallet management, and protocol-specific service traits.

## Features

- **Wallet Management**: SECP256K1 (k256) based wallet with BIP32/BIP39 support.
- **Contract Bridge**: Utility for creating and signing Stacks (Clarity) contract calls.
- **Service Traits**: Unified traits for Bisq, RGB, and BitVM service implementations.
- **Hardened**: No panics in core logic; all operations return `Result` for safe error handling.

## Usage

### Wallet Creation

```rust
use lib_conxian_core::Wallet;

// Create a new random wallet or from environment NEXUS_PRIVATE_KEY
let wallet = Wallet::new().expect("Failed to create wallet");

// Derive from mnemonic
let mnemonic = "your twelve word mnemonic...";
let wallet = Wallet::from_mnemonic(mnemonic, "").expect("Invalid mnemonic");
```

### Signing

```rust
let sig = wallet.sign("message to sign");
```

## Security

This library is designed for mission-critical use. It avoids `unwrap()` and `expect()` in favor of explicit error handling. All cryptographic operations use standard, audited Rust crates (`k256`, `sha2`, etc.).

## License

BSL 1.1
