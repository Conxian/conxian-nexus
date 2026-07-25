use crate::executor::bitvm_groth16::{
    BitcoinNetwork, Groth16Curve, PublicInputLayout, TrustedVerificationKeyConfig,
    TrustedVerificationKeyRegistry, VerificationKeyId, GROTH16_SCHEMA_VERSION,
    NEXUS_STATE_TRANSITION_CIRCUIT_ID, NEXUS_STATE_TRANSITION_PUBLIC_INPUTS,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{env, fmt};

pub const DEFAULT_DATABASE_URL: &str = "postgres://postgres:password@localhost:5432/nexus";
pub const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379";
pub const DEFAULT_STACKS_NODE_RPC_URL: &str = "https://api.mainnet.hiro.so/";

pub const ENV_ALLOW_DEFAULT_DB: &str = "ALLOW_DEFAULT_DB";
pub const ENV_ALLOW_DEFAULT_REDIS: &str = "ALLOW_DEFAULT_REDIS";
pub const ENV_EXPERIMENTAL_APIS: &str = "NEXUS_EXPERIMENTAL_APIS";
pub const ENV_ORACLE_ENABLED: &str = "ORACLE_ENABLED";
pub const ENV_ORACLE_STUB_OK: &str = "ORACLE_STUB_OK";
pub const ENV_ORACLE_ENDPOINT_URL: &str = "ORACLE_ENDPOINT_URL";
pub const ENV_ORACLE_CONTRACT_PRINCIPAL: &str = "ORACLE_CONTRACT_PRINCIPAL";
pub const ENV_ERP_ATTESTATION_TRUSTED_KEYS: &str = "ERP_ATTESTATION_TRUSTED_KEYS_JSON";
pub const ENV_ADMIN_API_TOKEN: &str = "NEXUS_ADMIN_API_TOKEN";
pub const ENV_BITVM_GROTH16_TRUSTED_REGISTRY: &str = "NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON";

pub const NEXUS_PUBLIC_INPUT_LAYOUT_V1: &str = "nexus-state-transition-v1";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BitvmGroth16TrustedRegistryConfig {
    pub expected_bitcoin_network: String,
    pub records: Vec<BitvmGroth16TrustedRecordConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BitvmGroth16TrustedRecordConfig {
    pub schema_version: u16,
    pub curve: String,
    pub circuit_id: String,
    pub verification_key_id: String,
    pub public_input_count: usize,
    pub public_input_layout: String,
    pub enabled: bool,
    pub verification_key_base64: String,
}

impl BitvmGroth16TrustedRegistryConfig {
    pub fn build_registry(
        &self,
    ) -> anyhow::Result<(BitcoinNetwork, TrustedVerificationKeyRegistry)> {
        use anyhow::{bail, Context};

        if self.records.is_empty() {
            bail!("trusted BitVM Groth16 registry must contain at least one record");
        }
        let network = BitcoinNetwork::parse(&self.expected_bitcoin_network)
            .context("invalid expected Bitcoin network in trusted BitVM Groth16 registry")?;
        let mut registry = TrustedVerificationKeyRegistry::default();
        for (index, record) in self.records.iter().enumerate() {
            if record.schema_version != GROTH16_SCHEMA_VERSION
                || record.curve != "bn254"
                || record.circuit_id != NEXUS_STATE_TRANSITION_CIRCUIT_ID
                || record.public_input_count != NEXUS_STATE_TRANSITION_PUBLIC_INPUTS
                || record.public_input_layout != NEXUS_PUBLIC_INPUT_LAYOUT_V1
            {
                bail!("unsupported trusted BitVM Groth16 metadata at record {index}");
            }
            if record.verification_key_id.len() != 64
                || !record
                    .verification_key_id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                bail!(
                    "trusted BitVM Groth16 verification_key_id at record {index} must be 64 lowercase hexadecimal characters"
                );
            }
            let verification_key_id: [u8; 32] = hex::decode(&record.verification_key_id)
                .context("invalid trusted BitVM Groth16 verification_key_id")?
                .try_into()
                .map_err(|_| {
                    anyhow::anyhow!("invalid trusted BitVM Groth16 verification_key_id width")
                })?;
            let verification_key_bytes = BASE64_STANDARD
                .decode(&record.verification_key_base64)
                .with_context(|| format!("invalid base64 verification key at record {index}"))?;
            registry
                .register(TrustedVerificationKeyConfig {
                    schema_version: record.schema_version,
                    curve: Groth16Curve::Bn254,
                    circuit_id: record.circuit_id.clone(),
                    verification_key_id: VerificationKeyId(verification_key_id),
                    public_input_count: record.public_input_count,
                    public_input_layout: PublicInputLayout::NexusStateTransitionV1,
                    enabled: record.enabled,
                    verification_key_bytes,
                })
                .with_context(|| format!("invalid trusted BitVM Groth16 record {index}"))?;
        }
        Ok((network, registry))
    }
}

/// Whether the OracleService is currently a stub or real.
pub const ORACLE_SERVICE_IS_STUBBED: bool = false;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub rest_port: u16,
    pub grpc_port: u16,
    pub stacks_node_rpc_url: String,
    pub stacks_node_ws_url: String,
    pub gateway_url: Option<String>,
    pub experimental_apis_enabled: bool,
    pub nostr_secret_key: Option<String>,
    pub nostr_relays: Vec<String>,
    pub tableland_base_url: String,
    pub kwil_provider_url: Option<String>,
    pub kwil_db_id: Option<String>,
    pub kwil_private_key_hex: Option<String>,
    pub oracle_enabled: bool,
    pub oracle_stub_ok: bool,
    pub oracle_endpoint_url: Option<String>,
    pub oracle_contract_principal: Option<String>,
    pub erp_attestation_trusted_keys: HashMap<String, String>,
    pub rust_log: String,
    pub worldid_app_id: String,
    pub zkml_vks: HashMap<String, String>,
    pub bitvm_groth16_trusted_registry: Option<BitvmGroth16TrustedRegistryConfig>,
    pub admin_api_token: Option<String>,
    pub admin_public_keys: Vec<String>,
    pub otel_exporter_otlp_endpoint: Option<String>,
    pub otel_service_name: String,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("redis_url", &"<redacted>")
            .field("rest_port", &self.rest_port)
            .field("grpc_port", &self.grpc_port)
            .field("stacks_node_rpc_url", &self.stacks_node_rpc_url)
            .field("stacks_node_ws_url", &self.stacks_node_ws_url)
            .field("gateway_url", &self.gateway_url)
            .field("experimental_apis_enabled", &self.experimental_apis_enabled)
            .field("oracle_enabled", &self.oracle_enabled)
            .field("oracle_stub_ok", &self.oracle_stub_ok)
            .field("oracle_endpoint_url", &self.oracle_endpoint_url)
            .field("oracle_contract_principal", &self.oracle_contract_principal)
            .field("erp_attestation_trusted_keys", &"<redacted>")
            .field("rust_log", &self.rust_log)
            .field("worldid_app_id", &self.worldid_app_id)
            .field("zkml_vks", &"<redacted>")
            .field(
                "bitvm_groth16_trusted_registry",
                &self
                    .bitvm_groth16_trusted_registry
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .field(
                "admin_api_token",
                &self.admin_api_token.as_ref().map(|_| "<redacted>"),
            )
            .field("admin_public_keys", &self.admin_public_keys)
            .field(
                "otel_exporter_otlp_endpoint",
                &self.otel_exporter_otlp_endpoint,
            )
            .field("otel_service_name", &self.otel_service_name)
            .finish()
    }
}

impl Config {
    pub fn default_test() -> Self {
        Self {
            database_url: "postgres://localhost/nexus_test".to_string(),
            redis_url: DEFAULT_REDIS_URL.to_string(),
            rest_port: 3000,
            grpc_port: 50051,
            stacks_node_rpc_url: DEFAULT_STACKS_NODE_RPC_URL.to_string(),
            stacks_node_ws_url: "wss://api.mainnet.hiro.so/".to_string(),
            gateway_url: None,
            experimental_apis_enabled: true,
            nostr_secret_key: None,
            nostr_relays: vec![],
            tableland_base_url: "https://validator.tableland.xyz".to_string(),
            kwil_provider_url: None,
            kwil_db_id: Option::None,
            kwil_private_key_hex: None,
            oracle_enabled: false,
            oracle_stub_ok: true,
            oracle_endpoint_url: None,
            oracle_contract_principal: None,
            erp_attestation_trusted_keys: HashMap::new(),
            rust_log: "info".to_string(),
            worldid_app_id: "".to_string(),
            zkml_vks: HashMap::new(),
            bitvm_groth16_trusted_registry: None,
            admin_api_token: None,
            admin_public_keys: vec![],
            otel_exporter_otlp_endpoint: None,
            otel_service_name: "conxian-nexus".to_string(),
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::{bail, Context};

        let rust_log = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        let allow_default_db = cfg!(debug_assertions) || env_flag(ENV_ALLOW_DEFAULT_DB);
        let database_url = match env::var("DATABASE_URL") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    if allow_default_db {
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
                    DEFAULT_STACKS_NODE_RPC_URL.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(env::VarError::NotPresent) => DEFAULT_STACKS_NODE_RPC_URL.to_string(),
            Err(env::VarError::NotUnicode(_)) => DEFAULT_STACKS_NODE_RPC_URL.to_string(),
        };

        let experimental_apis_enabled = env_flag(ENV_EXPERIMENTAL_APIS);
        let stacks_node_ws_url = env::var("STACKS_NODE_WS_URL")
            .unwrap_or_else(|_| "wss://api.mainnet.hiro.so/".to_string());
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

        let nostr_secret_key = env::var("NOSTR_SECRET_KEY").ok().filter(|s| !s.is_empty());
        let nostr_relays = env::var("NOSTR_RELAYS")
            .unwrap_or_else(|_| "ws://127.0.0.1:8080".to_string())
            .split(",")
            .map(|s| s.trim().to_string())
            .collect();

        let tableland_base_url = env::var("TABLELAND_BASE_URL")
            .unwrap_or_else(|_| "https://validator.tableland.xyz".to_string());
        let kwil_provider_url = env::var("KWIL_PROVIDER_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let kwil_db_id = env::var("KWIL_DB_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let kwil_private_key_hex = env::var("KWIL_PRIVATE_KEY_HEX")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let erp_attestation_trusted_keys = match env::var(ENV_ERP_ATTESTATION_TRUSTED_KEYS) {
            Ok(raw) => serde_json::from_str(&raw)
                .context("Failed to parse ERP_ATTESTATION_TRUSTED_KEYS_JSON")?,
            Err(_) => HashMap::new(),
        };

        let worldid_app_id = env::var("WORLDID_APP_ID").unwrap_or_default();

        let otel_exporter_otlp_endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let otel_service_name =
            env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "conxian-nexus".to_string());
        let admin_api_token = env::var(ENV_ADMIN_API_TOKEN)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let admin_public_keys = env::var("ADMIN_PUBLIC_KEYS")
            .unwrap_or_default()
            .split(",")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut zkml_vks = HashMap::new();
        for (key, value) in env::vars() {
            if key.starts_with("ZKML_VK_B64_") {
                zkml_vks.insert(key, value);
            }
        }

        let bitvm_groth16_trusted_registry = match env::var(ENV_BITVM_GROTH16_TRUSTED_REGISTRY) {
            Ok(raw) => {
                let parsed: BitvmGroth16TrustedRegistryConfig = serde_json::from_str(&raw)
                    .context("Failed to parse NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON")?;
                parsed
                    .build_registry()
                    .context("Invalid NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON")?;
                Some(parsed)
            }
            Err(env::VarError::NotPresent) => None,
            Err(env::VarError::NotUnicode(_)) => {
                bail!("NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON must be valid unicode")
            }
        };

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
                .context("Invalid REST_PORT")?,
            grpc_port: env::var("GRPC_PORT")
                .unwrap_or_else(|_| "50051".to_string())
                .parse()
                .context("Invalid GRPC_PORT")?,
            stacks_node_rpc_url,
            stacks_node_ws_url,
            gateway_url: env::var("GATEWAY_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            experimental_apis_enabled,
            oracle_enabled,
            oracle_stub_ok,
            oracle_endpoint_url,
            oracle_contract_principal,
            erp_attestation_trusted_keys,
            rust_log,
            worldid_app_id,
            zkml_vks,
            bitvm_groth16_trusted_registry,
            admin_api_token,
            admin_public_keys,
            otel_exporter_otlp_endpoint,
            otel_service_name,
        })
    }
}

pub fn env_flag(key: &str) -> bool {
    env::var(key).map(|v| parse_flag(&v)).unwrap_or(false)
}

pub fn parse_flag(v: &str) -> bool {
    let low = v.to_lowercase();
    low == "1" || low == "true" || low == "yes" || low == "on"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_consolidation() {
        env::set_var("DATABASE_URL", "postgres://localhost/test_consolidation");
        env::set_var("REDIS_URL", "redis://localhost/test_consolidation");
        env::set_var("RUST_LOG", "debug");
        env::set_var(ENV_ERP_ATTESTATION_TRUSTED_KEYS, r#"{"key1": "secret1"}"#);
        env::set_var("WORLDID_APP_ID", "app123");
        env::set_var("ZKML_VK_B64_MODEL1", "vk123");

        let config = Config::from_env().unwrap();
        assert_eq!(
            config.database_url,
            "postgres://localhost/test_consolidation"
        );
        assert_eq!(config.redis_url, "redis://localhost/test_consolidation");
        assert_eq!(config.rust_log, "debug");
        assert_eq!(
            config.erp_attestation_trusted_keys.get("key1").unwrap(),
            "secret1"
        );
        assert_eq!(config.worldid_app_id, "app123");
        assert_eq!(config.zkml_vks.get("ZKML_VK_B64_MODEL1").unwrap(), "vk123");
    }
}
