use crate::config::Config;
use redis::Client as RedisClient;
use sqlx::postgres::PgPool;

pub struct Storage {
    pub pg_pool: PgPool,
    pub redis_client: RedisClient,
}

impl Storage {
    pub async fn new(database_url: &str, redis_url: &str) -> anyhow::Result<Self> {
        if !cfg!(debug_assertions) {
            // Redis Boundary Check (Hole 1.2)
            let redis_local = redis_url.contains("127.0.0.1") || redis_url.contains("localhost");
            let redis_unauthenticated = !redis_url.contains("@");

            if redis_local || redis_unauthenticated {
                if std::env::var("NEXUS_ALLOW_UNSAFE_REDIS").is_ok() {
                    tracing::warn!(
                        "Unsafe Redis configuration allowed by NEXUS_ALLOW_UNSAFE_REDIS override."
                    );
                } else {
                    anyhow::bail!(
                        "Production boundary violation: Redis must be authenticated and remote in release builds. \
                         (Local: {}, Unauthenticated: {}). Set NEXUS_ALLOW_UNSAFE_REDIS=1 to override.",
                        redis_local,
                        redis_unauthenticated
                    );
                }
            }

            // PostgreSQL Boundary Check (Hole 1.2 Alignment)
            let pg_local = database_url.contains("127.0.0.1") || database_url.contains("localhost");
            // Check for authentication by looking for : before @ (simple heuristic for postgres://user:pass@host)
            let pg_unauthenticated = !database_url.contains(":")
                || !database_url.contains("@")
                || database_url.find(':') > database_url.find('@');

            if pg_local || pg_unauthenticated {
                if std::env::var("NEXUS_ALLOW_UNSAFE_DB").is_ok() {
                    tracing::warn!("Unsafe PostgreSQL configuration allowed by NEXUS_ALLOW_UNSAFE_DB override.");
                } else {
                    anyhow::bail!(
                        "Production boundary violation: PostgreSQL must be authenticated and remote in release builds. \
                         (Local: {}, Unauthenticated: {}). Set NEXUS_ALLOW_UNSAFE_DB=1 to override.",
                        pg_local,
                        pg_unauthenticated
                    );
                }
            }
        }

        let pg_pool = PgPool::connect(database_url).await?;
        let redis_client = RedisClient::open(redis_url)?;

        Ok(Self {
            pg_pool,
            redis_client,
        })
    }

    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        Self::new(&config.database_url, &config.redis_url).await
    }

    pub fn new_lazy(database_url: &str, redis_url: &str) -> anyhow::Result<Self> {
        let pg_pool = sqlx::postgres::PgPoolOptions::new().connect_lazy(database_url)?;
        let redis_client = RedisClient::open(redis_url)?;

        Ok(Self {
            pg_pool,
            redis_client,
        })
    }

    pub fn from_config_lazy(config: &Config) -> anyhow::Result<Self> {
        Self::new_lazy(&config.database_url, &config.redis_url)
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations").run(&self.pg_pool).await?;
        Ok(())
    }

    #[cfg(test)]
    pub fn for_tests() -> std::sync::Arc<Self> {
        let pg_pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/nexus")
            .expect("connect_lazy should not require a live DB");

        let redis_client = RedisClient::open("redis://127.0.0.1/")
            .expect("redis client construction should not require a live server");

        std::sync::Arc::new(Self {
            pg_pool,
            redis_client,
        })
    }
}

pub mod kwil;
pub mod tableland;
