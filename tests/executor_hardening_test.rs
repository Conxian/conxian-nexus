use conxian_nexus::executor::{ExecutionRequest};
use chrono::Utc;

#[tokio::test]
async fn test_execution_request_priority_serialization() {
    let req = ExecutionRequest {
        tx_id: "tx_123".to_string(),
        payload: "test".to_string(),
        timestamp: Utc::now(),
        sender: "alice".to_string(),
        priority: 10,
    };
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: ExecutionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.priority, 10);
}
