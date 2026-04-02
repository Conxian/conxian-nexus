use redis::Client as RedisClient;
use sqlx::postgres::PgPool;
use std::env;

pub struct Storage {
    pub pg_pool: PgPool,
    pub redis_client: RedisClient,
}

impl Storage {
    pub async fn new() -> anyhow::Result<Self> {
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/nexus".to_string());
        let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());

        let pg_pool = PgPool::connect(&database_url).await?;
        let redis_client = RedisClient::open(redis_url)?;

        Ok(Self {
            pg_pool,
            redis_client,
        })
    }

    pub fn new_lazy(database_url: &str, redis_url: &str) -> anyhow::Result<Self> {
        let pg_pool = sqlx::postgres::PgPoolOptions::new().connect_lazy(database_url)?;
        let redis_client = RedisClient::open(redis_url)?;

        Ok(Self {
            pg_pool,
            redis_client,
        })
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations").run(&self.pg_pool).await?;
        Ok(())
    }
}

pub mod kwil;
pub mod tableland;
