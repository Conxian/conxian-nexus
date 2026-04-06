use std::env;

pub const ENV_EXPERIMENTAL_APIS: &str = "NEXUS_EXPERIMENTAL_APIS";
pub const ENV_ORACLE_ENABLED: &str = "NEXUS_ORACLE_ENABLED";
pub const ENV_ORACLE_ENDPOINT_URL: &str = "ORACLE_ENDPOINT_URL";
pub const ENV_ORACLE_CONTRACT_PRINCIPAL: &str = "ORACLE_CONTRACT_PRINCIPAL";

pub fn parse_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub rest_port: u16,
    pub grpc_port: u16,
    pub log_level: String,
    pub stacks_node_rpc_url: String,
    pub gateway_url: Option<String>,
    pub experimental_apis_enabled: bool,
    pub oracle_enabled: bool,
    pub oracle_endpoint_url: Option<String>,
    pub oracle_contract_principal: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::Context;

        fn env_flag(key: &str) -> bool {
            env::var(key).ok().is_some_and(|v| parse_flag(&v))
        }

        Ok(Self {
            database_url: match env::var("DATABASE_URL") {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(
                        "DATABASE_URL not set; defaulting to postgres://localhost/nexus"
                    );
                    "postgres://localhost/nexus".to_string()
                }
            },
            redis_url: match env::var("REDIS_URL") {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!("REDIS_URL not set; defaulting to redis://127.0.0.1/");
                    "redis://127.0.0.1/".to_string()
                }
            },
            rest_port: env::var("REST_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .context("Invalid REST_PORT (expected u16)")?,
            grpc_port: env::var("GRPC_PORT")
                .unwrap_or_else(|_| "50051".to_string())
                .parse()
                .context("Invalid GRPC_PORT (expected u16)")?,
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            stacks_node_rpc_url: match env::var("STACKS_NODE_RPC_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                Some(url) => url,
                None => {
                    tracing::warn!(
                        "STACKS_NODE_RPC_URL not set or empty; defaulting to https://api.mainnet.hiro.so"
                    );
                    "https://api.mainnet.hiro.so".to_string()
                }
            },
            gateway_url: env::var("GATEWAY_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            experimental_apis_enabled: env_flag(ENV_EXPERIMENTAL_APIS),
            oracle_enabled: env_flag(ENV_ORACLE_ENABLED),
            oracle_endpoint_url: env::var(ENV_ORACLE_ENDPOINT_URL)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            oracle_contract_principal: env::var(ENV_ORACLE_CONTRACT_PRINCIPAL)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}
