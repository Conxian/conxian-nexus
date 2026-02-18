use tonic::{transport::Server, Request, Response, Status};
use std::sync::Arc;
use crate::storage::Storage;

pub mod nexus_proto {
    tonic::include_proto!("nexus");
}

use nexus_proto::nexus_service_server::{NexusService, NexusServiceServer};
use nexus_proto::{ProofRequest, ProofResponse, VerifyStateRequest, VerifyStateResponse};

pub struct MyNexusService {
    _storage: Arc<Storage>,
}

#[tonic::async_trait]
impl NexusService for MyNexusService {
    async fn get_proof(
        &self,
        request: Request<ProofRequest>,
    ) -> Result<Response<ProofResponse>, Status> {
        let req = request.into_inner();
        Ok(Response::new(ProofResponse {
            hash: format!("hash_for_{}", req.key),
            proof: "dummy_proof".to_string(),
        }))
    }

    async fn verify_state(
        &self,
        request: Request<VerifyStateRequest>,
    ) -> Result<Response<VerifyStateResponse>, Status> {
        let req = request.into_inner();
        Ok(Response::new(VerifyStateResponse {
            valid: req.state_root.starts_with("0x"),
        }))
    }
}

pub async fn start_grpc_server(storage: Arc<Storage>) -> anyhow::Result<()> {
    let addr = "0.0.0.0:50051".parse()?;
    let nexus_service = MyNexusService { _storage: storage };

    tracing::info!("gRPC server listening on {}", addr);

    Server::builder()
        .add_service(NexusServiceServer::new(nexus_service))
        .serve(addr)
        .await?;

    Ok(())
}
