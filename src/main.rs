use anyhow::Context;
use conxian_nexus::api;
use conxian_nexus::api::billing::nostr::NostrTelemetry;
use conxian_nexus::compat::core_bridge::{
    Wallet, ENV_CONXIAN_PRIVATE_KEY_HEX, ENV_NEXUS_PRIVATE_KEY,
};
use conxian_nexus::config::{
    Config, ENV_ORACLE_CONTRACT_PRINCIPAL, ENV_ORACLE_ENABLED, ENV_ORACLE_ENDPOINT_URL,
};
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::executor::{
    bitvm_groth16::CanonicalStateTransitionVerifier,
    canonical_bitvm::{
        CanonicalBitvmService, PostgresCanonicalBitvmReceiptStore, UnavailableBitcoinHeightProvider,
    },
};
use conxian_nexus::oracle::OracleService;
use conxian_nexus::orchestrator::AutonomousOrchestrator;
use conxian_nexus::safety::NexusSafety;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::kwil::{KwilAdapter, KwilConfig};
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use conxian_nexus::sync::NexusSync;
use opentelemetry::{global, trace::TracerProvider};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{self as sdktrace};
use opentelemetry_sdk::Resource;
use std::future;
use std::sync::Arc;
use tokio::signal;
use tokio::time::{self, Duration};
use tracing_subscriber::{prelude::*, EnvFilter};

fn load_oracle_wallet_with<F>(
    oracle_enabled: bool,
    load_wallet: F,
) -> anyhow::Result<Option<Wallet>>
where
    F: FnOnce() -> anyhow::Result<Wallet>,
{
    if !oracle_enabled {
        return Ok(None);
    }

    load_wallet().map(Some).with_context(|| {
        format!(
            "{ENV_ORACLE_ENABLED}=1 requires {ENV_CONXIAN_PRIVATE_KEY_HEX} or legacy {ENV_NEXUS_PRIVATE_KEY}"
        )
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    let config = Config::from_env().context("Failed to load configuration")?;

    // Initialize tracing
    let fmt_layer = tracing_subscriber::fmt::layer().with_filter(EnvFilter::new(&config.rust_log));

    if let Some(endpoint) = &config.otel_exporter_otlp_endpoint {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let tracer = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .expect("Failed to create OTLP exporter");

        let tracer = sdktrace::SdkTracerProvider::builder()
            .with_batch_exporter(tracer)
            .with_resource(
                Resource::builder()
                    .with_service_name(config.otel_service_name.clone())
                    .build(),
            )
            .build()
            .tracer("conxian-nexus");

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        tracing_subscriber::registry()
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry().with(fmt_layer).init();
    }

    // Initialize logging using the centralized config

    tracing::info!(
        "Initializing Conxian Nexus (Glass Node v{})...",
        env!("CARGO_PKG_VERSION")
    );

    // Initialize Global Start Time
    api::init_start_time();

    // Initialize Storage
    let storage = Arc::new(Storage::from_config(&config).await?);

    // Run Database Migrations
    tracing::info!("Running database migrations...");
    storage.run_migrations().await?;

    // Initialize State Tracker
    let state_tracker = Arc::new(NexusState::new());

    // Initialize Executor
    let rgb_mode = if config.experimental_apis_enabled {
        conxian_nexus::executor::rgb::RGBRolloutMode::Shadow
    } else {
        conxian_nexus::executor::rgb::RGBRolloutMode::Disabled
    };
    let mut executor =
        NexusExecutor::new(storage.clone(), rgb_mode, std::collections::HashSet::new());
    if let Some(registry_config) = &config.bitvm_groth16_trusted_registry {
        let (expected_network, registry) = registry_config
            .build_registry()
            .context("Failed to construct canonical BitVM trusted registry")?;
        let service = CanonicalBitvmService::new(
            Arc::new(CanonicalStateTransitionVerifier::new(Arc::new(registry))),
            expected_network,
            Arc::new(UnavailableBitcoinHeightProvider),
            Arc::new(PostgresCanonicalBitvmReceiptStore::new(storage.clone())),
        );
        executor = executor.with_canonical_bitvm_service(Arc::new(service));
        tracing::warn!(
            "Canonical BitVM registry loaded, but verification remains unavailable until a reviewed trusted Bitcoin-height provider is wired"
        );
    } else {
        tracing::info!(
            "Canonical BitVM verification unavailable: NEXUS_BITVM_GROTH16_TRUSTED_REGISTRY_JSON is not configured"
        );
    }
    let executor = Arc::new(executor);

    // Initialize Tableland Adapter [CON-69]
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    // Initialize Kwil Adapter [CON-330]
    let kwil = if let (Some(provider_url), Some(db_id), Some(private_key_hex)) = (
        &config.kwil_provider_url,
        &config.kwil_db_id,
        &config.kwil_private_key_hex,
    ) {
        let wallet = Arc::new(
            Wallet::from_private_key_hex(private_key_hex)
                .context("Invalid KWIL_PRIVATE_KEY_HEX")?,
        );

        Some(Arc::new(KwilAdapter::new(
            storage.clone(),
            KwilConfig {
                provider_url: provider_url.clone(),
                db_id: db_id.clone(),
            },
            wallet,
        )?))
    } else {
        tracing::info!("Kwil persistence disabled (KWIL_* env vars not configured)");
        None
    };

    // Initialize Nostr Telemetry [CON-473]
    let nostr = if let Some(sk) = &config.nostr_secret_key {
        match NostrTelemetry::new(sk, config.nostr_relays.clone()).await {
            Ok(n) => Some(Arc::new(n)),
            Err(e) => {
                tracing::error!("Failed to initialize Nostr telemetry: {}", e);
                None
            }
        }
    } else {
        tracing::info!("Nostr telemetry disabled (NOSTR_SECRET_KEY not set)");
        None
    };

    // Initialize Oracle Service
    let oracle_service = if config.oracle_enabled {
        let endpoint_url = config.oracle_endpoint_url.clone().with_context(|| {
            format!("{ENV_ORACLE_ENABLED}=1 requires {ENV_ORACLE_ENDPOINT_URL}")
        })?;
        let contract_principal = config.oracle_contract_principal.clone().with_context(|| {
            format!("{ENV_ORACLE_ENABLED}=1 requires {ENV_ORACLE_CONTRACT_PRINCIPAL}")
        })?;
        let wallet = load_oracle_wallet_with(true, Wallet::new)?
            .expect("enabled Oracle always returns an injected wallet");

        Some(Arc::new(OracleService::new(
            storage.clone(),
            endpoint_url,
            contract_principal,
            wallet,
        )))
    } else {
        None
    };

    // Initialize Services
    let sync_service = Arc::new(NexusSync::new(
        storage.clone(),
        state_tracker.clone(),
        tableland.clone(),
        kwil.clone(),
        config.stacks_node_rpc_url.clone(),
        config.stacks_node_ws_url.clone(),
    ));
    let safety_service = Arc::new(NexusSafety::new(
        storage.clone(),
        config.stacks_node_rpc_url.clone(),
        config.gateway_url.clone(),
    ));

    // Initialize Autonomous Orchestrator [NEXUS-ORCH-01]
    let orchestrator = Arc::new(AutonomousOrchestrator::new(
        storage.clone(),
        state_tracker.clone(),
        nostr.clone(),
    ));

    // Load Initial State from DB
    sync_service.load_initial_state().await?;

    // Spawn Sync Service
    let sync_handle = {
        let sync = sync_service.clone();
        tokio::spawn(async move {
            if let Err(e) = sync.run().await {
                tracing::error!("Sync service failed: {}", e);
            }
        })
    };

    // Spawn Safety Service (Heartbeat)
    let safety_handle = {
        let safety = safety_service.clone();
        tokio::spawn(async move {
            if let Err(e) = safety.run_heartbeat().await {
                tracing::error!("Safety service failed: {}", e);
            }
        })
    };

    // Spawn Oracle Service
    let oracle_handle = if let Some(ref oracle) = oracle_service {
        let oracle_worker = oracle.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = oracle_worker.run().await {
                tracing::error!("Oracle service failed: {}", e);
            }
        }))
    } else {
        tracing::info!(
            "OracleService disabled (set {}=1 to enable)",
            ENV_ORACLE_ENABLED
        );
        None
    };

    let oracle_join = async move {
        match oracle_handle {
            Some(handle) => handle.await,
            None => future::pending::<Result<(), tokio::task::JoinError>>().await,
        }
    };

    // Spawn Rebalance Background Task
    let rebalance_executor = executor.clone();
    let rebalance_handle = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = rebalance_executor.execute_rebalance().await {
                tracing::error!("Rebalance task failed: {}", e);
            }
        }
    });

    // [NEXUS-04] Spawn Sovereign Health Reporting (Nostr)
    let health_nostr = nostr.clone();
    let health_report_handle = if let Some(n) = health_nostr {
        let health_storage = storage.clone();
        let health_state = state_tracker.clone();
        Some(tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(300)); // Every 5 mins
            loop {
                interval.tick().await;
                let max_height = sqlx::query_scalar::<_, Option<i64>>(
                    "SELECT MAX(height) FROM stacks_blocks WHERE type = 'burn_block' AND state = 'hard'",
                )
                .fetch_one(&health_storage.pg_pool)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "Health report height query failed");
                    e
                })
                .ok()
                .flatten()
                .unwrap_or(0)
                .max(0) as u64;
                let state_root = health_state.get_state_root();

                if let Err(e) = n
                    .report_health_nostr("ALIVE", max_height, &state_root)
                    .await
                {
                    tracing::error!("Failed to report health to Nostr: {}", e);
                }
            }
        }))
    } else {
        None
    };

    let health_join = async move {
        match health_report_handle {
            Some(handle) => handle.await,
            None => future::pending::<Result<(), tokio::task::JoinError>>().await,
        }
    };

    // Spawn Autonomous Orchestrator [NEXUS-ORCH-01]
    let orch_worker = orchestrator.clone();
    let orch_handle = tokio::spawn(async move {
        if let Err(e) = orch_worker.run().await {
            tracing::error!("Orchestrator failed: {}", e);
        }
    });

    // Start REST API Server
    let rest_storage = storage.clone();
    let rest_state = state_tracker.clone();
    let rest_executor = executor.clone();
    let rest_oracle = oracle_service.clone();
    let rest_tableland = tableland.clone();
    let rest_kwil = kwil.clone();
    let rest_nostr = nostr.clone();
    let rest_port = config.rest_port;
    let rest_config = Arc::new(config.clone());
    let rest_handle = tokio::spawn(async move {
        if let Err(e) = api::rest::start_rest_server(
            rest_storage,
            rest_state,
            rest_executor,
            rest_oracle,
            rest_tableland,
            rest_kwil,
            rest_nostr,
            rest_port,
            rest_config,
        )
        .await
        {
            tracing::error!("REST API server failed: {}", e);
        }
    });

    // Start gRPC API Server
    let grpc_storage = storage.clone();
    let grpc_state = state_tracker.clone();
    let grpc_executor = executor.clone();
    let grpc_port = config.grpc_port;
    let grpc_skip_auth = cfg!(debug_assertions); // Skip auth in debug builds only
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = api::grpc::start_grpc_server(
            grpc_storage,
            grpc_state,
            grpc_executor,
            grpc_port,
            grpc_skip_auth,
        )
        .await
        {
            tracing::error!("gRPC API server failed: {}", e);
        }
    });

    tracing::info!("All Nexus services are running.");

    // Graceful shutdown handling
    let shutdown = async {
        signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C handler");
        tracing::info!("Shutdown signal received");
    };

    tokio::select! {
        _ = shutdown => tracing::info!("Shutting down..."),
        res = sync_handle => tracing::error!("Sync service exited: {:?}", res),
        res = safety_handle => tracing::error!("Safety service exited: {:?}", res),
        res = oracle_join => tracing::error!("Oracle service exited: {:?}", res),
        res = rebalance_handle => tracing::error!("Rebalance task exited: {:?}", res),
        res = health_join => tracing::error!("Health report task exited: {:?}", res),
        res = orch_handle => tracing::error!("Orchestrator task exited: {:?}", res),
        res = rest_handle => tracing::error!("REST handle exited: {:?}", res),
        res = grpc_handle => tracing::error!("gRPC handle exited: {:?}", res),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_wallet() -> Wallet {
        let mut key = [0_u8; 32];
        key[31] = 1;
        Wallet::from_private_key_bytes(&key).expect("fixed test key")
    }

    #[test]
    fn disabled_oracle_does_not_load_signer() {
        let wallet = load_oracle_wallet_with(false, || panic!("signer loader must not run"))
            .expect("disabled Oracle");
        assert!(wallet.is_none());
    }

    #[test]
    fn enabled_oracle_accepts_valid_signer() {
        let wallet = load_oracle_wallet_with(true, || Ok(fixed_wallet()))
            .expect("enabled Oracle signer")
            .expect("wallet");
        assert_eq!(
            wallet.public_key(),
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
    }

    #[test]
    fn enabled_oracle_rejects_missing_signer() {
        let error = load_oracle_wallet_with(true, || anyhow::bail!("missing private key"))
            .err()
            .expect("missing signer rejected");
        assert!(error.to_string().contains("CONXIAN_PRIVATE_KEY_HEX"));
        assert!(error.to_string().contains("NEXUS_PRIVATE_KEY"));
    }

    #[test]
    fn enabled_oracle_rejects_invalid_signer() {
        let error = load_oracle_wallet_with(true, || Wallet::from_private_key_hex("not-hex"))
            .err()
            .expect("invalid signer rejected");
        assert!(error.to_string().contains("CONXIAN_PRIVATE_KEY_HEX"));
    }
}
