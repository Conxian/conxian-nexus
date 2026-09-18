//! Fail-closed consume-once idempotency for the Nexus delivery runtime.
//!
//! Guarantees at-most-once effect execution across replicas and restarts by
//! using the PostgreSQL unique constraint as the atomic conditional-write
//! primitive (`INSERT ... ON CONFLICT DO NOTHING`). A record's existence is the
//! proof of consumption; there is no separate "claimed but not committed" state.
//!
//! Additionally provides distributed transactional idempotency locks (`idempotency_locks`)
//! with atomic TTL expiration, row-level concurrency, owner-based release/extension,
//! and payload persistence (#251).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;

pub const MAX_KEY_LEN: usize = 512;
pub const MAX_OPERATION_LEN: usize = 128;
pub const MAX_CLOCK_LEN: usize = 128;
pub const MAX_OWNER_LEN: usize = 256;

/// Result of a consume-once attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeOutcome {
    /// The key was not previously seen; the caller now owns the effect.
    Fresh,
    /// The key was already consumed; the caller must not re-execute the effect.
    AlreadyConsumed,
}

/// Represents an active or expired idempotency lock record (#251).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct IdempotencyLock {
    pub key: String,
    pub owner: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("idempotency key must be 1..={MAX_KEY_LEN} characters")]
    InvalidKey,
    #[error("operation must be 1..={MAX_OPERATION_LEN} characters")]
    InvalidOperation,
    #[error("idempotency clock must be 1..={MAX_CLOCK_LEN} characters")]
    InvalidClock,
    #[error("idempotency owner must be 1..={MAX_OWNER_LEN} characters")]
    InvalidOwner,
    #[error("lock ttl must be positive")]
    InvalidTtl,
    #[error("idempotency clock moved backwards")]
    ClockRollback,
    #[error("idempotency backend error: {0}")]
    Backend(#[from] sqlx::Error),
}

/// Consume-once store backed by a shared PostgreSQL pool.
pub struct IdempotencyStore {
    pool: PgPool,
}

impl IdempotencyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Validation precedes any observation or mutation so that malformed input
    /// can never advance state or consume a key.
    fn validate(key: &str, operation: &str) -> Result<(), IdempotencyError> {
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(IdempotencyError::InvalidKey);
        }
        if operation.is_empty() || operation.len() > MAX_OPERATION_LEN {
            return Err(IdempotencyError::InvalidOperation);
        }
        Ok(())
    }

    fn validate_lock_params(key: &str, owner: &str, ttl_secs: i64) -> Result<(), IdempotencyError> {
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(IdempotencyError::InvalidKey);
        }
        if owner.is_empty() || owner.len() > MAX_OWNER_LEN {
            return Err(IdempotencyError::InvalidOwner);
        }
        if ttl_secs <= 0 {
            return Err(IdempotencyError::InvalidTtl);
        }
        Ok(())
    }

    /// Atomically consume a single idempotency key.
    pub async fn consume_once(
        &self,
        key: &str,
        operation: &str,
    ) -> Result<ConsumeOutcome, IdempotencyError> {
        Self::validate(key, operation)?;

        let inserted = sqlx::query(
            "INSERT INTO idempotency_records (idempotency_key, operation) \
             VALUES ($1, $2) ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .execute(&self.pool)
        .await?;

        Ok(if inserted.rows_affected() == 1 {
            ConsumeOutcome::Fresh
        } else {
            ConsumeOutcome::AlreadyConsumed
        })
    }

    /// Consume a batch atomically (all-or-nothing).
    ///
    /// Every key is validated before any write. If any key is already consumed,
    /// the entire batch rolls back and reports `AlreadyConsumed` for every item
    /// without persisting a partial result.
    pub async fn consume_once_batch(
        &self,
        items: &[(String, String)],
    ) -> Result<Vec<ConsumeOutcome>, IdempotencyError> {
        for (key, operation) in items {
            Self::validate(key, operation)?;
        }
        if items.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.pool.begin().await?;
        let mut outcomes = Vec::with_capacity(items.len());

        for (key, operation) in items {
            let inserted = sqlx::query(
                "INSERT INTO idempotency_records (idempotency_key, operation) \
                 VALUES ($1, $2) ON CONFLICT (idempotency_key) DO NOTHING",
            )
            .bind(key)
            .bind(operation)
            .execute(&mut *tx)
            .await?;

            if inserted.rows_affected() == 1 {
                outcomes.push(ConsumeOutcome::Fresh);
            } else {
                tx.rollback().await?;
                return Ok(items
                    .iter()
                    .map(|_| ConsumeOutcome::AlreadyConsumed)
                    .collect());
            }
        }

        tx.commit().await?;
        Ok(outcomes)
    }

    /// Consume a single idempotency key with an absolute retention horizon.
    ///
    /// The record carries `expires_at` so that [`Self::purge_expired`] can
    /// reclaim it once the horizon passes. Consumption semantics are otherwise
    /// identical to [`Self::consume_once`].
    pub async fn consume_once_until(
        &self,
        key: &str,
        operation: &str,
        retain_until: DateTime<Utc>,
    ) -> Result<ConsumeOutcome, IdempotencyError> {
        Self::validate(key, operation)?;

        let inserted = sqlx::query(
            "INSERT INTO idempotency_records (idempotency_key, operation, expires_at) \
             VALUES ($1, $2, $3) ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(retain_until)
        .execute(&self.pool)
        .await?;

        Ok(if inserted.rows_affected() == 1 {
            ConsumeOutcome::Fresh
        } else {
            ConsumeOutcome::AlreadyConsumed
        })
    }

    /// Delete records whose retention horizon has passed.
    ///
    /// Returns the number of reclaimed records. Records without a retention
    /// horizon (`expires_at IS NULL`) are never reclaimed.
    pub async fn purge_expired(&self, now: DateTime<Utc>) -> Result<u64, IdempotencyError> {
        let result = sqlx::query(
            "DELETE FROM idempotency_records WHERE expires_at IS NOT NULL AND expires_at <= $1",
        )
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Observe a wall-clock second for a named clock, fail-closed on rollback.
    ///
    /// The update is a single conditional upsert: the high-water mark is only
    /// advanced when `now_secs` is greater than or equal to the stored value.
    /// A zero-row result means the stored value is already ahead of `now_secs`,
    /// which is reported as [`IdempotencyError::ClockRollback`].
    pub async fn observe_time(&self, clock: &str, now_secs: i64) -> Result<(), IdempotencyError> {
        if clock.is_empty() || clock.len() > MAX_CLOCK_LEN {
            return Err(IdempotencyError::InvalidClock);
        }

        let result = sqlx::query(
            "INSERT INTO idempotency_high_water (clock, last_observed_secs) \
             VALUES ($1, $2) \
             ON CONFLICT (clock) DO UPDATE \
             SET last_observed_secs = EXCLUDED.last_observed_secs, updated_at = now() \
             WHERE idempotency_high_water.last_observed_secs <= EXCLUDED.last_observed_secs",
        )
        .bind(clock)
        .bind(now_secs)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(IdempotencyError::ClockRollback);
        }
        Ok(())
    }

    /// Atomically acquire or re-acquire an idempotency lock (#251).
    ///
    /// Returns `true` if the lock was successfully acquired (either fresh or replacing an
    /// expired lock or owned lock), or `false` if an active unexpired lock is held by another owner.
    pub async fn acquire_lock(
        &self,
        key: &str,
        owner: &str,
        ttl_secs: i64,
        payload: Option<serde_json::Value>,
    ) -> Result<bool, IdempotencyError> {
        Self::validate_lock_params(key, owner, ttl_secs)?;

        let now = Utc::now();
        let expires_at = now + Duration::seconds(ttl_secs);

        let result = sqlx::query(
            "INSERT INTO idempotency_locks (key, owner, created_at, expires_at, payload) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (key) DO UPDATE \
             SET owner = EXCLUDED.owner, \
                 created_at = EXCLUDED.created_at, \
                 expires_at = EXCLUDED.expires_at, \
                 payload = EXCLUDED.payload \
             WHERE idempotency_locks.expires_at <= $3 OR idempotency_locks.owner = EXCLUDED.owner",
        )
        .bind(key)
        .bind(owner)
        .bind(now)
        .bind(expires_at)
        .bind(payload)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// Extend the duration of an active idempotency lock (#251).
    ///
    /// Returns `true` if the lock is held by `owner` and not expired, or `false` otherwise.
    pub async fn extend_lock(
        &self,
        key: &str,
        owner: &str,
        additional_ttl_secs: i64,
    ) -> Result<bool, IdempotencyError> {
        Self::validate_lock_params(key, owner, additional_ttl_secs)?;

        let now = Utc::now();
        let new_expires_at = now + Duration::seconds(additional_ttl_secs);

        let result = sqlx::query(
            "UPDATE idempotency_locks \
             SET expires_at = $3 \
             WHERE key = $1 AND owner = $2 AND expires_at > $4",
        )
        .bind(key)
        .bind(owner)
        .bind(new_expires_at)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// Release an idempotency lock if held by `owner` (#251).
    ///
    /// Returns `true` if the lock record was deleted, or `false` if not found or owned by another worker.
    pub async fn release_lock(&self, key: &str, owner: &str) -> Result<bool, IdempotencyError> {
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(IdempotencyError::InvalidKey);
        }
        if owner.is_empty() || owner.len() > MAX_OWNER_LEN {
            return Err(IdempotencyError::InvalidOwner);
        }

        let result = sqlx::query("DELETE FROM idempotency_locks WHERE key = $1 AND owner = $2")
            .bind(key)
            .bind(owner)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() == 1)
    }

    /// Retrieve the current status of an idempotency lock (#251).
    pub async fn get_lock(&self, key: &str) -> Result<Option<IdempotencyLock>, IdempotencyError> {
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(IdempotencyError::InvalidKey);
        }

        let lock = sqlx::query_as::<_, IdempotencyLock>(
            "SELECT key, owner, created_at, expires_at, payload FROM idempotency_locks WHERE key = $1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(lock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lazy_store() -> IdempotencyStore {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/nexus")
            .expect("connect_lazy should not require a live DB");
        IdempotencyStore::new(pool)
    }

    #[tokio::test]
    async fn invalid_key_rejected_before_db_access() {
        let store = lazy_store();
        assert!(matches!(
            store.consume_once("", "op").await,
            Err(IdempotencyError::InvalidKey)
        ));
        let long_key = "k".repeat(MAX_KEY_LEN + 1);
        assert!(matches!(
            store.consume_once(&long_key, "op").await,
            Err(IdempotencyError::InvalidKey)
        ));
    }

    #[tokio::test]
    async fn invalid_operation_rejected_before_db_access() {
        let store = lazy_store();
        assert!(matches!(
            store.consume_once("key", "").await,
            Err(IdempotencyError::InvalidOperation)
        ));
    }

    #[tokio::test]
    async fn batch_validates_all_items_before_any_db_access() {
        let store = lazy_store();
        let items = vec![
            ("ok-key".to_string(), "op".to_string()),
            ("".to_string(), "op".to_string()),
        ];
        assert!(matches!(
            store.consume_once_batch(&items).await,
            Err(IdempotencyError::InvalidKey)
        ));
    }

    #[tokio::test]
    async fn empty_batch_is_a_noop() {
        let store = lazy_store();
        assert_eq!(store.consume_once_batch(&[]).await.unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn lock_parameter_validation() {
        let store = lazy_store();
        assert!(matches!(
            store.acquire_lock("", "worker1", 30, None).await,
            Err(IdempotencyError::InvalidKey)
        ));
        assert!(matches!(
            store.acquire_lock("key1", "", 30, None).await,
            Err(IdempotencyError::InvalidOwner)
        ));
        assert!(matches!(
            store.acquire_lock("key1", "worker1", 0, None).await,
            Err(IdempotencyError::InvalidTtl)
        ));
        assert!(matches!(
            store.acquire_lock("key1", "worker1", -10, None).await,
            Err(IdempotencyError::InvalidTtl)
        ));
        assert!(matches!(
            store.extend_lock("key1", "worker1", 0).await,
            Err(IdempotencyError::InvalidTtl)
        ));
        assert!(matches!(
            store.release_lock("", "worker1").await,
            Err(IdempotencyError::InvalidKey)
        ));
        assert!(matches!(
            store.get_lock("").await,
            Err(IdempotencyError::InvalidKey)
        ));
    }
}
