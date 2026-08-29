//! Fail-closed consume-once idempotency for the Nexus delivery runtime.
//!
//! Guarantees at-most-once effect execution across replicas and restarts by
//! using the PostgreSQL unique constraint as the atomic conditional-write
//! primitive (`INSERT ... ON CONFLICT DO NOTHING`). A record's existence is the
//! proof of consumption; there is no separate "claimed but not committed" state.

use sqlx::postgres::PgPool;

pub const MAX_KEY_LEN: usize = 512;
pub const MAX_OPERATION_LEN: usize = 128;

/// Result of a consume-once attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeOutcome {
    /// The key was not previously seen; the caller now owns the effect.
    Fresh,
    /// The key was already consumed; the caller must not re-execute the effect.
    AlreadyConsumed,
}

#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("idempotency key must be 1..={MAX_KEY_LEN} characters")]
    InvalidKey,
    #[error("operation must be 1..={MAX_OPERATION_LEN} characters")]
    InvalidOperation,
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
}
