use conxian_nexus::api;
use conxian_nexus::config::{Config, ENV_ORACLE_ENABLED};
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::oracle::OracleService;
use conxian_nexus::safety::NexusSafety;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::Storage;
use conxian_nexus::sync::NexusSync;
use std::future;
use std::sync::Arc;
use tokio::signal;
use tokio::time::{self, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();
    // Initialize logging
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing_subscriber::fmt().with_env_filter(&log_level).init();

    let config = Config::from_env()?;

    tracing::info!("Initializing Conxian Nexus (Glass Node)...");

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
    let executor = Arc::new(NexusExecutor::new(storage.clone()));

    // Initialize Oracle Service
    let oracle_service = if config.oracle_enabled {
        let endpoint_url = config.oracle_endpoint_url.clone().unwrap();
        let contract_principal = config.oracle_contract_principal.clone().unwrap();

        Some(Arc::new(OracleService::new(
            storage.clone(),
            endpoint_url,
            contract_principal,
        )))
    } else {
        None
    };

    // Initialize Services
    let sync_service = Arc::new(NexusSync::new(
        storage.clone(),
        state_tracker.clone(),
        config.stacks_node_rpc_url.clone(),
    ));
    let safety_service = Arc::new(NexusSafety::new(
        storage.clone(),
        config.stacks_node_rpc_url.clone(),
        config.gateway_url.clone(),
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

    // Start REST API Server
    let rest_storage = storage.clone();
    let rest_state = state_tracker.clone();
    let rest_executor = executor.clone();
    let rest_oracle = oracle_service.clone();
    let rest_port = config.rest_port;
    let experimental_apis_enabled = config.experimental_apis_enabled;
    let rest_handle = tokio::spawn(async move {
        if let Err(e) = api::rest::start_rest_server(
            rest_storage,
            rest_state,
            rest_executor,
            rest_oracle,
            rest_port,
            experimental_apis_enabled,
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
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) =
            api::grpc::start_grpc_server(grpc_storage, grpc_state, grpc_executor, grpc_port).await
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
        res = rest_handle => tracing::error!("REST handle exited: {:?}", res),
        res = grpc_handle => tracing::error!("gRPC handle exited: {:?}", res),
    }

    Ok(())
}
