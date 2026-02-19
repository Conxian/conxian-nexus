use crate::state::NexusState;
use crate::storage::Storage;
use std::sync::Arc;
use tonic::{Request, Response, Status, transport::Server};

pub mod nexus_proto {
    tonic::include_proto!("nexus");
}

use nexus_proto::nexus_service_server::{NexusService, NexusServiceServer};
use nexus_proto::{ProofRequest, ProofResponse, VerifyStateRequest, VerifyStateResponse};

pub struct MyNexusService {
    _storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
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
}

pub async fn start_grpc_server(
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    port: u16,
) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port).parse()?;
    let nexus_service = MyNexusService {
        _storage: storage,
        nexus_state,
    };

    tracing::info!("gRPC server listening on {}", addr);

    Server::builder()
        .add_service(NexusServiceServer::new(nexus_service))
        .serve(addr)
        .await?;

    Ok(())
}
