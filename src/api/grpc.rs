use crate::executor::{ExecutionRequest, NexusExecutor};
use crate::state::NexusState;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use std::sync::Arc;
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
        Ok(Response::new(StatusResponse {
            state_root: self.nexus_state.get_state_root(),
            mmr_root: self.nexus_state.get_mmr_root(),
            processed_height: 0,
            safety_mode: false,
            drift: 0,
        }))
    }

    async fn get_metrics(
        &self,
        _request: Request<MetricsRequest>,
    ) -> Result<Response<MetricsResponse>, Status> {
        Ok(Response::new(MetricsResponse {
            total_transactions: 0,
            total_blocks: 0,
            safety_mode: false,
            drift: 0,
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
