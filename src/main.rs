pub mod api;
pub mod storage;
pub mod sync;
pub mod executor;
pub mod safety;
pub mod config;

use std::sync::Arc;
use tokio::signal;
use crate::storage::Storage;
use crate::sync::NexusSync;
use crate::safety::NexusSafety;
use crate::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    let config = Config::from_env();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    tracing::info!("Initializing Conxian Nexus (Glass Node)...");

    // Initialize Storage
    let storage = Arc::new(Storage::new().await?);

    // Run Database Migrations
    tracing::info!("Running database migrations...");
    storage.run_migrations().await?;

    // Initialize Services
    let sync_service = Arc::new(NexusSync::new(storage.clone()));
    let safety_service = Arc::new(NexusSafety::new(storage.clone()));

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

    // Start REST API Server
    let rest_storage = storage.clone();
    let rest_port = config.rest_port;
    let rest_handle = tokio::spawn(async move {
        if let Err(e) = api::rest::start_rest_server(rest_storage, rest_port).await {
            tracing::error!("REST API server failed: {}", e);
        }
    });

    // Start gRPC API Server
    let grpc_storage = storage.clone();
    let grpc_port = config.grpc_port;
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = api::grpc::start_grpc_server(grpc_storage, grpc_port).await {
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
        res = rest_handle => tracing::error!("REST handle exited: {:?}", res),
        res = grpc_handle => tracing::error!("gRPC handle exited: {:?}", res),
    }

    Ok(())
}
