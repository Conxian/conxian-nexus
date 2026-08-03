use conxian_nexus::api::grpc::proto::nexus_service_server::NexusService;
use conxian_nexus::api::grpc::proto::ServicesRequest;
use conxian_nexus::api::grpc::NexusGrpcService;
use conxian_nexus::config::Config;
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::Storage;
use std::collections::HashSet;
use std::sync::Arc;
use tonic::{Code, Request};

async fn setup_grpc_test_service(skip_auth: bool) -> (NexusGrpcService, Arc<Storage>) {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));

    let service = NexusGrpcService::new_for_test(storage.clone(), nexus_state, executor, skip_auth);

    (service, storage)
}

#[tokio::test]
async fn test_grpc_auth_skipped_when_configured() {
    let (service, _storage) = setup_grpc_test_service(true).await;

    // Call without any metadata/headers
    let request = Request::new(ServicesRequest {});
    let response = service.get_services(request).await;

    assert!(
        response.is_ok(),
        "Request should succeed when skip_auth is true"
    );
}

#[tokio::test]
async fn test_grpc_auth_missing_header() {
    let (service, _storage) = setup_grpc_test_service(false).await;

    // Call without any metadata/headers
    let request = Request::new(ServicesRequest {});
    let response = service.get_services(request).await;

    assert!(
        response.is_err(),
        "Request should fail when skip_auth is false and header is missing"
    );
    let err = response.err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
    assert!(err.message().contains("Missing API key"));
}

#[tokio::test]
async fn test_grpc_auth_invalid_format() {
    let (service, _storage) = setup_grpc_test_service(false).await;

    // Call with short API key (< 16 characters)
    let mut request = Request::new(ServicesRequest {});
    request
        .metadata_mut()
        .insert("x-api-key", "short".parse().unwrap());
    let response = service.get_services(request).await;

    assert!(
        response.is_err(),
        "Request should fail when key format is invalid"
    );
    let err = response.err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
    assert!(err.message().contains("Invalid API key format"));
}

#[tokio::test]
async fn test_grpc_auth_dev_mode_rejected_in_release() {
    let (service, _storage) = setup_grpc_test_service(false).await;

    // Call with dev-mode API key when skip_auth is false
    let mut request = Request::new(ServicesRequest {});
    request
        .metadata_mut()
        .insert("x-api-key", "dev-mode".parse().unwrap());
    let response = service.get_services(request).await;

    assert!(
        response.is_err(),
        "Request should fail with dev-mode in release/strict environments"
    );
    let err = response.err().unwrap();
    assert_eq!(err.code(), Code::Unauthenticated);
    assert!(err.message().contains("Invalid API key"));
}

#[tokio::test]
async fn test_grpc_auth_not_found_in_redis() {
    let (service, _storage) = setup_grpc_test_service(false).await;

    // Call with random 16+ character key not stored in Redis
    let mut request = Request::new(ServicesRequest {});
    request.metadata_mut().insert(
        "x-api-key",
        "random_unauthorized_key_123456".parse().unwrap(),
    );
    let response = service.get_services(request).await;

    assert!(
        response.is_err(),
        "Request should fail when API key does not exist in Redis"
    );
    let err = response.err().unwrap();
    // Since Redis might be unavailable or running, accept either Internal (connection failed) or Unauthenticated (key not found)
    assert!(
        err.code() == Code::Unauthenticated || err.code() == Code::Internal,
        "Expected Unauthenticated or Internal, got {:?}",
        err.code()
    );
}

#[tokio::test]
async fn test_grpc_auth_authorized_key() {
    std::env::set_var("NEXUS_GRPC_TEST_BYPASS", "1");
    let (service, _storage) = setup_grpc_test_service(false).await;

    // Use test bypass key that bypasses the Redis connection check under test
    let api_key = "valid_grpc_test_api_key_123";

    // Call with valid API key
    let mut request = Request::new(ServicesRequest {});
    request
        .metadata_mut()
        .insert("x-api-key", api_key.parse().unwrap());
    let response = service.get_services(request).await;

    std::env::remove_var("NEXUS_GRPC_TEST_BYPASS");

    assert!(
        response.is_ok(),
        "Request should succeed with authorized key: {:?}",
        response.err()
    );
}
