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

/// ROAST coordinator — wraps FROST signing with cooperative-subset logic.
pub struct RoastCoordinator {
    config: RoastConfig,
    current_round: Option<RoastRound>,
    /// Participants that have been flagged as faulty across all rounds.
    faulty_participants: HashSet<ParticipantId>,
}

impl RoastCoordinator {
    pub fn new(config: RoastConfig) -> Self {
        Self {
            config,
            current_round: None,
            faulty_participants: HashSet::new(),
        }
    }

    /// Start a new signing round for the given signing context.
    ///
    /// Returns the set of participant IDs that should receive the
    /// round-1 commitment request.
    pub fn start_round(
        &mut self,
        _message: &[u8],
    ) -> Result<HashSet<ParticipantId>, RoastError> {
        if self.config.threshold > self.config.total_participants {
            return Err(RoastError::InvalidConfig(
                "threshold cannot exceed total_participants".into(),
            ));
        }

        // Build participant set, excluding known faulty participants
        let candidates: HashSet<_> = (1..=self.config.total_participants as u16)
            .map(ParticipantId)
            .filter(|p| !self.faulty_participants.contains(p))
            .collect();

        if candidates.len() < self.config.threshold as usize {
            return Err(RoastError::InsufficientParticipants {
                available: candidates.len(),
                required: self.config.threshold as usize,
            });
        }

        self.current_round = Some(RoastRound {
            active: candidates.clone(),
            excluded: HashSet::new(),
            retry: 0,
            status: RoundStatus::CollectingCommitments,
        });

        Ok(candidates)
    }

    /// Record a participant as timed out for this round.
    pub fn mark_timeout(&mut self, participant: ParticipantId) -> RoastResult {
        let round = self.current_round.as_mut().ok_or(RoastError::NoActiveRound)?;
        round.active.remove(&participant);
        round.excluded.insert(participant);

        if round.active.len() < self.config.threshold as usize {
            round.status = RoundStatus::Abandoned;
            return Err(RoastError::RoundAbandoned(
                "not enough active participants after timeout".into(),
            ));
        }
        Ok(())
    }

    /// Record a participant as faulty (invalid data).
    ///
    /// Faulty participants are excluded from this and ALL future rounds.
    pub fn mark_faulty(&mut self, participant: ParticipantId, reason: &str) -> RoastResult {
        self.faulty_participants.insert(participant);
        let round = self.current_round.as_mut().ok_or(RoastError::NoActiveRound)?;
        round.active.remove(&participant);
        round.excluded.insert(participant);

        if round.active.len() < self.config.threshold as usize {
            round.status = RoundStatus::Abandoned;
            return Err(RoastError::RoundAbandoned(format!(
                "not enough active after marking {:?} faulty: {}",
                participant, reason
            )));
        }
        Ok(())
    }

    /// Retry the round with remaining participants.
    ///
    /// Faulty participants remain excluded. Timed-out participants
    /// may be retried in a new round.
    pub fn retry_round(&mut self) -> Result<HashSet<ParticipantId>, RoastError> {
        let round = self.current_round.as_mut().ok_or(RoastError::NoActiveRound)?;

        if round.retry >= self.config.max_retries {
            return Err(RoastError::MaxRetriesExceeded(self.config.max_retries));
        }

        round.retry += 1;
        // Move timed-out participants back to active (but not faulty ones)
        let retry_set: HashSet<_> = round
            .excluded
            .iter()
            .filter(|p| !self.faulty_participants.contains(p))
            .cloned()
            .collect();
        round.active.extend(retry_set.iter());
        for p in &retry_set {
            round.excluded.remove(p);
        }
        round.status = RoundStatus::CollectingCommitments;

        Ok(round.active.clone())
    }

    /// Mark the round as complete.
    pub fn complete_round(&mut self) {
        if let Some(ref mut round) = self.current_round {
            round.status = RoundStatus::Complete;
        }
    }

    pub fn active_count(&self) -> usize {
        self.current_round.as_ref().map(|r| r.active.len()).unwrap_or(0)
    }

    pub fn faulty_count(&self) -> usize {
        self.faulty_participants.len()
    }
}

/// ROAST coordinator errors.
#[derive(Debug)]
pub enum RoastError {
    InvalidConfig(String),
    NoActiveRound,
    InsufficientParticipants { available: usize, required: usize },
    RoundAbandoned(String),
    MaxRetriesExceeded(u32),
}

pub type RoastResult = Result<(), RoastError>;

impl std::fmt::Display for RoastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid ROAST config: {msg}"),
            Self::NoActiveRound => write!(f, "no active ROAST round"),
            Self::InsufficientParticipants { available, required } => {
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
        let target = SigningTarget::default();
        let participants = coordinator.start_round(&target).unwrap();
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
}
