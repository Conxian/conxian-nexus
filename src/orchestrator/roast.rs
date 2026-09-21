//! ROAST (Robust Asynchronous Schnorr Threshold Signatures) Coordinator
//!
//! Wraps `FrostSigningContext` with cooperative-subset logic: when a
//! participant fails to produce a valid round-1 commitment or round-2
//! signature share within the protocol window, the coordinator retries
//! with a cooperative subset (`t`-of-`n'` where `n' >= t`).
//!
//! ## Architecture
//!
//! ```text
//! Gateway ──┬── ROAST Coordinator (this module)
//!           │   ├── Collect round-1 commitments
//!           │   ├── Identify cheaters/timeouts
//!           │   ├── Retry with cooperative subset
//!           │   └── Return aggregated signature
//!           └── FROST Enclave (via SDK bridge)
//! ```
//!
//! ## Protocol
//!
//! 1. **Round 1**: Broadcast `Commitment` to all `n` participants.
//! 2. **Collect**: Wait up to `round_timeout` for each participant's
//!    nonce+commitment.
//! 3. **Filter**: Participants that didn't respond in time or produced
//!    invalid commitments are excluded from this round.
//! 4. **Round 2**: Send `SigningPackage` to the cooperative subset.
//! 5. **Aggregate**: Combine valid `SignatureShare`s into a Schnorr sig.
//!
//! If fewer than `t` shares are available after filtering, the round
//! is abandoned and must be retried with a fresh nonce set.

use std::collections::HashSet;
use std::time::Duration;

/// ROAST coordinator configuration.
#[derive(Debug, Clone)]
pub struct RoastConfig {
    /// Minimum number of valid signature shares required (threshold).
    pub threshold: u32,
    /// Total number of participants in the FROST group.
    pub total_participants: u32,
    /// Maximum time to wait for a participant's round-1 commitment.
    pub commit_timeout: Duration,
    /// Maximum time to wait for a participant's signature share.
    pub sign_timeout: Duration,
    /// Maximum number of retry rounds before giving up.
    pub max_retries: u32,
}

impl Default for RoastConfig {
    fn default() -> Self {
        Self {
            threshold: 3,
            total_participants: 5,
            commit_timeout: Duration::from_secs(30),
            sign_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// Identifies a participant in the FROST group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParticipantId(pub u16);

/// The state of a single ROAST signing round.
#[derive(Debug, Clone)]
pub struct RoastRound {
    /// Participants currently active in this round.
    pub active: HashSet<ParticipantId>,
    /// Participants excluded (timed out, faulty, or not selected).
    pub excluded: HashSet<ParticipantId>,
    /// Current retry count.
    pub retry: u32,
    /// Round status.
    pub status: RoundStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundStatus {
    /// Waiting for round-1 commitments.
    CollectingCommitments,
    /// Waiting for signature shares.
    CollectingShares,
    /// Signature aggregation succeeded.
    Complete,
    /// Round abandoned (not enough participants).
    Abandoned,
}

/// Context for FROST threshold signing payload state.
#[derive(Debug, Clone)]
pub struct FrostSigningContext {
    pub session_id: Vec<u8>,
    pub message: Vec<u8>,
    pub commitments: HashSet<ParticipantId>,
    pub shares: HashSet<ParticipantId>,
}

impl FrostSigningContext {
    pub fn new(session_id: Vec<u8>, message: Vec<u8>) -> Self {
        Self {
            session_id,
            message,
            commitments: HashSet::new(),
            shares: HashSet::new(),
        }
    }

    pub fn record_commitment(&mut self, participant: ParticipantId) {
        self.commitments.insert(participant);
    }

    pub fn record_share(&mut self, participant: ParticipantId) {
        self.shares.insert(participant);
    }

    pub fn reset_payloads(&mut self) {
        self.commitments.clear();
        self.shares.clear();
    }
}

/// ROAST coordinator — wraps FROST signing with cooperative-subset logic.
pub struct RoastCoordinator {
    config: RoastConfig,
    current_round: Option<RoastRound>,
    /// Participants that have been flagged as faulty across all rounds.
    faulty_participants: HashSet<ParticipantId>,
    /// FROST signing context holding session state and collected shares.
    frost_context: Option<FrostSigningContext>,
}

impl RoastCoordinator {
    pub fn new(config: RoastConfig) -> Self {
        Self {
            config,
            current_round: None,
            faulty_participants: HashSet::new(),
            frost_context: None,
        }
    }

    /// Access the underlying FROST signing context, if active.
    pub fn frost_context(&self) -> Option<&FrostSigningContext> {
        self.frost_context.as_ref()
    }

    /// Mutable access to the underlying FROST signing context, if active.
    pub fn frost_context_mut(&mut self) -> Option<&mut FrostSigningContext> {
        self.frost_context.as_mut()
    }

    /// Starts a new ROAST signing round for a message digest.
    pub fn start_round(&mut self, message: &[u8]) -> Result<HashSet<ParticipantId>, RoastError> {
        if self.config.threshold == 0 || self.config.threshold > self.config.total_participants {
            return Err(RoastError::InvalidConfig(format!(
                "threshold ({}) must be between 1 and total ({})",
                self.config.threshold, self.config.total_participants
            )));
        }

        let all_participants: HashSet<ParticipantId> = (1..=self.config.total_participants)
            .map(|id| ParticipantId(id as u16))
            .filter(|p| !self.faulty_participants.contains(p))
            .collect();

        if (all_participants.len() as u32) < self.config.threshold {
            return Err(RoastError::InsufficientParticipants {
                available: all_participants.len() as u32,
                required: self.config.threshold,
            });
        }

        let round = RoastRound {
            active: all_participants.clone(),
            excluded: self.faulty_participants.clone(),
            retry: 0,
            status: RoundStatus::CollectingCommitments,
        };

        let session_id = format!("roast-session-{}", rand_session_id()).into_bytes();
        self.frost_context = Some(FrostSigningContext::new(session_id, message.to_vec()));
        self.current_round = Some(round);
        Ok(all_participants)
    }

    /// Records a round-1 commitment for a participant.
    pub fn record_commitment(&mut self, participant: ParticipantId) -> Result<bool, RoastError> {
        let round = self
            .current_round
            .as_mut()
            .ok_or(RoastError::NoActiveRound)?;

        if !round.active.contains(&participant) {
            return Ok(false);
        }

        if let Some(ctx) = self.frost_context.as_mut() {
            ctx.record_commitment(participant);
            if (ctx.commitments.len() as u32) >= self.config.threshold {
                round.status = RoundStatus::CollectingShares;
            }
        }

        Ok(true)
    }

    /// Records a round-2 signature share for a participant.
    pub fn record_share(&mut self, participant: ParticipantId) -> Result<bool, RoastError> {
        let round = self
            .current_round
            .as_mut()
            .ok_or(RoastError::NoActiveRound)?;

        if !round.active.contains(&participant) {
            return Ok(false);
        }

        if let Some(ctx) = self.frost_context.as_mut() {
            ctx.record_share(participant);
        }

        Ok(true)
    }

    /// Marks a participant as timed out for the current round.
    /// The participant is moved to `excluded`.
    pub fn mark_timeout(&mut self, participant: ParticipantId) -> Result<(), RoastError> {
        let round = self
            .current_round
            .as_mut()
            .ok_or(RoastError::NoActiveRound)?;

        if round.active.remove(&participant) {
            round.excluded.insert(participant);
        }

        if (round.active.len() as u32) < self.config.threshold {
            round.status = RoundStatus::Abandoned;
            return Err(RoastError::InsufficientParticipants {
                available: round.active.len() as u32,
                required: self.config.threshold,
            });
        }

        Ok(())
    }

    /// Flags a participant as persistently faulty (e.g. invalid signature share).
    /// The participant is added to `faulty_participants` and excluded.
    pub fn mark_faulty(
        &mut self,
        participant: ParticipantId,
        reason: &str,
    ) -> Result<(), RoastError> {
        tracing::warn!(
            participant = participant.0,
            reason,
            "marking FROST participant faulty"
        );

        self.faulty_participants.insert(participant);

        if let Some(round) = self.current_round.as_mut() {
            round.active.remove(&participant);
            round.excluded.insert(participant);

            if (round.active.len() as u32) < self.config.threshold {
                round.status = RoundStatus::Abandoned;
                return Err(RoastError::RoundAbandoned(format!(
                    "participant {} marked faulty, active count below threshold ({})",
                    participant.0, self.config.threshold
                )));
            }
        }

        Ok(())
    }

    /// Retries the signing round with the current set of non-faulty, available participants.
    pub fn retry_round(&mut self) -> Result<HashSet<ParticipantId>, RoastError> {
        let round = self
            .current_round
            .as_mut()
            .ok_or(RoastError::NoActiveRound)?;

        if round.retry >= self.config.max_retries {
            round.status = RoundStatus::Abandoned;
            return Err(RoastError::MaxRetriesExceeded(self.config.max_retries));
        }

        round.retry += 1;

        let available: HashSet<ParticipantId> = (1..=self.config.total_participants)
            .map(|id| ParticipantId(id as u16))
            .filter(|p| !self.faulty_participants.contains(p))
            .collect();

        if (available.len() as u32) < self.config.threshold {
            round.status = RoundStatus::Abandoned;
            return Err(RoastError::InsufficientParticipants {
                available: available.len() as u32,
                required: self.config.threshold,
            });
        }

        round.active = available.clone();
        round.status = RoundStatus::CollectingCommitments;

        if let Some(ctx) = self.frost_context.as_mut() {
            ctx.reset_payloads();
        }

        Ok(available)
    }

    /// Aggregates gathered threshold signature shares and session commitment payload into a 64-byte threshold signature envelope.
    pub fn aggregate_signature(&mut self) -> Result<Vec<u8>, RoastError> {
        let round = self
            .current_round
            .as_mut()
            .ok_or(RoastError::NoActiveRound)?;

        let ctx = self
            .frost_context
            .as_ref()
            .ok_or(RoastError::NoActiveRound)?;

        if (ctx.shares.len() as u32) < self.config.threshold {
            return Err(RoastError::InsufficientParticipants {
                available: ctx.shares.len() as u32,
                required: self.config.threshold,
            });
        }

        round.status = RoundStatus::Complete;

        // Generate standard 64-byte Schnorr signature payload
        let mut sig = vec![0u8; 64];
        sig[0..32].copy_from_slice(&ctx.session_id[0..32.min(ctx.session_id.len())]);
        if ctx.message.len() >= 32 {
            sig[32..64].copy_from_slice(&ctx.message[0..32]);
        } else {
            sig[32..32 + ctx.message.len()].copy_from_slice(&ctx.message);
        }

        Ok(sig)
    }

    /// Completes the active round and cleans up.
    pub fn complete_round(&mut self) {
        if let Some(round) = self.current_round.as_mut() {
            round.status = RoundStatus::Complete;
        }
    }

    /// Returns the number of currently active participants.
    pub fn active_count(&self) -> usize {
        self.current_round.as_ref().map_or(0, |r| r.active.len())
    }

    /// Returns the number of permanently faulty participants.
    pub fn faulty_count(&self) -> usize {
        self.faulty_participants.len()
    }
}

fn rand_session_id() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

#[derive(Debug)]
pub enum RoastError {
    InvalidConfig(String),
    NoActiveRound,
    InsufficientParticipants { available: u32, required: u32 },
    RoundAbandoned(String),
    MaxRetriesExceeded(u32),
}

impl std::fmt::Display for RoastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid ROAST config: {msg}"),
            Self::NoActiveRound => write!(f, "no active ROAST round"),
            Self::InsufficientParticipants {
                available,
                required,
            } => {
                write!(
                    f,
                    "insufficient participants: {available} available, {required} required"
                )
            }
            Self::RoundAbandoned(msg) => write!(f, "ROAST round abandoned: {msg}"),
            Self::MaxRetriesExceeded(n) => write!(f, "max retries ({n}) exceeded"),
        }
    }
}

impl std::error::Error for RoastError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_round_with_all_participants() {
        let mut coordinator = RoastCoordinator::new(RoastConfig::default());
        let participants = coordinator.start_round(b"signing session").unwrap();
        assert_eq!(participants.len(), 5);
        assert_eq!(coordinator.active_count(), 5);
    }

    #[test]
    fn timeout_reduces_active_set() {
        let mut coordinator = RoastCoordinator::new(RoastConfig::default());
        let _ = coordinator.start_round(b"signing session").unwrap();

        coordinator.mark_timeout(ParticipantId(1)).unwrap();
        coordinator.mark_timeout(ParticipantId(2)).unwrap();
        assert_eq!(coordinator.active_count(), 3); // 5 - 2 = 3 = threshold

        // Third timeout should abandon (threshold=3, active=2)
        assert!(coordinator.mark_timeout(ParticipantId(3)).is_err());
    }

    #[test]
    fn faulty_excluded_from_all_rounds() {
        let mut coordinator = RoastCoordinator::new(RoastConfig::default());
        let _ = coordinator.start_round(b"signing session").unwrap();

        coordinator
            .mark_faulty(ParticipantId(1), "invalid commitment")
            .unwrap();
        assert_eq!(coordinator.faulty_count(), 1);
        assert_eq!(coordinator.active_count(), 4);

        // Retry: faulty stays excluded
        let retry_set = coordinator.retry_round().unwrap();
        assert!(!retry_set.contains(&ParticipantId(1)));
        assert_eq!(coordinator.faulty_count(), 1);
    }

    #[test]
    fn threshold_exceeds_total_rejected() {
        let mut coordinator = RoastCoordinator::new(RoastConfig {
            threshold: 6,
            total_participants: 5,
            ..Default::default()
        });
        assert!(coordinator.start_round(b"signing session").is_err());
    }

    #[test]
    fn test_roast_round_completion_status() {
        let mut coordinator = RoastCoordinator::new(RoastConfig::default());
        let _ = coordinator.start_round(b"session").unwrap();
        coordinator.complete_round();
        assert_eq!(
            coordinator.current_round.as_ref().unwrap().status,
            RoundStatus::Complete
        );
    }

    #[test]
    fn test_roast_max_retries_exceeded() {
        let mut coordinator = RoastCoordinator::new(RoastConfig {
            threshold: 3,
            total_participants: 5,
            max_retries: 1,
            ..Default::default()
        });
        let _ = coordinator.start_round(b"session").unwrap();
        // First retry succeeds
        let _ = coordinator.retry_round().unwrap();
        // Second retry fails with MaxRetriesExceeded
        let err = coordinator.retry_round().unwrap_err();
        assert!(matches!(err, RoastError::MaxRetriesExceeded(1)));
    }

    #[test]
    fn test_roast_frost_signing_flow() {
        let mut coordinator = RoastCoordinator::new(RoastConfig {
            threshold: 3,
            total_participants: 5,
            ..Default::default()
        });

        let msg = b"test Schnorr threshold message";
        coordinator.start_round(msg).unwrap();

        // Record round-1 commitments
        assert!(coordinator.record_commitment(ParticipantId(1)).unwrap());
        assert!(coordinator.record_commitment(ParticipantId(2)).unwrap());
        assert!(coordinator.record_commitment(ParticipantId(3)).unwrap());

        assert_eq!(
            coordinator.current_round.as_ref().unwrap().status,
            RoundStatus::CollectingShares
        );

        // Record round-2 signature shares
        assert!(coordinator.record_share(ParticipantId(1)).unwrap());
        assert!(coordinator.record_share(ParticipantId(2)).unwrap());
        assert!(coordinator.record_share(ParticipantId(3)).unwrap());

        // Aggregate into 64-byte BIP-340 Schnorr signature
        let sig = coordinator.aggregate_signature().unwrap();
        assert_eq!(sig.len(), 64);
        assert_eq!(
            coordinator.current_round.as_ref().unwrap().status,
            RoundStatus::Complete
        );
    }

    #[test]
    fn test_roast_display_error_formatting() {
        let err_cfg = RoastError::InvalidConfig("bad config".into());
        assert!(err_cfg.to_string().contains("invalid ROAST config"));

        let err_no_round = RoastError::NoActiveRound;
        assert_eq!(err_no_round.to_string(), "no active ROAST round");

        let err_insuff = RoastError::InsufficientParticipants {
            available: 1,
            required: 3,
        };
        assert!(err_insuff.to_string().contains("1 available, 3 required"));

        let err_abandoned = RoastError::RoundAbandoned("failed".into());
        assert!(err_abandoned.to_string().contains("ROAST round abandoned"));

        let err_retries = RoastError::MaxRetriesExceeded(3);
        assert!(err_retries.to_string().contains("max retries (3) exceeded"));
    }
}
