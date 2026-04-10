# Global Settlement Ingress [CON-160][CON-166]

Conxian Nexus provides an additive ingress layer for institutional settlement inputs, designed to bridge legacy TradFi rails with the sovereign Stacks L1 truth layer.

## Supported Protocols

- **ISO 20022**: Supports `pacs.008` (Customer Credit Transfer) and `pacs.009` (Financial Institution Credit Transfer).
- **PAPSS**: Pan-African Payment and Settlement System callback integration.
- **BRICS**: Regional settlement network ingestion.

## Security Floor

External triggers are strictly additive and **proposal-only**. They cannot directly execute contract logic.

1. **TEE Attestation**: Every trigger request must include a valid Trusted Execution Environment (TEE) attestation. The Nexus verifies this attestation before processing.
2. **Oracle Verification**: The `OracleAggregator` performs multi-source cross-verification of the signal (e.g., verifying exchange rates and amounts) before emitting a proposal.
3. **144-Block Time-lock**: Verified triggers initiate a mandatory 144-block time-lock in the `settlement_proposals` table. This allows for manual review or automated cancellation before any state-machine transition occurs.

## Auditability

Institutional-grade audit logs are maintained in the `cxn_external_settlement_logs` table:
- `external_tx_reference`: The original TradFi reference ID.
- `settlement_network_origin`: The source network (ISO20022, PAPSS, BRICS).
- `fiat_value_pegged`: The pegged value at the time of ingestion.
- `raw_payload`: The full normalized JSON payload for historical audit.

## API Endpoint

`POST /v1/settlement/trigger`

```json
{
  "source": "ISO20022",
  "external_id": "INST-SETTLE-12345",
  "payload": {
    "amount": 10000.00,
    "currency": "USD",
    "type": "pacs.008"
  },
  "attestation": "TEE_SIGNED_HW_..."
}
```
