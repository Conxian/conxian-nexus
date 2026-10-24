//! Portable administrative state plus atomic, idempotent audit receipts.
//! Authentication, signature policy and release acceptance belong to callers.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    RequestRelease,
    ProposeGovernance,
    DecideRelease,
    DecideGovernance,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Reject,
    RequestChanges,
}

#[derive(Clone, Debug, Serialize)]
pub struct Command {
    pub operation: Operation,
    pub entity_id: String,
    pub actor: String,
    pub decision: Option<Decision>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub accepted: bool,
    pub message: String,
    pub audit_event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEvent {
    pub id: String,
    pub category: String,
    pub actor: String,
    pub summary: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("invalid workflow input")]
    InvalidInput,
    #[error("idempotency key already bound to a different command")]
    CommandConflict,
    #[error("workflow entity not found")]
    NotFound,
    #[error("invalid workflow transition")]
    InvalidTransition,
    #[error("workflow storage unavailable")]
    Backend(#[from] sqlx::Error),
    #[error("workflow receipt cannot be decoded")]
    Receipt(#[from] serde_json::Error),
}

fn identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

impl Command {
    pub fn fingerprint(&self) -> Result<String, WorkflowError> {
        if !identifier(&self.entity_id)
            || !identifier(&self.actor)
            || self.notes.as_ref().is_some_and(|v| v.len() > 4096)
            || matches!(
                self.operation,
                Operation::RequestRelease | Operation::ProposeGovernance
            ) != self.decision.is_none()
        {
            return Err(WorkflowError::InvalidInput);
        }
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }

    fn kind(&self) -> &'static str {
        match self.operation {
            Operation::RequestRelease | Operation::DecideRelease => "release",
            Operation::ProposeGovernance | Operation::DecideGovernance => "governance",
        }
    }

    fn next_status(&self, current: Option<&str>) -> Result<&'static str, WorkflowError> {
        use Decision::*;
        use Operation::*;
        match (&self.operation, current, &self.decision) {
            (RequestRelease, None | Some("draft" | "rejected"), None) => Ok("in_review"),
            (ProposeGovernance, None | Some("draft" | "changes_requested"), None) => Ok("pending"),
            (DecideRelease, Some("in_review"), Some(Approve)) => Ok("approved"),
            (DecideRelease, Some("in_review"), Some(Reject)) => Ok("rejected"),
            (DecideRelease, Some("in_review"), Some(RequestChanges)) => Ok("draft"),
            (DecideGovernance, Some("pending"), Some(Approve)) => Ok("approved"),
            (DecideGovernance, Some("pending"), Some(Reject)) => Ok("rejected"),
            (DecideGovernance, Some("pending"), Some(RequestChanges)) => Ok("changes_requested"),
            (DecideRelease | DecideGovernance, None, _) => Err(WorkflowError::NotFound),
            _ => Err(WorkflowError::InvalidTransition),
        }
    }
}

#[derive(Clone)]
pub struct WorkflowStore {
    pool: PgPool,
}

impl WorkflowStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Commit state and receipt together. Reconnect/retry returns the original
    /// receipt; no in-memory cache is an authority for completion.
    pub async fn apply(
        &self,
        command_id: &str,
        command: &Command,
    ) -> Result<Receipt, WorkflowError> {
        if !identifier(command_id) {
            return Err(WorkflowError::InvalidInput);
        }
        let fingerprint = command.fingerprint()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL synchronous_commit = on")
            .execute(&mut *tx)
            .await?;
        // Fixed lock namespaces avoid command/entity cross-lock collisions.
        // Hash collisions only serialize unrelated work; they do not grant it.
        sqlx::query("SELECT pg_advisory_xact_lock(24001, hashtext($1))")
            .bind(command_id)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT fingerprint, receipt_json FROM admin_workflow_events WHERE command_id = $1",
        )
        .bind(command_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("fingerprint")? != fingerprint {
                return Err(WorkflowError::CommandConflict);
            }
            let receipt = serde_json::from_str(&row.try_get::<String, _>("receipt_json")?)?;
            tx.commit().await?;
            return Ok(receipt);
        }
        let entity_lock = format!("{}:{}", command.kind(), command.entity_id);
        sqlx::query("SELECT pg_advisory_xact_lock(24002, hashtext($1))")
            .bind(entity_lock)
            .execute(&mut *tx)
            .await?;
        let current = sqlx::query_scalar::<_, String>(
            "SELECT status FROM admin_workflow_entities WHERE kind = $1 AND id = $2 FOR UPDATE",
        )
        .bind(command.kind())
        .bind(&command.entity_id)
        .fetch_optional(&mut *tx)
        .await?;
        let status = command.next_status(current.as_deref())?;
        let now = Utc::now();
        sqlx::query("INSERT INTO admin_workflow_entities(kind,id,title,owner,status,updated_at) VALUES($1,$2,$2,$3,$4,$5) ON CONFLICT(kind,id) DO UPDATE SET status = EXCLUDED.status, updated_at = EXCLUDED.updated_at")
            .bind(command.kind()).bind(&command.entity_id).bind(&command.actor).bind(status).bind(now)
            .execute(&mut *tx).await?;
        let receipt_id = format!("workflow_{}", uuid::Uuid::new_v4().simple());
        let decision = matches!(
            command.operation,
            Operation::DecideRelease | Operation::DecideGovernance
        );
        let receipt = Receipt {
            accepted: true,
            message: "Administrative workflow and audit event committed; not release or production acceptance.".into(),
            audit_event_id: format!("audit_{}", uuid::Uuid::new_v4().simple()),
            request_id: if decision { None } else { Some(receipt_id.clone()) },
            decision_id: if decision { Some(receipt_id) } else { None },
        };
        sqlx::query("INSERT INTO admin_workflow_events(id,command_id,fingerprint,category,entity_id,actor,summary,timestamp,receipt_json,notes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&receipt.audit_event_id).bind(command_id).bind(fingerprint).bind(command.kind())
            .bind(&command.entity_id).bind(&command.actor)
            .bind(format!("Administrative {} workflow changed to {}", command.kind(), status))
            .bind(now).bind(serde_json::to_string(&receipt)?).bind(&command.notes).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn entities(
        &self,
        kind: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Entity>, WorkflowError> {
        if !matches!(kind, "release" | "governance")
            || !(1..=1000).contains(&limit)
            || offset > 1_000_000
        {
            return Err(WorkflowError::InvalidInput);
        }
        Ok(sqlx::query_as("SELECT id,title,status,owner,updated_at FROM admin_workflow_entities WHERE kind = $1 ORDER BY updated_at DESC,id LIMIT $2 OFFSET $3")
            .bind(kind).bind(i64::from(limit)).bind(i64::from(offset)).fetch_all(&self.pool).await?)
    }

    pub async fn audit(&self, limit: u32, offset: u32) -> Result<Vec<AuditEvent>, WorkflowError> {
        if !(1..=1000).contains(&limit) || offset > 1_000_000 {
            return Err(WorkflowError::InvalidInput);
        }
        Ok(sqlx::query_as("SELECT id,category,actor,summary,timestamp FROM admin_workflow_events ORDER BY timestamp DESC,id LIMIT $1 OFFSET $2")
            .bind(i64::from(limit)).bind(i64::from(offset)).fetch_all(&self.pool).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(operation: Operation, decision: Option<Decision>) -> Command {
        Command {
            operation,
            decision,
            entity_id: "artifact-1".into(),
            actor: "operator-1".into(),
            notes: None,
        }
    }
    #[test]
    fn lifecycle_rejects_missing_and_terminal_decisions() {
        let c = command(Operation::DecideRelease, Some(Decision::Approve));
        assert!(matches!(c.next_status(None), Err(WorkflowError::NotFound)));
        assert!(matches!(
            c.next_status(Some("published")),
            Err(WorkflowError::InvalidTransition)
        ));
        assert_eq!(c.next_status(Some("in_review")).unwrap(), "approved");
    }
    #[test]
    fn fingerprint_rejects_malformed_commands_before_storage() {
        assert!(command(Operation::RequestRelease, Some(Decision::Approve))
            .fingerprint()
            .is_err());
        assert!(command(Operation::DecideGovernance, None)
            .fingerprint()
            .is_err());
        let mut c = command(Operation::RequestRelease, None);
        c.actor = "\n".into();
        assert!(c.fingerprint().is_err());
    }
}
