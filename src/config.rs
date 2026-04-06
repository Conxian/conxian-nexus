use std::env;

const DEFAULT_STACKS_NODE_RPC_URL: &str = "https://api.mainnet.hiro.so";
const ORACLE_SERVICE_IS_STUBBED: bool = true;

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
    pub oracle_stub_ok: bool,
    pub oracle_endpoint_url: Option<String>,
    pub oracle_contract_principal: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::Context;

        fn env_flag(key: &str) -> bool {
            let value = match env::var(key) {
                Ok(v) => v,
                Err(_) => return false,
            };

            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }

        let stacks_node_rpc_url = match env::var("STACKS_NODE_RPC_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                // Treat empty/whitespace as "not set" so we fall back to the default URL.
                if trimmed.is_empty() {
                    DEFAULT_STACKS_NODE_RPC_URL.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => DEFAULT_STACKS_NODE_RPC_URL.to_string(),
            Err(env::VarError::NotUnicode(_)) => {
                anyhow::bail!("STACKS_NODE_RPC_URL must be valid unicode");
            }
        };

        let experimental_apis_enabled = env_flag("NEXUS_EXPERIMENTAL_APIS");
        let oracle_enabled = env_flag("NEXUS_ORACLE_ENABLED");
        let oracle_stub_ok = env_flag("NEXUS_ORACLE_STUB_OK");

        if oracle_enabled && ORACLE_SERVICE_IS_STUBBED && !oracle_stub_ok {
            anyhow::bail!(
                "NEXUS_ORACLE_ENABLED is blocked because OracleService is still stubbed. For dev/test only, also set NEXUS_ORACLE_STUB_OK=1 (or true/yes/on)."
            );
        }

        Ok(Self {
            database_url: env::var("DATABASE_URL").context("Missing env var: DATABASE_URL")?,
            redis_url: env::var("REDIS_URL").context("Missing env var: REDIS_URL")?,
            rest_port: env::var("REST_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .context("Invalid REST_PORT (expected u16)")?,
            grpc_port: env::var("GRPC_PORT")
                .unwrap_or_else(|_| "50051".to_string())
                .parse()
                .context("Invalid GRPC_PORT (expected u16)")?,
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            stacks_node_rpc_url,
            gateway_url: env::var("GATEWAY_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            experimental_apis_enabled,
            oracle_enabled,
            oracle_stub_ok,
            oracle_endpoint_url: env::var("ORACLE_ENDPOINT_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            oracle_contract_principal: env::var("ORACLE_CONTRACT_PRINCIPAL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}
