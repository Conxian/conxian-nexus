//! MCP-402 HTTP surface.
//!
//! Exposes the server-side [`crate::mcp_402::Mcp402Facade`] gate over HTTP,
//! mirroring `conxian_market`'s `mcp_402.ts` facade. Two endpoints:
//!
//! - `POST /v1/mcp402/demand`  — build an x402 demand from an MCP tool call.
//! - `POST /v1/mcp402/authorize` — gate dispatch on a receipt, optionally with
//!   a cryptographically verified x402 payment proof.
//!
//! Nexus observes and proves; it does not settle or custody funds.

use crate::api::rest::AppState;
use crate::mcp_402::{Mcp402Receipt, McpToolCall};
use crate::verification::X402PaymentPayload;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

/// Router for the MCP-402 payment gate.
pub fn mcp402_routes() -> Router<AppState> {
    Router::new()
        .route("/demand", post(mcp402_demand_handler))
        .route("/authorize", post(mcp402_authorize_handler))
}

/// Request body for the authorize gate: a tool call, a receipt, and an
/// optional x402 payment proof (structural-only when absent).
#[derive(Debug, Deserialize)]
pub struct Mcp402AuthorizeRequest {
    pub call: McpToolCall,
    pub receipt: Mcp402Receipt,
    #[serde(default)]
    pub proof: Option<X402PaymentPayload>,
}

/// Build the x402 demand for an MCP tool invocation.
pub async fn mcp402_demand_handler(
    State(state): State<AppState>,
    Json(call): Json<McpToolCall>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(state.mcp402.demand(&call))).into_response()
}

/// Gate MCP tool dispatch on a receipt, and on a payment proof when supplied.
///
/// Fail-closed: denials are reported in-band as `authorized: false` with a
/// reason, never as a silent success.
pub async fn mcp402_authorize_handler(
    State(state): State<AppState>,
    Json(req): Json<Mcp402AuthorizeRequest>,
) -> impl IntoResponse {
    let result = match &req.proof {
        Some(proof) => state
            .mcp402
            .authorize_with_proof(&req.call, &req.receipt, proof),
        None => state.mcp402.authorize(&req.call, &req.receipt),
    };
    (StatusCode::OK, Json(result)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_402::{Mcp402Demand, Mcp402Facade, McpJobContext};

    fn call(bounty_sats: u64) -> McpToolCall {
        McpToolCall {
            tool: "market.escrow.release".to_string(),
            arguments: serde_json::json!({ "jobId": "job-1" }),
            job: McpJobContext {
                id: "job-1".to_string(),
                title: "Release escrow".to_string(),
                description: "Release escrow to the agent".to_string(),
                bounty_sats,
                deadline: None,
            },
            payer_did: "did:conxian:payer".to_string(),
        }
    }

    fn receipt() -> Mcp402Receipt {
        Mcp402Receipt {
            demand_id: "job-1".to_string(),
            transaction_id: "tx-1".to_string(),
            amount_sats: 500,
            paid_at: 1,
            payer_did: "did:conxian:payer".to_string(),
        }
    }

    #[test]
    fn demand_handler_maps_a_tool_call_to_an_x402_demand() {
        let facade = Mcp402Facade::new();
        let demand = facade.demand(&call(500));
        assert_eq!(demand.amount_sats, 500);
        assert_eq!(demand.resource_id, "job-1");
        assert_eq!(demand.scheme, "x402");
        assert_eq!(demand.currency, "sats");
    }

    #[test]
    fn authorize_gates_on_a_structurally_valid_receipt() {
        let facade = Mcp402Facade::new();
        let result = facade.authorize(&call(500), &receipt());
        assert!(result.authorized);
        assert!(result.authorization_token.is_none());
    }

    #[test]
    fn authorize_fails_closed_on_a_zero_bounty() {
        let facade = Mcp402Facade::new();
        let result = facade.authorize(&call(0), &receipt());
        assert!(!result.authorized);
        assert!(result.reason.unwrap().contains("bounty"));
    }

    #[test]
    fn demand_returns_expected_payment_pointer() {
        let facade = Mcp402Facade::new();
        let demand: Mcp402Demand = facade.demand(&call(250));
        assert_eq!(demand.payment_pointer, "$conxian.com/market/job/job-1");
    }
}
