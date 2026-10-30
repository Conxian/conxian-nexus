# Nexus architecture

Nexus owns observation, state commitments, and proof verification for the public conxian.org protocol boundary. It is a Bitcoin-rooted universal-chain system. Managed node availability, operations, and SLAs belong to conxian-labs.com. Execution belongs to Gateway; hardware signing/custody verification belongs to the Enclave SDK and user-controlled providers.

There is no native Conxian token, tokenomics, or staking incentive. External chains, layers, assets, and protocol compatibility are retained. The repository's BUSL-1.1 license makes this implementation source-available; its public protocol role does not change the license into an OSI open-source license.

`sync` ingests observations; `state` commits MMR state; `verification` validates explicitly supported evidence; adapters and APIs expose bounded interfaces. SP1/RISC Zero interfaces cannot claim successful proof verification without a configured cryptographic backend and trusted program/image binding.

Diagnostic registration discovery is derived from actual typed components, with unknown health and no qualified operations. Exact chain/network/operation capability qualification requires provider authentication, fresh deployment-bound evidence, Core contract validation, and consumer policy. See [capability discovery](docs/CAPABILITY_DISCOVERY.md).

Admin observation APIs return unavailable evidence until backed by real providers. Fedimint's text prefix/hash path is not a cryptographic verifier and fails closed. A final BIP-340 signature proves validity under its supplied key; it does not establish independent FROST custody or authenticated group-key authority. These gaps remain tracked in Nexus #418.
