//! [CON-1302 / ROAST] Robust Asynchronous Threshold Signatures (ROAST) Orchestrator.
//! Wraps FROST threshold signing rounds to guarantee liveness under asynchronous networks and malicious signers.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoastRoundStatus {
    Initializing,
    CollectingNonceShares,
    CollectingSignatureShares,
    Completed,
    Failed(String),
}

pub struct RoastSession {
    pub session_id: String,
    pub threshold: usize,
    pub total_participants: usize,
    pub active_participants: HashSet<u32>,
    pub faulty_participants: HashSet<u32>,
    pub signature_shares: HashMap<u32, Vec<u8>>,
    pub status: RoastRoundStatus,
}

impl RoastSession {
    pub fn new(session_id: String, threshold: usize, total_participants: usize) -> Self {
        let active_participants = (1..=total_participants as u32).collect();
        Self {
            session_id,
            threshold,
            total_participants,
            active_participants,
            faulty_participants: HashSet::new(),
            signature_shares: HashMap::new(),
            status: RoastRoundStatus::Initializing,
        }
    }

    pub fn submit_share(
        &mut self,
        participant_id: u32,
        share_bytes: Vec<u8>,
    ) -> Result<bool, String> {
        if self.faulty_participants.contains(&participant_id) {
            return Err("Participant marked as faulty".to_string());
        }

        if !self.active_participants.contains(&participant_id) {
            return Err("Participant not in active set".to_string());
        }

        if share_bytes.is_empty() {
            self.mark_participant_faulty(participant_id, "Submitted empty share".to_string());
            return Err("Empty share submitted".to_string());
        }

        self.signature_shares.insert(participant_id, share_bytes);

        if self.signature_shares.len() >= self.threshold {
            self.status = RoastRoundStatus::Completed;
            Ok(true)
        } else {
            self.status = RoastRoundStatus::CollectingSignatureShares;
            Ok(false)
        }
    }

    pub fn mark_participant_faulty(&mut self, participant_id: u32, reason: String) {
        self.active_participants.remove(&participant_id);
        self.faulty_participants.insert(participant_id);
        self.signature_shares.remove(&participant_id);

        if self.active_participants.len() < self.threshold {
            self.status = RoastRoundStatus::Failed(format!(
                "Active participants dropped below threshold {}: {}",
                self.threshold, reason
            ));
        }
    }
}
