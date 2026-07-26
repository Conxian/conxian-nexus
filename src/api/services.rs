use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceStatus {
    pub service_name: String,
    pub status: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiProtocolStatus {
    pub services: Vec<ServiceStatus>,
}

pub fn get_all_services_status() -> MultiProtocolStatus {
    MultiProtocolStatus {
        services: vec![
            service_status("Bisq", "v1.2.0"),
            service_status("RGB", "v0.10.0"),
            service_status("BitVM", "v0.1.0"),
        ],
    }
}

fn service_status(service_name: &str, version: &str) -> ServiceStatus {
    ServiceStatus {
        service_name: service_name.to_string(),
        status: "Active".to_string(),
        version: version.to_string(),
    }
}

pub(crate) fn get_grpc_services_status() -> Vec<crate::api::grpc::proto::ServiceStatus> {
    get_all_services_status()
        .services
        .into_iter()
        .map(|service| crate::api::grpc::proto::ServiceStatus {
            service_name: service.service_name,
            status: service.status,
            version: service.version,
        })
        .collect()
}
use crate::api::rest::AppState;
use axum::{response::IntoResponse, routing::get, Json, Router};
pub fn services_routes() -> Router<AppState> {
    Router::new().route("/status", get(get_services_status_handler))
}
async fn get_services_status_handler() -> impl IntoResponse {
    Json(get_all_services_status())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn rest_status_serialization_preserves_existing_records() {
        let value = serde_json::to_value(get_all_services_status()).expect("serializable status");
        assert_eq!(
            value,
            serde_json::json!({
                "services": [
                    {"service_name": "Bisq", "status": "Active", "version": "v1.2.0"},
                    {"service_name": "RGB", "status": "Active", "version": "v0.10.0"},
                    {"service_name": "BitVM", "status": "Active", "version": "v0.1.0"}
                ]
            })
        );
    }

    #[test]
    fn grpc_status_serialization_preserves_existing_records() {
        let response = crate::api::grpc::proto::ServicesResponse {
            services: get_grpc_services_status(),
        };
        let encoded = response.encode_to_vec();
        let decoded = crate::api::grpc::proto::ServicesResponse::decode(encoded.as_slice())
            .expect("valid protobuf");
        let records = decoded
            .services
            .into_iter()
            .map(|service| (service.service_name, service.status, service.version))
            .collect::<Vec<_>>();

        assert_eq!(
            records,
            vec![
                (
                    "Bisq".to_string(),
                    "Active".to_string(),
                    "v1.2.0".to_string()
                ),
                (
                    "RGB".to_string(),
                    "Active".to_string(),
                    "v0.10.0".to_string()
                ),
                (
                    "BitVM".to_string(),
                    "Active".to_string(),
                    "v0.1.0".to_string()
                ),
            ]
        );
    }
}
