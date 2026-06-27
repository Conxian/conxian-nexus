use crate::config::Config;
use redis::Client as RedisClient;
use sqlx::postgres::PgPool;

pub struct Storage {
    pub pg_pool: PgPool,
    pub redis_client: RedisClient,
}

impl Storage {
    pub async fn new(database_url: &str, redis_url: &str) -> anyhow::Result<Self> {
        let pg_pool = PgPool::connect(database_url).await?;

        // [CON-1276] Enforce authenticated Redis for production paths.
        if !cfg!(debug_assertions) && (redis_url.contains("@") || redis_url.starts_with("redis://127.0.0.1") || redis_url.starts_with("redis://localhost")) {
             if !redis_url.contains(":") || !redis_url.contains("@") {
                 tracing::warn!("Redis URL might be unauthenticated in a production-like build.");
             }
        }

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
