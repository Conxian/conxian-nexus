use crate::executor::{ExecutionRequest, NexusExecutor};
use crate::state::NexusState;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

// Proto generated code
pub mod proto {
    tonic::include_proto!("nexus");
}

use proto::nexus_service_server::NexusService;
use proto::*;

pub struct NexusGrpcService {
    pub storage: Arc<Storage>,
    pub nexus_state: Arc<NexusState>,
    pub executor: Arc<NexusExecutor>,
    metrics_counts_cache: MetricsCountsCache,
    redis_conn: Mutex<Option<redis::aio::MultiplexedConnection>>,
}

const METRICS_COUNTS_CACHE_TTL: Duration = Duration::from_secs(10);

struct MetricsCountsCacheState {
    value: Option<(Instant, u64, u64)>,
}

struct MetricsCountsCache {
    state: Mutex<MetricsCountsCacheState>,
    refresh_lock: Mutex<()>,
}

impl MetricsCountsCache {
    fn new() -> Self {
        Self {
            state: Mutex::new(MetricsCountsCacheState { value: None }),
            refresh_lock: Mutex::new(()),
        }
    }
}

impl NexusGrpcService {
    async fn read_fresh_cached_metrics_counts(&self) -> Option<(u64, u64)> {
        let cache_guard = self.metrics_counts_cache.state.lock().await;
        if let Some((cached_at, cached_tx_count, cached_block_count)) = cache_guard.value {
            if cached_at.elapsed() < METRICS_COUNTS_CACHE_TTL {
                return Some((cached_tx_count, cached_block_count));
            }
        }

        None
    }
    async fn fetch_metrics_counts(&self) -> Result<(u64, u64), Status> {
        let (tx_count, block_count): (i64, i64) = sqlx::query_as(
            "SELECT \
                (SELECT COUNT(*) FROM stacks_transactions t \
                 JOIN stacks_blocks b ON t.block_hash = b.hash \
                 WHERE b.state != 'orphaned') AS tx_count, \
                (SELECT COUNT(*) FROM stacks_blocks WHERE state != 'orphaned') AS block_count",
        )
        .fetch_one(&self.storage.pg_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Database error in GetMetrics (counts)");
            Status::internal("Database error in GetMetrics (counts)")
        })?;

        Ok((tx_count as u64, block_count as u64))
    }

    async fn read_cached_metrics_counts(&self) -> Result<(u64, u64), Status> {
        if let Some(counts) = self.read_fresh_cached_metrics_counts().await {
            return Ok(counts);
        }

        let _refresh_guard = self.metrics_counts_cache.refresh_lock.lock().await;

        if let Some(counts) = self.read_fresh_cached_metrics_counts().await {
            return Ok(counts);
        }

        let (tx_count, block_count) = self.fetch_metrics_counts().await?;
        let mut cache_guard = self.metrics_counts_cache.state.lock().await;
        cache_guard.value = Some((Instant::now(), tx_count, block_count));
        Ok((tx_count, block_count))
    }

    async fn read_safety_flags(&self, context: &str) -> Result<(bool, u64), Status> {
        let default_flags = (true, 0);

        let redis_client = self.storage.redis_client.clone();
        let cached_conn = { self.redis_conn.lock().await.clone() };

        let mut conn = match cached_conn {
            Some(conn) => conn,
            None => {
                let conn = match redis_client.get_multiplexed_async_connection().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            context,
                            "Redis error reading safety flags (connect); defaulting safe"
                        );
                        return Ok(default_flags);
                    }
                };

                *self.redis_conn.lock().await = Some(conn.clone());
                conn
            }
        };

        let pipeline_result: Result<(Option<String>, Option<String>), redis::RedisError> =
            redis::pipe()
                .cmd("GET")
                .arg("nexus:safety_mode")
                .cmd("GET")
                .arg("nexus:drift")
                .query_async(&mut conn)
                .await;

        let (safety_raw, drift_raw): (Option<String>, Option<String>) = match pipeline_result {
            Ok(result) => result,
            Err(e) => {
                *self.redis_conn.lock().await = None;
                tracing::warn!(
                    error = %e,
                    context,
                    "Redis error reading safety flags (pipeline); retrying once"
                );

                let mut conn = match redis_client.get_multiplexed_async_connection().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            context,
                            "Redis error reading safety flags (connect); defaulting safe"
                        );
                        return Ok(default_flags);
                    }
                };

                match redis::pipe()
                    .cmd("GET")
                    .arg("nexus:safety_mode")
                    .cmd("GET")
                    .arg("nexus:drift")
                    .query_async(&mut conn)
                    .await
                {
                    Ok(result) => {
                        *self.redis_conn.lock().await = Some(conn.clone());
                        result
                    }
                    Err(e) => {
                        *self.redis_conn.lock().await = None;
                        tracing::error!(
                            error = %e,
                            context,
                            "Redis error reading safety flags (pipeline); defaulting safe"
                        );
                        return Ok(default_flags);
                    }
                }
            }
        };

        let safety_mode: bool = match safety_raw.as_deref() {
            None => false,
            Some(raw) => {
                let normalized = raw.trim().to_ascii_lowercase();
                if crate::config::parse_flag(&normalized) {
                    true
                } else if matches!(normalized.as_str(), "" | "0" | "false" | "no" | "off") {
                    false
                } else {
                    tracing::warn!(
                        context,
                        value = %raw,
                        "Unrecognized nexus:safety_mode value in Redis; treating as false"
                    );
                    false
                }
            }
        };

        let drift: u64 = match drift_raw.as_deref() {
            None => 0,
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    0
                } else if let Ok(value) = trimmed.parse::<u64>() {
                    value
                } else {
                    tracing::warn!(
                        context,
                        value = %raw,
                        "Unrecognized nexus:drift value in Redis; treating as 0"
                    );
                    0
                }
            }
        };

        Ok((safety_mode, drift))
    }
}

#[tonic::async_trait]
impl NexusService for NexusGrpcService {
    async fn get_proof(
        &self,
        request: Request<ProofRequest>,
    ) -> Result<Response<ProofResponse>, Status> {
        let req = request.into_inner();
        let (hash, proof) = self
            .nexus_state
            .generate_merkle_proof(&req.key)
            .map(|p| {
                (
                    p.root.clone(),
                    serde_json::to_string(&p).unwrap_or_default(),
                )
            })
            .unwrap_or_else(|| (self.nexus_state.get_state_root(), "{}".to_string()));

        Ok(Response::new(ProofResponse { hash, proof }))
    }

    async fn verify_state(
        &self,
        request: Request<VerifyStateRequest>,
    ) -> Result<Response<VerifyStateResponse>, Status> {
        let req = request.into_inner();
        let current_root = self.nexus_state.get_state_root();
        Ok(Response::new(VerifyStateResponse {
            valid: current_root == req.state_root,
            mmr_root: self.nexus_state.get_mmr_root(),
        }))
    }

    async fn get_status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let max_height: Option<i64> =
            sqlx::query_scalar("SELECT MAX(height) FROM stacks_blocks WHERE state != 'orphaned'")
                .fetch_one(&self.storage.pg_pool)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "Database error in GetStatus (max_height)");
                    Status::internal("Database error in GetStatus (max_height)")
                })?;

        let processed_height: u64 = max_height.unwrap_or(0).max(0) as u64;

        let (safety_mode, drift) = self.read_safety_flags("GetStatus").await?;

        Ok(Response::new(StatusResponse {
            state_root: self.nexus_state.get_state_root(),
            mmr_root: self.nexus_state.get_mmr_root(),
            processed_height,
            safety_mode,
            drift,
        }))
    }

    async fn get_metrics(
        &self,
        _request: Request<MetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        let (tx_count, block_count) = self.read_cached_metrics_counts().await?;

        let (safety_mode, drift) = self.read_safety_flags("GetMetrics").await?;

        Ok(Response::new(MetricsResponse {
            total_transactions: tx_count,
            total_blocks: block_count,
            safety_mode,
            drift,
            uptime_seconds: crate::api::get_uptime(),
        }))
    }

    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let req = request.into_inner();
        let timestamp = if req.timestamp.is_empty() {
            Utc::now()
        } else {
            req.timestamp
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now())
        };

        let exec_req = ExecutionRequest {
            tx_id: req.tx_id.clone(),
            payload: req.payload,
            sender: req.sender,
            timestamp,
        };

        match self.executor.validate_transaction(&exec_req).await {
            Ok(true) => Ok(Response::new(ExecuteResponse {
                tx_id: req.tx_id,
                status: "Success".to_string(),
                message: "Validated".to_string(),
            })),
            _ => Ok(Response::new(ExecuteResponse {
                tx_id: req.tx_id,
                status: "Rejected".to_string(),
                message: "Rejected".to_string(),
            })),
        }
    }

    async fn get_services(
        &self,
        _request: Request<ServicesRequest>,
    ) -> Result<Response<ServicesResponse>, Status> {
        let multi_status = crate::api::services::get_all_services_status();
        let services = multi_status
            .services
            .into_iter()
            .map(|s| ServiceStatus {
                service_name: s.service_name,
                status: s.status,
                version: s.version,
            })
            .collect();
        Ok(Response::new(ServicesResponse { services }))
    }
}

pub async fn start_grpc_server(
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    executor: Arc<NexusExecutor>,
    port: u16,
) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port).parse()?;
    let nexus_service = NexusGrpcService {
        storage,
        nexus_state,
        executor,
        metrics_counts_cache: MetricsCountsCache::new(),
        redis_conn: Mutex::new(None),
    };

    tracing::info!("gRPC server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(proto::nexus_service_server::NexusServiceServer::new(
            nexus_service,
        ))
        .serve(addr)
        .await?;

    Ok(())
}
