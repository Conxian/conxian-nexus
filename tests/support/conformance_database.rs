//! Disposable test-only PostgreSQL configuration. Never reads DATABASE_URL.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;
use url::Url;

pub fn validate(
    raw: Option<&str>,
    allow: Option<&str>,
    inherited_pg: bool,
) -> Result<(), &'static str> {
    let raw = raw
        .filter(|value| !value.is_empty())
        .ok_or("CONFORMANCE_DATABASE_URL is required")?;
    if allow != Some("true") {
        return Err("ALLOW_DESTRUCTIVE_TEST_RESET=true is required for the disposable fixture");
    }
    if inherited_pg {
        return Err("unset inherited PG* settings before running disposable conformance tests");
    }
    if raw.len() > 4096 || raw.trim() != raw || raw.chars().any(char::is_control) {
        return Err("invalid disposable conformance URL");
    }
    let url = Url::parse(raw).map_err(|_| "invalid disposable conformance URL")?;
    if !matches!(url.scheme(), "postgres" | "postgresql")
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
        || url.port().is_none_or(|port| port == 0)
        || url.path() != "/conformance_test"
        || url.username() != "conformance"
        || url.password().is_none_or(str::is_empty)
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("fixture requires numeric loopback, explicit port/password, conformance role, conformance_test database, and no URL overrides");
    }
    Ok(())
}

// No Debug implementation: connection options contain fixture credentials.
pub struct ConformanceDatabase {
    options: PgConnectOptions,
    schema: String,
}

impl ConformanceDatabase {
    pub fn from_environment() -> Self {
        let raw = std::env::var("CONFORMANCE_DATABASE_URL").ok();
        let allow = std::env::var("ALLOW_DESTRUCTIVE_TEST_RESET").ok();
        let inherited_pg =
            std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("PG"));
        validate(raw.as_deref(), allow.as_deref(), inherited_pg)
            .unwrap_or_else(|message| panic!("{message}"));
        let options = PgConnectOptions::from_str(raw.as_deref().expect("validated explicit URL"))
            .unwrap_or_else(|_| panic!("invalid disposable conformance driver options"));
        Self {
            options,
            schema: format!("nexus_conformance_{}", uuid::Uuid::new_v4().simple()),
        }
    }

    pub async fn pool(&self) -> PgPool {
        let schema = self.schema.clone();
        PgPoolOptions::new()
            .max_connections(64)
            .acquire_timeout(Duration::from_secs(5))
            .after_connect(move |connection, _| {
                let schema = schema.clone();
                Box::pin(async move {
                    let identity: (String, String, bool) = sqlx::query_as("SELECT current_database(), current_user, rolsuper FROM pg_roles WHERE rolname = current_user")
                        .fetch_one(&mut *connection).await?;
                    if identity != ("conformance_test".into(), "conformance".into(), false) {
                        return Err(sqlx::Error::Protocol("disposable fixture identity mismatch".into()));
                    }
                    sqlx::query("SELECT set_config('search_path', $1, false)")
                        .bind(schema).execute(&mut *connection).await?;
                    Ok(())
                })
            })
            .connect_with(self.options.clone()).await
            .unwrap_or_else(|_| panic!("disposable PostgreSQL fixture is unavailable"))
    }

    /// Installs only replay migrations in this test's fresh private schema.
    /// No TRUNCATE, public-table reset, fallback URL, or production migration.
    pub async fn prepare(&self) {
        let pool = self.pool().await;
        let mut tx = pool
            .begin()
            .await
            .expect("begin disposable fixture transaction");
        // schema is an internal fixed prefix followed only by UUID hex digits.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE SCHEMA {}",
            self.schema
        )))
        .execute(&mut *tx)
        .await
        .expect("create private disposable schema");
        for migration in [
            include_str!("../../migrations/20260829000000_idempotency.sql"),
            include_str!("../../migrations/20260830000000_idempotency_high_water.sql"),
            include_str!("../../migrations/20260912000000_idempotency_locks.sql"),
        ] {
            sqlx::raw_sql(migration)
                .execute(&mut *tx)
                .await
                .expect("install disposable replay migration");
        }
        tx.commit()
            .await
            .expect("commit disposable replay migrations");
        pool.close().await;
    }
}
