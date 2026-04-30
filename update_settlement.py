import sys

with open('src/api/settlement.rs', 'r') as f:
    content = f.read()

# Add imports if missing
if 'use crate::storage::kwil' not in content:
    content = content.replace('use sqlx::Row;', 'use sqlx::Row;\nuse crate::storage::kwil::{KwilSettlementProposalCommitment, KwilSettlementLogCommitment};')

# Insert Kwil logging and proposal persistence
log_insertion = """
    let external_tx_ref = uetr.or(e2e_id).unwrap_or(&payload.external_id).to_string();
    let _ = sqlx::query(
        "INSERT INTO cxn_external_settlement_logs (external_tx_reference, settlement_network_origin, fiat_value_pegged, raw_payload)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(&external_tx_ref)
    .bind(&payload.source)
    .bind(fiat_value)
    .bind(&payload.payload)
    .execute(&state.storage.pg_pool)
    .await;

    // [CON-330] Pilot: Mirror settlement log to Kwil
    if let Some(kwil) = &state.kwil {
        let _ = kwil.persist_settlement_log(KwilSettlementLogCommitment {
            external_tx_reference: external_tx_ref,
            settlement_network_origin: payload.source.clone(),
            fiat_value_pegged: fiat_value,
            raw_payload: payload.payload.clone(),
        }).await.map_err(|e| tracing::warn!("Kwil settlement log persistence failed: {}", e)).ok();
    }
"""

content = content.replace("""    let _ = sqlx::query(
        "INSERT INTO cxn_external_settlement_logs (external_tx_reference, settlement_network_origin, fiat_value_pegged, raw_payload)
         VALUES (, , , )"
    )
    .bind(uetr.or(e2e_id).unwrap_or(&payload.external_id))
    .bind(&payload.source)
    .bind(fiat_value)
    .bind(&payload.payload)
    .execute(&state.storage.pg_pool)
    .await;""", log_insertion)

proposal_insertion = """
    // 5. Persist the proposal as "proposal-only"
    let res = sqlx::query(
        "INSERT INTO settlement_proposals (proposal_id, external_id, source, payload, status, init_height, unlock_height)
         VALUES ($1, $2, $3, $4, 'active', $5, $6)"
    )
    .bind(&proposal_id)
    .bind(&payload.external_id)
    .bind(&payload.source)
    .bind(&payload.payload)
    .bind(current_height)
    .bind(unlock_height as i64)
    .execute(&state.storage.pg_pool)
    .await;

    // [CON-330] Pilot: Mirror settlement proposal to Kwil
    if let Some(kwil) = &state.kwil {
        let _ = kwil.persist_settlement_proposal(KwilSettlementProposalCommitment {
            proposal_id: proposal_id.clone(),
            external_id: payload.external_id.clone(),
            source: payload.source.clone(),
            payload: payload.payload.clone(),
            status: "active".to_string(),
            init_height: current_height,
            unlock_height: unlock_height as i64,
        }).await.map_err(|e| tracing::warn!("Kwil settlement proposal persistence failed: {}", e)).ok();
    }
"""

content = content.replace("""    // 5. Persist the proposal as "proposal-only"
    let res = sqlx::query(
        "INSERT INTO settlement_proposals (proposal_id, external_id, source, payload, status, init_height, unlock_height)
         VALUES (, , , , 'active', , )"
    )
    .bind(&proposal_id)
    .bind(&payload.external_id)
    .bind(&payload.source)
    .bind(&payload.payload)
    .bind(current_height)
    .bind(unlock_height as i64)
    .execute(&state.storage.pg_pool)
    .await;""", proposal_insertion)

with open('src/api/settlement.rs', 'w') as f:
    f.write(content)
