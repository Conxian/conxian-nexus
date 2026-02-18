pub fn sign_transaction(tx_id: &str) -> String {
    format!("signed_{}", tx_id)
}
