use std::env;

pub const ENV_EXPERIMENTAL_APIS: &str = "NEXUS_EXPERIMENTAL_APIS";
pub const ENV_ORACLE_ENABLED: &str = "NEXUS_ORACLE_ENABLED";
pub const ENV_ORACLE_STUB_OK: &str = "NEXUS_ORACLE_STUB_OK";
pub const ENV_ORACLE_ENDPOINT_URL: &str = "ORACLE_ENDPOINT_URL";
pub const ENV_ORACLE_CONTRACT_PRINCIPAL: &str = "ORACLE_CONTRACT_PRINCIPAL";

const DEFAULT_DATABASE_URL: &str = "postgres://localhost/nexus";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1/";
const DEFAULT_STACKS_NODE_RPC_URL: &str = "https://api.mainnet.hiro.so";

// CON-394: Remove or flip this once the real OracleService is implemented.
const ORACLE_SERVICE_IS_STUBBED: bool = true;

pub(crate) fn parse_flag(value: &str) -> bool {
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
        use anyhow::{bail, Context};

        fn env_flag(key: &str) -> bool {
            env::var(key).map(|v| parse_flag(&v)).unwrap_or(false)
        }

        let database_url = match env::var("DATABASE_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    tracing::warn!(
                        default = DEFAULT_DATABASE_URL,
                        "DATABASE_URL set but empty; defaulting"
                    );
                    DEFAULT_DATABASE_URL.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => {
                tracing::warn!(default = DEFAULT_DATABASE_URL, "DATABASE_URL not set; defaulting");
                DEFAULT_DATABASE_URL.to_string()
            }
            Err(env::VarError::NotUnicode(_)) => bail!("DATABASE_URL must be valid unicode"),
        };

        let redis_url = match env::var("REDIS_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    tracing::warn!(default = DEFAULT_REDIS_URL, "REDIS_URL set but empty; defaulting");
                    DEFAULT_REDIS_URL.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => {
                tracing::warn!(default = DEFAULT_REDIS_URL, "REDIS_URL not set; defaulting");
                DEFAULT_REDIS_URL.to_string()
            }
            Err(env::VarError::NotUnicode(_)) => bail!("REDIS_URL must be valid unicode"),
        };

        let stacks_node_rpc_url = match env::var("STACKS_NODE_RPC_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    tracing::warn!(
                        default = DEFAULT_STACKS_NODE_RPC_URL,
                        "STACKS_NODE_RPC_URL set but empty; defaulting"
                    );
                    DEFAULT_STACKS_NODE_RPC_URL.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => {
                tracing::warn!(
                    default = DEFAULT_STACKS_NODE_RPC_URL,
                    "STACKS_NODE_RPC_URL not set; defaulting"
                );
                DEFAULT_STACKS_NODE_RPC_URL.to_string()
            }
            Err(env::VarError::NotUnicode(_)) => bail!("STACKS_NODE_RPC_URL must be valid unicode"),
        };

        let experimental_apis_enabled = env_flag(ENV_EXPERIMENTAL_APIS);
        let oracle_enabled = env_flag(ENV_ORACLE_ENABLED);
        let oracle_stub_ok = env_flag(ENV_ORACLE_STUB_OK);
        let oracle_endpoint_url = env::var(ENV_ORACLE_ENDPOINT_URL)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let oracle_contract_principal = env::var(ENV_ORACLE_CONTRACT_PRINCIPAL)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if oracle_enabled && ORACLE_SERVICE_IS_STUBBED && !oracle_stub_ok {
            bail!(
                "{} is blocked because OracleService is still stubbed. For dev/test only, also set {}=1 (or true/yes/on).",
                ENV_ORACLE_ENABLED,
                ENV_ORACLE_STUB_OK
            );
        }

        if oracle_enabled {
            if oracle_endpoint_url.is_none() {
                anyhow::bail!("{ENV_ORACLE_ENABLED}=1 requires {ENV_ORACLE_ENDPOINT_URL}");
            }

            if oracle_contract_principal.is_none() {
                anyhow::bail!("{ENV_ORACLE_ENABLED}=1 requires {ENV_ORACLE_CONTRACT_PRINCIPAL}");
            }
        }

        Ok(Self {
            database_url,
            redis_url,
            rest_port: env::var("REST_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .context("Invalid REST_PORT (expected u16)")?,
            grpc_port: env::var("GRPC_PORT")
                .unwrap_or_else(|_| "50051".to_string())
                .parse()
                .context("Invalid GRPC_PORT (expected u16)")?,
            stacks_node_rpc_url,
            gateway_url: env::var("GATEWAY_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            experimental_apis_enabled,
            oracle_enabled,
            oracle_stub_ok,
            oracle_endpoint_url,
            oracle_contract_principal,
        })
    }
}
