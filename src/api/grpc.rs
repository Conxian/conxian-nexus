use crate::state::NexusState;
use crate::storage::Storage;
use crate::executor::NexusExecutor;
use std::sync::Arc;
use tonic::{Request, Response, Status, transport::Server};
use sqlx::Row;

pub mod nexus_proto {
    tonic::include_proto!("nexus");
}

use nexus_proto::nexus_service_server::{NexusService, NexusServiceServer};
use nexus_proto::{
    ProofRequest, ProofResponse, VerifyStateRequest, VerifyStateResponse,
    StatusRequest, StatusResponse, ServicesRequest, ServicesResponse,
    MetricsRequest, MetricsResponse, ExecuteRequest, ExecuteResponse,
    ServiceStatus as ProtoServiceStatus
};

pub struct MyNexusService {
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    executor: Arc<NexusExecutor>,
}

#[tonic::async_trait]
impl NexusService for MyNexusService {
    async fn get_proof(
        &self,
        request: Request<ProofRequest>,
    ) -> Result<Response<ProofResponse>, Status> {
        let req = request.into_inner();
        let (hash, proof) = self.nexus_state.generate_proof(&req.key);
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
        }))
    }

    async fn get_status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let state_root = self.nexus_state.get_state_root();

        let row = sqlx::query("SELECT MAX(height) as max_height FROM stacks_blocks")
            .fetch_one(&self.storage.pg_pool)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let processed_height: Option<i64> = row.get("max_height");

        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| Status::internal(format!("Redis error: {}", e)))?;

        let safety_mode: bool = redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async(&mut conn)
            .await
            .unwrap_or(false);

        let drift: u64 = redis::cmd("GET")
            .arg("nexus:drift")
            .query_async(&mut conn)
            .await
            .unwrap_or(0);

        Ok(Response::new(StatusResponse {
            state_root,
            processed_height: processed_height.unwrap_or(0) as u64,
            safety_mode,
            drift,
        }))
    }

    async fn get_metrics(
        &self,
        _request: Request<MetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        let tx_count: i64 = sqlx::query("SELECT COUNT(*) FROM stacks_transactions")
            .fetch_one(&self.storage.pg_pool)
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);

        let block_count: i64 = sqlx::query("SELECT COUNT(*) FROM stacks_blocks")
            .fetch_one(&self.storage.pg_pool)
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);

        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await
            .map_err(|e| Status::internal(format!("Redis error: {}", e)))?;
        let safety_mode: bool = redis::cmd("GET").arg("nexus:safety_mode").query_async(&mut conn).await.unwrap_or(false);
        let drift: u64 = redis::cmd("GET").arg("nexus:drift").query_async(&mut conn).await.unwrap_or(0);

        let uptime = crate::api::get_uptime();

        Ok(Response::new(MetricsResponse {
            total_transactions: tx_count as u64,
            total_blocks: block_count as u64,
            safety_mode,
            drift,
            uptime_seconds: uptime,
        }))
    }

    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let req = request.into_inner();
        let execution_request = crate::executor::ExecutionRequest {
            tx_id: req.tx_id.clone(),
            payload: req.payload,
            sender: req.sender,
            timestamp: chrono::DateTime::parse_from_rfc3339(&req.timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        };

        match self.executor.validate_transaction(&execution_request).await {
            Ok(true) => {
                Ok(Response::new(ExecuteResponse {
                    tx_id: req.tx_id,
                    status: "Success".to_string(),
                    message: "Transaction validated by FSOC Sequencer and executed.".to_string(),
                }))
            }
            Ok(false) => {
                Ok(Response::new(ExecuteResponse {
                    tx_id: req.tx_id,
                    status: "Rejected".to_string(),
                    message: "Transaction rejected by FSOC Sequencer (Potential MEV/Front-running).".to_string(),
                }))
            }
            Err(e) => {
                Err(Status::internal(format!("Internal error during validation: {}", e)))
            }
        }
    }

    async fn get_services(
        &self,
        _request: Request<ServicesRequest>,
    ) -> Result<Response<ServicesResponse>, Status> {
        let status = crate::api::services::get_all_services_status();
        let services = status.services.into_iter().map(|s| ProtoServiceStatus {
            service_name: s.service_name,
            status: s.status,
            version: s.version,
        }).collect();

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
    let nexus_service = MyNexusService {
        storage,
        nexus_state,
        executor,
    };

    tracing::info!("gRPC server listening on {}", addr);

    Server::builder()
        .add_service(NexusServiceServer::new(nexus_service))
        .serve(addr)
        .await?;

    Ok(())
}
