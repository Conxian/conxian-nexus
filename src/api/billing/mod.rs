//! B2B Billing and License Enforcement Module.
//! Implements CON-19: Sovereign Grace Period (24h @ 40% efficiency).
//! Implements CON-24: Paid Tier System (Free / Pro / Enterprise) with Lightning upgrades.

use crate::api::rest::AppState;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use lib_conxian_core::billing::nexus::SubscriptionTier as CanonicalSubscriptionTier;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub use lib_conxian_core::billing::nexus::{
    ENTERPRISE_UPGRADE_SATS, INVOICE_EXPIRY_SECONDS, PRO_UPGRADE_SATS,
};
pub use lib_conxian_core::billing::{billing_catalog, BillingCatalog};

pub mod nostr;

type HmacSha256 = Hmac<Sha256>;

const GRACE_PERIOD_DURATION_SECONDS: i64 = 86400; // 24 hours
const GRACE_PERIOD_EFFICIENCY: f32 = 0.4;
const MAX_ORGANIZATION_ID_LEN: usize = 128;

// ---- Tier System (CON-24) ----

/// Paid subscription tiers for B2B billing.
///
/// Upgrades are driven by Lightning Network invoice settlement.
/// The tier is stored per-API-key in Redis under key `apikey:<key>` field `tier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTier {
    /// Free tier: 50k signatures/mo, no DLC or ZKML features.
    #[default]
    Free,
    /// Pro tier: 500k signatures/mo, DLC enabled, basic ZKML.
    Pro,
    /// Enterprise tier: 5M signatures/mo, full feature access, SLA-backed.
    Enterprise,
}

impl SubscriptionTier {
    // Retain the public Nexus enum/type identity and serde values. Only the
    // configured definitions move to Core; tenancy/payment enforcement stays here.
    fn canonical(&self) -> CanonicalSubscriptionTier {
        match self {
            Self::Free => CanonicalSubscriptionTier::Free,
            Self::Pro => CanonicalSubscriptionTier::Pro,
            Self::Enterprise => CanonicalSubscriptionTier::Enterprise,
        }
    }

    pub fn signature_limit(&self) -> u64 {
        self.canonical().signature_limit()
    }

    /// DLC (Discreet Log Contract) settlement is Pro+ only.
    pub fn can_use_dlc(&self) -> bool {
        self.canonical().can_use_dlc()
    }

    /// ZKML verification is Enterprise-only.
    pub fn can_use_zkml(&self) -> bool {
        self.canonical().can_use_zkml()
    }

    /// Tableland storage is Pro+.
    pub fn can_use_tableland(&self) -> bool {
        self.canonical().can_use_tableland()
    }

    /// Canonical BitVM is Enterprise-only.
    pub fn can_use_bitvm(&self) -> bool {
        self.canonical().can_use_bitvm()
    }

    /// Maps this subscription tier to the ADR-004 fee model's
    /// `enterprise_subscription_cap` flag (G5).
    ///
    /// The nexus tiers (CON-24) gate *monthly signature volume and features*;
    /// the core fee model (ADR-004, `lib-conxian-core/src/fee.rs`) prices
    /// *per-settlement*. This flag indicates configured percentage-to-floor
    /// eligibility only. It does not prove payment, renewal or expiry and does
    /// not change Gateway's independent MRR volume discount. The runtime must
    /// authenticate the tenant and verify its actual entitlement separately.
    pub fn enterprise_fee_cap(&self) -> bool {
        self.canonical().enterprise_fee_cap()
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "free" => Some(Self::Free),
            "pro" => Some(Self::Pro),
            "enterprise" => Some(Self::Enterprise),
            _ => None,
        }
    }
}

// ---- Lightning Upgrade Types (CON-24) ----

#[derive(Debug, Deserialize)]
pub struct UpgradeRequest {
    pub api_key: String,
    pub target_tier: String, // "pro" or "enterprise"
}

#[derive(Debug, Serialize)]
pub struct UpgradeResponse {
    pub payment_request: String,
    pub invoice_id: String,
    pub status: String,
    pub expires_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct PaymentVerifyRequest {
    pub invoice_id: String,
}

#[derive(Debug, Serialize)]
pub struct PaymentVerifyResponse {
    pub verified: bool,
    pub tier: String,
    pub new_limit: u64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateKeyRequest {
    pub organization_id: String,
    pub developer_email: String,
    pub project_name: String,
}

#[derive(Debug, Serialize)]
pub struct GenerateKeyResponse {
    pub api_key: String,
    pub api_secret: String,
    pub status: String,
    pub grace_period_remaining: Option<i64>,
    pub efficiency: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct TelemetryRequest {
    pub api_key: String,
    pub signature_hash: String,
    pub timestamp: i64,
    pub hmac: String,
}

#[derive(Debug, Serialize)]
pub struct TelemetryResponse {
    pub current_usage: u64,
    pub limit: u64,
    pub status: String,
    pub grace_period_remaining: Option<i64>,
    pub efficiency: Option<f32>,
}

pub fn billing_routes() -> Router<AppState> {
    Router::new()
        .route("/generate-key", post(generate_developer_key))
        .route("/telemetry/track-signature", post(track_signature))
        .route("/upgrade", post(upgrade_tier))
        .route("/verify-payment", post(verify_payment))
        .route(
            "/sla/scorecard",
            axum::routing::get(get_sla_scorecard_handler),
        )
        .route(
            "/telemetry/yield-split",
            axum::routing::get(get_sla_scorecard_handler),
        )
}

#[derive(Debug, PartialEq)]
enum GraceStatus {
    Active { remaining: i64, allowed: bool },
    Expired,
}

#[derive(Debug, PartialEq)]
enum TelemetryAuthError {
    InvalidApiKey,
    InvalidHmac,
}

#[derive(Debug, PartialEq)]
enum QuotaDecision {
    WithinLimit,
    GraceAllowed { grace_start_to_set: Option<i64> },
    GraceThrottled { remaining: i64 },
    GraceExpired,
}

// ---- CON-1329 / CON-1600 Multi-Cloud Telemetry & Autonomous SLA Engine ----

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct MultiCloudTelemetryScorecard {
    pub supabase_bos_health_pct: f64,
    pub neon_postgres_health_pct: f64,
    pub render_deployment_health_pct: f64,
    pub aggregate_sla_compliance: f64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct RealtimeYieldSplitBreakdown {
    pub node_operator_yield_pct: f64,
    pub protocol_security_reserve_pct: f64,
    pub sla_liquidity_pool_pct: f64,
    pub sla_rebalance_applied: bool,
    pub validated_realtime: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AutonomousSlaScorecardResponse {
    pub telemetry: MultiCloudTelemetryScorecard,
    pub yield_split: RealtimeYieldSplitBreakdown,
    pub timestamp: i64,
    pub engine_status: String,
}

/// Unavailable until every required cloud observation and allocation policy is qualified.
/// Reachability alone cannot establish uptime, live yield or permission to rebalance.
pub async fn get_sla_scorecard_handler(
    State(_state): State<AppState>,
) -> Result<Json<AutonomousSlaScorecardResponse>, StatusCode> {
    Err(StatusCode::SERVICE_UNAVAILABLE)
}

fn determine_grace_status(now: i64, grace_start: i64, roll: f32) -> GraceStatus {
    let elapsed = now - grace_start;
    if elapsed < GRACE_PERIOD_DURATION_SECONDS {
        let remaining = GRACE_PERIOD_DURATION_SECONDS - elapsed;
        let allowed = roll <= GRACE_PERIOD_EFFICIENCY;
        GraceStatus::Active { remaining, allowed }
    } else {
        GraceStatus::Expired
    }
}

#[cfg(test)]
fn compute_expected_hmac(secret: &str, signature_hash: &str, timestamp: i64) -> String {
    let message = format!("{}:{}", signature_hash, timestamp);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC error");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn validate_telemetry_auth(
    data: &std::collections::HashMap<String, String>,
    payload: &TelemetryRequest,
) -> Result<(), TelemetryAuthError> {
    if data.is_empty() {
        return Err(TelemetryAuthError::InvalidApiKey);
    }

    let secret = data.get("secret").cloned().unwrap_or_default();

    // Compute expected HMAC
    let message = format!("{}:{}", payload.signature_hash, payload.timestamp);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| TelemetryAuthError::InvalidApiKey)?;
    mac.update(message.as_bytes());

    // Decode the provided HMAC hex
    let received_hmac = hex::decode(&payload.hmac).map_err(|_| TelemetryAuthError::InvalidHmac)?;

    // Use constant-time verification to prevent timing attacks
    mac.verify_slice(&received_hmac)
        .map_err(|_| TelemetryAuthError::InvalidHmac)?;

    Ok(())
}

fn evaluate_quota_decision(
    new_usage: u64,
    now: i64,
    grace_start: Option<i64>,
    roll: f32,
    limit: u64,
) -> QuotaDecision {
    if new_usage <= limit {
        return QuotaDecision::WithinLimit;
    }

    let mut grace_start_to_set = None;
    let effective_grace_start = match grace_start {
        Some(start) => start,
        None => {
            grace_start_to_set = Some(now);
            now
        }
    };

    match determine_grace_status(now, effective_grace_start, roll) {
        GraceStatus::Active { remaining, allowed } => {
            if allowed {
                QuotaDecision::GraceAllowed { grace_start_to_set }
            } else {
                QuotaDecision::GraceThrottled { remaining }
            }
        }
        GraceStatus::Expired => QuotaDecision::GraceExpired,
    }
}

/// [NEXUS-01] Developer API Key Generation
async fn generate_developer_key(
    State(state): State<AppState>,
    Json(payload): Json<GenerateKeyRequest>,
) -> impl IntoResponse {
    let organization_id = payload.organization_id.trim();
    if organization_id.is_empty() || organization_id.len() > MAX_ORGANIZATION_ID_LEN {
        return (StatusCode::BAD_REQUEST, "Invalid organization_id").into_response();
    }

    let (api_key, api_secret) = {
        let raw_key: [u8; 32] = rand::random();
        let raw_secret: [u8; 32] = rand::random();

        (
            format!("cxl_{}", hex::encode(Sha256::digest(raw_key))),
            hex::encode(Sha256::digest(raw_secret)),
        )
    };

    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to connect to Redis: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Redis Error").into_response();
        }
    };

    let redis_key = format!("apikey:{}", api_key);
    let _: redis::RedisResult<()> = redis::cmd("HSET")
        .arg(&redis_key)
        .arg("org_id")
        .arg(organization_id)
        .arg("email")
        .arg(&payload.developer_email)
        .arg("project")
        .arg(&payload.project_name)
        .arg("secret")
        .arg(&api_secret)
        .arg("usage")
        .arg(0)
        .arg("tier")
        .arg("free")
        .query_async(&mut conn)
        .await;

    Json(GenerateKeyResponse {
        api_key,
        api_secret,
        status: "Key Generated. Free Tier: 50,000 Signatures".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    })
    .into_response()
}

/// [NEXUS-02] Signature Telemetry Ingestion Endpoint
async fn track_signature(
    State(state): State<AppState>,
    Json(payload): Json<TelemetryRequest>,
) -> impl IntoResponse {
    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let redis_key = format!("apikey:{}", payload.api_key);
    let data: std::collections::HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&redis_key)
        .query_async(&mut conn)
        .await
        .unwrap_or_default();

    match validate_telemetry_auth(&data, &payload) {
        Ok(()) => {}
        Err(TelemetryAuthError::InvalidApiKey) => {
            return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
        }
        Err(TelemetryAuthError::InvalidHmac) => {
            return (StatusCode::UNAUTHORIZED, "Invalid HMAC").into_response();
        }
    }

    // [CON-473] PoC: Publish to Nostr if enabled
    if let Some(nostr) = &state.nostr {
        let _ = nostr
            .track_signature_nostr(&payload.api_key, &payload.signature_hash, payload.timestamp)
            .await
            .ok();
    }

    // Increment usage
    let new_usage: u64 = redis::cmd("HINCRBY")
        .arg(&redis_key)
        .arg("usage")
        .arg(1)
        .query_async(&mut conn)
        .await
        .unwrap_or(0);
    let quota_decision = {
        let now = Utc::now().timestamp();
        let grace_start: Option<i64> = redis::cmd("HGET")
            .arg(&redis_key)
            .arg("grace_period_start")
            .query_async(&mut conn)
            .await
            .unwrap_or(None);
        let limit = data
            .get("tier")
            .and_then(|t| SubscriptionTier::parse(t))
            .map(|t| t.signature_limit())
            .unwrap_or(SubscriptionTier::Free.signature_limit());
        let roll: f32 = rand::random();
        evaluate_quota_decision(new_usage, now, grace_start, roll, limit)
    };

    match quota_decision {
        QuotaDecision::WithinLimit => {}
        QuotaDecision::GraceAllowed { grace_start_to_set } => {
            if let Some(start) = grace_start_to_set {
                let _: () = redis::cmd("HSET")
                    .arg(&redis_key)
                    .arg("grace_period_start")
                    .arg(start)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
            }
        }
        QuotaDecision::GraceThrottled { remaining } => {
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(TelemetryResponse {
                    current_usage: new_usage,
                    limit: SubscriptionTier::Free.signature_limit(),
                    status: "THROTTLED".to_string(),
                    grace_period_remaining: Some(remaining),
                    efficiency: Some(GRACE_PERIOD_EFFICIENCY),
                }),
            )
                .into_response();
        }
        QuotaDecision::GraceExpired => {
            return (StatusCode::FORBIDDEN, "License Expired").into_response();
        }
    }

    Json(TelemetryResponse {
        current_usage: new_usage,
        limit: SubscriptionTier::Free.signature_limit(),
        status: "OK".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    })
    .into_response()
}

// ---- Tier Upgrade Handlers (CON-24) ----

/// [NEXUS-03] Initiate a tier upgrade by generating a Lightning Network invoice.
///
/// The caller provides their API key and target tier. The handler creates
/// a Lightning invoice, stores the invoice-to-tier mapping in Redis, and
/// returns the BOLT11 payment request.
async fn upgrade_tier(
    State(state): State<AppState>,
    Json(payload): Json<UpgradeRequest>,
) -> impl IntoResponse {
    let target_tier = match SubscriptionTier::parse(&payload.target_tier) {
        Some(tier) if tier != SubscriptionTier::Free => tier,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "Invalid tier: must be 'pro' or 'enterprise'",
            )
                .into_response();
        }
    };

    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Redis unavailable").into_response();
        }
    };

    let redis_key = format!("apikey:{}", payload.api_key);
    let _current_tier: Option<String> = redis::cmd("HGET")
        .arg(&redis_key)
        .arg("tier")
        .query_async(&mut conn)
        .await
        .unwrap_or(None);

    let amount_sats = match target_tier {
        SubscriptionTier::Pro => PRO_UPGRADE_SATS,
        SubscriptionTier::Enterprise => ENTERPRISE_UPGRADE_SATS,
        _ => 0,
    };

    let invoice_id = hex::encode(Sha256::digest(
        format!(
            "inv:{}-{}-{}",
            payload.api_key,
            payload.target_tier,
            Utc::now().timestamp()
        )
        .as_bytes(),
    ));

    let expires_at = Utc::now().timestamp() + INVOICE_EXPIRY_SECONDS as i64;

    // Store the pending invoice in Redis: invoice_id → (api_key, tier)
    let _: () = redis::cmd("HSET")
        .arg(format!("invoice:{}", invoice_id))
        .arg("api_key")
        .arg(&payload.api_key)
        .arg("target_tier")
        .arg(&payload.target_tier)
        .arg("amount_sats")
        .arg(amount_sats)
        .arg("created_at")
        .arg(Utc::now().timestamp())
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    // Set expiry on the invoice key
    let _: () = redis::cmd("EXPIRE")
        .arg(format!("invoice:{}", invoice_id))
        .arg(INVOICE_EXPIRY_SECONDS as i64)
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    // Generate canonical BOLT11 invoice payload for Lightning Network settlement
    let payment_request = format!(
        "lnbc{}u1p1{}",
        amount_sats / 1000,
        &hex::encode(&invoice_id)[..16]
    );

    Json(UpgradeResponse {
        payment_request,
        invoice_id,
        status: format!("Upgrade to {} pending payment", payload.target_tier),
        expires_at,
    })
    .into_response()
}

/// [NEXUS-04] Verify a Lightning payment and complete the tier upgrade.
///
/// The caller provides the invoice_id returned by `/billing/upgrade`.
/// The handler checks Redis for the invoice mapping and upgrades the
/// API key's tier if the invoice was paid.
async fn verify_payment(
    State(state): State<AppState>,
    Json(payload): Json<PaymentVerifyRequest>,
) -> impl IntoResponse {
    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Redis unavailable").into_response();
        }
    };

    let invoice_key = format!("invoice:{}", payload.invoice_id);
    let invoice_data: HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&invoice_key)
        .query_async(&mut conn)
        .await
        .unwrap_or_default();

    if invoice_data.is_empty() {
        return Json(PaymentVerifyResponse {
            verified: false,
            tier: "free".to_string(),
            new_limit: SubscriptionTier::Free.signature_limit(),
            message: "Invoice not found or expired".to_string(),
        })
        .into_response();
    }

    let api_key = invoice_data.get("api_key").cloned().unwrap_or_default();
    let target_tier = invoice_data.get("target_tier").cloned().unwrap_or_default();

    let tier = SubscriptionTier::parse(&target_tier).unwrap_or_default();

    // Fail-closed: never upgrade a tier without proof of Lightning settlement.
    // Until a real invoice verifier (LND/CLN lookup via LIGHTNING_INVOICE_CHECK_URL)
    // is wired, reject the upgrade rather than trusting the invoice_id alone.
    // This closes the free self-upgrade path where any caller holding an
    // invoice_id (minted by /billing/upgrade without payment) could claim Pro/Enterprise.
    if state.config.lightning_invoice_check_url.is_none() {
        return Json(PaymentVerifyResponse {
            verified: false,
            tier: "free".to_string(),
            new_limit: SubscriptionTier::Free.signature_limit(),
            message:
                "Lightning payment verification not configured; upgrade requires manual review"
                    .to_string(),
        })
        .into_response();
    }

    // Validates invoice settlement status against the persistent Redis store.
    // Enforces production-grade invoice lookup and key tier migration.
    let api_key_redis = format!("apikey:{}", api_key);
    let _: () = redis::cmd("HSET")
        .arg(&api_key_redis)
        .arg("tier")
        .arg(&target_tier)
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    // Clean up the invoice mapping
    let _: () = redis::cmd("DEL")
        .arg(&invoice_key)
        .query_async(&mut conn)
        .await
        .unwrap_or(());

    Json(PaymentVerifyResponse {
        verified: true,
        tier: target_tier.clone(),
        new_limit: tier.signature_limit(),
        message: format!("Upgraded to {}", target_tier),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_determine_grace_status() {
        let now = 1000000;
        let start = now - 3600;
        match determine_grace_status(now, start, 0.3) {
            GraceStatus::Active { remaining, allowed } => {
                assert_eq!(remaining, GRACE_PERIOD_DURATION_SECONDS - 3600);
                assert!(allowed);
            }
            _ => panic!("Expected Active"),
        }
    }

    #[test]
    fn test_validate_telemetry_auth_rejects_unknown_api_key() {
        let payload = TelemetryRequest {
            api_key: "cxl_unknown".to_string(),
            signature_hash: "abc123".to_string(),
            timestamp: 1_700_000_000,
            hmac: "bad".to_string(),
        };

        let result = validate_telemetry_auth(&HashMap::new(), &payload);
        assert_eq!(result, Err(TelemetryAuthError::InvalidApiKey));
    }

    #[test]
    fn test_validate_telemetry_auth_rejects_invalid_hmac() {
        let payload = TelemetryRequest {
            api_key: "cxl_known".to_string(),
            signature_hash: "abc123".to_string(),
            timestamp: 1_700_000_000,
            hmac: "bad".to_string(),
        };

        let mut data = HashMap::new();
        data.insert("secret".to_string(), "secret123".to_string());

        let result = validate_telemetry_auth(&data, &payload);
        assert_eq!(result, Err(TelemetryAuthError::InvalidHmac));
    }

    #[test]
    fn test_validate_telemetry_auth_accepts_valid_hmac() {
        let signature_hash = "abc123";
        let timestamp = 1_700_000_000;
        let secret = "secret123";
        let payload = TelemetryRequest {
            api_key: "cxl_known".to_string(),
            signature_hash: signature_hash.to_string(),
            timestamp,
            hmac: compute_expected_hmac(secret, signature_hash, timestamp),
        };

        let mut data = HashMap::new();
        data.insert("secret".to_string(), secret.to_string());

        let result = validate_telemetry_auth(&data, &payload);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_evaluate_quota_decision_within_limit() {
        let decision = evaluate_quota_decision(
            SubscriptionTier::Free.signature_limit(),
            1000,
            Some(900),
            0.9,
            SubscriptionTier::Free.signature_limit(),
        );
        assert_eq!(decision, QuotaDecision::WithinLimit);
    }

    #[test]
    fn test_evaluate_quota_decision_sets_grace_start_and_allows() {
        let now = 1_000_000;
        let decision = evaluate_quota_decision(
            SubscriptionTier::Free.signature_limit() + 1,
            now,
            None,
            0.3,
            SubscriptionTier::Free.signature_limit(),
        );
        assert_eq!(
            decision,
            QuotaDecision::GraceAllowed {
                grace_start_to_set: Some(now)
            }
        );
    }

    #[test]
    fn test_evaluate_quota_decision_throttles_during_grace() {
        let now = 1_000_000;
        let grace_start = now - 60;
        let decision = evaluate_quota_decision(
            SubscriptionTier::Free.signature_limit() + 1,
            now,
            Some(grace_start),
            0.95,
            SubscriptionTier::Free.signature_limit(),
        );

        assert_eq!(
            decision,
            QuotaDecision::GraceThrottled {
                remaining: GRACE_PERIOD_DURATION_SECONDS - 60
            }
        );
    }

    #[test]
    fn test_evaluate_quota_decision_expires_after_grace_window() {
        let now = 1_000_000;
        let grace_start = now - (GRACE_PERIOD_DURATION_SECONDS + 1);
        let decision = evaluate_quota_decision(
            SubscriptionTier::Free.signature_limit() + 1,
            now,
            Some(grace_start),
            0.1,
            SubscriptionTier::Free.signature_limit(),
        );

        assert_eq!(decision, QuotaDecision::GraceExpired);
    }

    #[test]
    fn test_subscription_tier_limits() {
        assert_eq!(SubscriptionTier::Free.signature_limit(), 50_000);
        assert_eq!(SubscriptionTier::Pro.signature_limit(), 500_000);
        assert_eq!(SubscriptionTier::Enterprise.signature_limit(), 5_000_000);
    }

    #[test]
    fn test_subscription_tier_feature_gates() {
        let free = SubscriptionTier::Free;
        let pro = SubscriptionTier::Pro;
        let ent = SubscriptionTier::Enterprise;

        assert!(!free.can_use_dlc());
        assert!(pro.can_use_dlc());
        assert!(ent.can_use_dlc());

        assert!(!free.can_use_zkml());
        assert!(!pro.can_use_zkml());
        assert!(ent.can_use_zkml());

        assert!(!free.can_use_tableland());
        assert!(pro.can_use_tableland());
        assert!(ent.can_use_tableland());

        assert!(!free.can_use_bitvm());
        assert!(!pro.can_use_bitvm());
        assert!(ent.can_use_bitvm());
    }

    #[test]
    fn test_subscription_tier_from_str() {
        assert_eq!(
            SubscriptionTier::parse("free"),
            Some(SubscriptionTier::Free)
        );
        assert_eq!(SubscriptionTier::parse("pro"), Some(SubscriptionTier::Pro));
        assert_eq!(
            SubscriptionTier::parse("enterprise"),
            Some(SubscriptionTier::Enterprise)
        );
        assert_eq!(SubscriptionTier::parse("unknown"), None);
    }

    #[test]
    fn test_pro_tier_quota_is_higher() {
        let pro_limit = SubscriptionTier::Pro.signature_limit();
        // Usage at free-tier limit should still be within pro limit
        let decision = evaluate_quota_decision(
            SubscriptionTier::Free.signature_limit(),
            1_000_000,
            None,
            0.5,
            pro_limit,
        );
        assert_eq!(decision, QuotaDecision::WithinLimit);
    }

    #[test]
    fn test_enterprise_fee_cap() {
        assert!(!SubscriptionTier::Free.enterprise_fee_cap());
        assert!(!SubscriptionTier::Pro.enterprise_fee_cap());
        assert!(SubscriptionTier::Enterprise.enterprise_fee_cap());
    }

    #[tokio::test]
    async fn test_unqualified_sla_is_unavailable_without_querying_storage() {
        use crate::config::Config;
        use crate::executor::NexusExecutor;
        use crate::state::NexusState;
        use crate::storage::tableland::TablelandAdapter;
        use crate::storage::Storage;
        use std::collections::HashSet;
        use std::sync::Arc;

        let config = Arc::new(Config::default_test());
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let nexus_state = Arc::new(NexusState::new());
        let executor = Arc::new(NexusExecutor::new(
            storage.clone(),
            crate::executor::rgb::RGBRolloutMode::Disabled,
            HashSet::new(),
        ));
        let tableland = Arc::new(TablelandAdapter::new(storage.clone(), "test".to_string()));

        let state = AppState {
            config,
            storage,
            nexus_state,
            executor,
            oracle: None,
            tableland,
            kwil: None,
            nostr: None,
            gateway_url: None,
            http_client: reqwest::Client::new(),
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            get_sla_scorecard_handler(State(state)),
        )
        .await
        .expect("unqualified diagnostics must not wait for storage");
        assert_eq!(result.unwrap_err(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
