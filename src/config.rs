use std::env;

pub const ENV_EXPERIMENTAL_APIS: &str = "NEXUS_EXPERIMENTAL_APIS";
pub const ENV_ORACLE_ENABLED: &str = "NEXUS_ORACLE_ENABLED";
pub const ENV_ORACLE_STUB_OK: &str = "NEXUS_ORACLE_STUB_OK";
pub const ENV_ORACLE_ENDPOINT_URL: &str = "ORACLE_ENDPOINT_URL";
pub const ENV_ORACLE_CONTRACT_PRINCIPAL: &str = "ORACLE_CONTRACT_PRINCIPAL";
pub const ENV_ALLOW_DEFAULT_DB: &str = "NEXUS_ALLOW_DEFAULT_DB";
pub const ENV_ALLOW_DEFAULT_REDIS: &str = "NEXUS_ALLOW_DEFAULT_REDIS";

const DEFAULT_DATABASE_URL: &str = "postgres://localhost/nexus";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1/";
const DEFAULT_STACKS_NODE_RPC_URL: &str = "https://api.mainnet.hiro.so";

// CON-394: Remediated contamination. Stubbing is now explicit and restricted.
const ORACLE_SERVICE_IS_STUBBED: bool = false; // Remediated;

pub(crate) fn parse_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Debug, Clone)]
pub struct Config {
    pub nostr_secret_key: Option<String>,
    pub nostr_relays: Vec<String>,
    pub tableland_base_url: String,
    pub kwil_provider_url: Option<String>,
    pub kwil_db_id: Option<String>,
    pub kwil_private_key_hex: Option<String>,
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
    pub fn default_test() -> Self {
        Self {
            database_url: "postgres://localhost/nexus_test".to_string(),
            redis_url: DEFAULT_REDIS_URL.to_string(),
            rest_port: 3000,
            grpc_port: 50051,
            stacks_node_rpc_url: DEFAULT_STACKS_NODE_RPC_URL.to_string(),
            gateway_url: None,
            experimental_apis_enabled: true,
            nostr_secret_key: None,
            nostr_relays: vec![],
            tableland_base_url: "https://validator.tableland.xyz".to_string(),
            kwil_provider_url: None,
            kwil_db_id: None,
            kwil_private_key_hex: None,
            oracle_enabled: false,
            oracle_stub_ok: true,
            oracle_endpoint_url: None,
            oracle_contract_principal: None,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::{bail, Context};

        fn env_flag(key: &str) -> bool {
            env::var(key).map(|v| parse_flag(&v)).unwrap_or(false)
        }

        let allow_default_db = cfg!(debug_assertions) || env_flag(ENV_ALLOW_DEFAULT_DB);
        let database_url = match env::var("DATABASE_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    if allow_default_db {
                        tracing::warn!(
                            default = DEFAULT_DATABASE_URL,
                            "DATABASE_URL set but empty; defaulting"
                        );
                        DEFAULT_DATABASE_URL.to_string()
                    } else {
                        bail!("Missing env var: DATABASE_URL");
                    }
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => {
                if allow_default_db {
                    tracing::warn!(
                        default = DEFAULT_DATABASE_URL,
                        "DATABASE_URL not set; defaulting"
                    );
                    DEFAULT_DATABASE_URL.to_string()
                } else {
                    bail!("Missing env var: DATABASE_URL");
                }
            }
            Err(env::VarError::NotUnicode(_)) => bail!("DATABASE_URL must be valid unicode"),
        };

        let allow_default_redis = cfg!(debug_assertions) || env_flag(ENV_ALLOW_DEFAULT_REDIS);
        let redis_url = match env::var("REDIS_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    if allow_default_redis {
                        tracing::warn!(
                            default = DEFAULT_REDIS_URL,
                            "REDIS_URL set but empty; defaulting"
                        );
                        DEFAULT_REDIS_URL.to_string()
                    } else {
                        bail!("Missing env var: REDIS_URL");
                    }
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => {
                if allow_default_redis {
                    tracing::warn!(default = DEFAULT_REDIS_URL, "REDIS_URL not set; defaulting");
                    DEFAULT_REDIS_URL.to_string()
                } else {
                    bail!("Missing env var: REDIS_URL");
                }
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
            Err(env::VarError::NotUnicode(_)) => {
                tracing::warn!(
                    default = DEFAULT_STACKS_NODE_RPC_URL,
                    "STACKS_NODE_RPC_URL contains non-unicode bytes; defaulting"
                );
                DEFAULT_STACKS_NODE_RPC_URL.to_string()
            }
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
            anyhow::bail!(
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

        let nostr_secret_key = env::var("NOSTR_SECRET_KEY").ok().filter(|s| !s.is_empty());
        let nostr_relays = env::var("NOSTR_RELAYS").unwrap_or_else(|_| "ws://127.0.0.1:8080".to_string()).split(",").map(|s| s.trim().to_string()).collect();

        let tableland_base_url = env::var("TABLELAND_BASE_URL").unwrap_or_else(|_| "https://validator.tableland.xyz".to_string());
        let kwil_provider_url = env::var("KWIL_PROVIDER_URL").ok().filter(|s| !s.is_empty());
        let kwil_db_id = env::var("KWIL_DB_ID").ok().filter(|s| !s.is_empty());
        let kwil_private_key_hex = env::var("KWIL_PRIVATE_KEY_HEX")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if kwil_provider_url.is_some() || kwil_db_id.is_some() || kwil_private_key_hex.is_some() {
            if kwil_provider_url.is_none() || kwil_db_id.is_none() || kwil_private_key_hex.is_none() {
                bail!("Kwil persistence requires KWIL_PROVIDER_URL, KWIL_DB_ID, and KWIL_PRIVATE_KEY_HEX to all be set");
            }
        }

        Ok(Self {
            nostr_secret_key,
            nostr_relays,
            tableland_base_url,
            kwil_provider_url,
            kwil_db_id,
            kwil_private_key_hex,
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
