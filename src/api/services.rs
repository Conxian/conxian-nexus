use lib_conxian_core::gateway::{BisqService, BitVMService, ConxianService, RGBService};
use serde::Serialize;

#[derive(Serialize)]
pub struct MultiProtocolStatus {
    pub services: Vec<lib_conxian_core::gateway::ServiceStatus>,
}

pub fn get_all_services_status() -> MultiProtocolStatus {
    let bisq = BisqService;
    let rgb = RGBService;
    let bitvm = BitVMService;

    MultiProtocolStatus {
        services: vec![bisq.status(), rgb.status(), bitvm.status()],
    }
}
use crate::api::rest::AppState;
use axum::{response::IntoResponse, routing::get, Json, Router};
pub fn services_routes() -> Router<AppState> {
    Router::new().route("/status", get(get_services_status_handler))
}
async fn get_services_status_handler() -> impl IntoResponse {
    Json(get_all_services_status())
}
