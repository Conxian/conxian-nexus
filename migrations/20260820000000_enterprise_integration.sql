-- Enterprise ERP & Blockchain Integration Schema Expansion (v0.4.23)

CREATE TABLE IF NOT EXISTS enterprise_iso20022_finality_events (
    uetr VARCHAR(64) PRIMARY KEY,
    msg_type VARCHAR(16) NOT NULL, -- pain.001 or pacs.008
    debtor_agent VARCHAR(64) NOT NULL,
    creditor_agent VARCHAR(64) NOT NULL,
    amount NUMERIC(18, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    settlement_status VARCHAR(32) NOT NULL,
    chain_target VARCHAR(32) NOT NULL, -- EVM / Bitcoin L2
    proof_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS enterprise_pos_settlement_commitments (
    commitment_id VARCHAR(64) PRIMARY KEY,
    merchant_id VARCHAR(64) NOT NULL,
    terminal_id VARCHAR(64) NOT NULL,
    transaction_count BIGINT NOT NULL,
    total_amount NUMERIC(18, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    mmr_pos BIGINT NOT NULL,
    mmr_root VARCHAR(66) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS enterprise_supply_provenance_hashes (
    po_number VARCHAR(64) PRIMARY KEY,
    document_hash VARCHAR(64) NOT NULL,
    supplier_id VARCHAR(64) NOT NULL,
    buyer_id VARCHAR(64) NOT NULL,
    mmr_pos BIGINT NOT NULL,
    mmr_root VARCHAR(66) NOT NULL,
    verified BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS enterprise_sme_escrow_sync (
    invoice_id VARCHAR(64) PRIMARY KEY,
    ubl_version VARCHAR(16) NOT NULL,
    escrow_contract_address VARCHAR(128) NOT NULL,
    chain_family VARCHAR(32) NOT NULL,
    escrow_status VARCHAR(32) NOT NULL,
    amount NUMERIC(18, 4) NOT NULL,
    currency VARCHAR(3) NOT NULL,
    l1_finality_height BIGINT NOT NULL,
    offchain_state_commitment VARCHAR(66) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS enterprise_compliance_zk_sanitized_states (
    case_id VARCHAR(64) PRIMARY KEY,
    postal_address_hash VARCHAR(64) NOT NULL,
    town_name_hash VARCHAR(64) NOT NULL,
    country_code VARCHAR(3) NOT NULL,
    sanitized_fields JSONB NOT NULL,
    zk_proof_hash VARCHAR(64) NOT NULL,
    verifier_contract VARCHAR(128) NOT NULL,
    verified BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
