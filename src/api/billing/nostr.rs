//! [CON-473] PoC: Nostr relay + collector bridge for Nexus telemetry.
//! Publishes and consumes signed telemetry events from a Nostr relay.
//! Updated for nostr-sdk v0.43.0.

use crate::storage::Storage;
use anyhow::{anyhow, Context};
use nostr_sdk::prelude::*;
use serde_json::json;
use std::sync::Arc;

const TELEMETRY_EVENT_KIND: u16 = 26001;
const HEALTH_EVENT_KIND: u16 = 26002;
const MAX_EVENT_AGE_SECONDS: u64 = 3600;

#[derive(Debug, PartialEq)]
enum BridgeAction {
    Bridge,
    IgnoreDuplicate,
    RejectInvalidApiKey,
}

fn telemetry_event_kind() -> Kind {
    Kind::from(TELEMETRY_EVENT_KIND)
}

fn health_event_kind() -> Kind {
    Kind::from(HEALTH_EVENT_KIND)
}

fn is_telemetry_kind(kind: Kind) -> bool {
    kind == telemetry_event_kind()
}

fn is_event_fresh(created_at: Timestamp, now: u64) -> bool {
    created_at.as_u64().saturating_add(MAX_EVENT_AGE_SECONDS) >= now
}

fn parse_api_key_from_content(content: &str) -> anyhow::Result<String> {
    let payload: serde_json::Value = serde_json::from_str(content)?;
    let api_key = payload
        .get("api_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing api_key in Nostr event"))?;
    Ok(api_key.to_string())
}

fn dedup_key_for_event(event_id: &str) -> String {
    format!("nostr_dedup:{}", event_id)
}

fn determine_bridge_action(is_new: bool, api_key_exists: bool) -> BridgeAction {
    if !is_new {
        BridgeAction::IgnoreDuplicate
    } else if !api_key_exists {
        BridgeAction::RejectInvalidApiKey
    } else {
        BridgeAction::Bridge
    }
}

pub struct NostrTelemetry {
    client: Client,
    pubkey_bech32: String,
}

impl NostrTelemetry {
    pub async fn new(secret_key: &str, relays: Vec<String>) -> anyhow::Result<Self> {
        let keys = Keys::parse(secret_key).context("Failed to parse Nostr secret key")?;
        let pubkey_bech32 = keys
            .public_key()
            .to_bech32()
            .context("Failed to encode pubkey")?;

        let client = Client::builder().signer(keys).build();
        for relay in relays {
            client.add_relay(relay).await?;
        }
        client.connect().await;

        Ok(Self {
            client,
            pubkey_bech32,
        })
    }

    pub async fn track_signature_nostr(
        &self,
        api_key: &str,
        signature_hash: &str,
        timestamp: i64,
    ) -> anyhow::Result<EventId> {
        let content = json!({
            "api_key": api_key,
            "signature_hash": signature_hash,
            "timestamp": timestamp,
            "kind": "nexus_telemetry_v1"
        })
        .to_string();

        // Updated for nostr-sdk v0.43.0: EventBuilder::new takes kind and content
        let builder = EventBuilder::new(telemetry_event_kind(), content);
        let event = self.client.send_event_builder(builder).await?;
        let event_id = event.id();

        tracing::info!(
            "Published telemetry to Nostr. EventId: {:?}, PubKey: {}",
            event_id,
            self.pubkey_bech32
        );
        Ok(*event_id)
    }

    /// [NEXUS-04] Sovereign Health Reporting via Nostr.
    /// Periodically reports Nexus health status to decentralized relays.
    pub async fn report_health_nostr(
        &self,
        status: &str,
        processed_height: u64,
        state_root: &str,
    ) -> anyhow::Result<EventId> {
        let content = json!({
            "status": status,
            "processed_height": processed_height,
            "state_root": state_root,
            "timestamp": Timestamp::now().as_u64(),
            "kind": "nexus_health_v1"
        })
        .to_string();

        // Updated for nostr-sdk v0.43.0: EventBuilder::new takes kind and content
        let builder = EventBuilder::new(health_event_kind(), content);
        let event = self.client.send_event_builder(builder).await?;
        let event_id = event.id();

        tracing::info!(
            "Reported health to Nostr. EventId: {:?}, Status: {}",
            event_id,
            status
        );
        Ok(*event_id)
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.client.shutdown().await;
        Ok(())
    }
}

/// [CON-473] Nostr Collector Bridge.
/// Subscribes to telemetry events and writes them to Redis.
pub struct NostrCollector {
    client: Client,
    storage: Arc<Storage>,
}

impl NostrCollector {
    pub async fn new(relays: Vec<String>, storage: Arc<Storage>) -> anyhow::Result<Self> {
        let client = Client::default();
        for relay in relays {
            client.add_relay(relay).await?;
        }
        client.connect().await;

        Ok(Self { client, storage })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let filter = Filter::new().kind(telemetry_event_kind());
        // nostr-sdk v0.43.0 subscribe expects a Filter (not a Vec)
        self.client.subscribe(filter, None).await?;

        tracing::info!("Nostr Collector started, listening for telemetry events (Kind 26001)...");

        let mut notifications = self.client.notifications();
        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Event { event, .. } = notification {
                if is_telemetry_kind(event.kind) {
                    if let Err(e) = self.handle_telemetry_event(*event).await {
                        tracing::error!("Failed to handle Nostr telemetry event: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_telemetry_event(&self, event: Event) -> anyhow::Result<()> {
        let event_id = event.id.to_hex();

        // 1. Verify freshness (e.g., not older than 1 hour)
        let now = Timestamp::now().as_u64();
        if !is_event_fresh(event.created_at, now) {
            tracing::warn!("Nostr Collector: ignoring stale event: {}", event_id);
            return Ok(());
        }

        let api_key = parse_api_key_from_content(&event.content)?;

        tracing::debug!(
            "Nostr Collector: processing event {} from api_key: {}",
            event_id,
            api_key
        );

        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await?;

        // 2. Deduplication check using Redis
        let dedup_key = dedup_key_for_event(&event_id);
        let is_new: bool = redis::cmd("SET")
            .arg(&dedup_key)
            .arg(1)
            .arg("NX")
            .arg("EX")
            .arg(86400) // Keep dedup for 24h
            .query_async::<()>(&mut conn)
            .await
            .is_ok(); // Simplified for v0.27

        if determine_bridge_action(is_new, true) == BridgeAction::IgnoreDuplicate {
            tracing::debug!("Nostr Collector: duplicate event ignored: {}", event_id);
            return Ok(());
        }

        let redis_key = format!("apikey:{}", api_key);

        // 3. Check if API Key exists
        let exists: bool = redis::cmd("EXISTS")
            .arg(&redis_key)
            .query_async::<bool>(&mut conn)
            .await?;
        match determine_bridge_action(true, exists) {
            BridgeAction::Bridge => {}
            BridgeAction::IgnoreDuplicate => unreachable!("is_new is true in this branch"),
            BridgeAction::RejectInvalidApiKey => {
                return Err(anyhow!("Invalid API Key in Nostr telemetry: {}", api_key));
            }
        }

        // 4. Increment usage
        let _: u64 = redis::cmd("HINCRBY")
            .arg(&redis_key)
            .arg("usage")
            .arg(1)
            .query_async::<u64>(&mut conn)
            .await
            .unwrap_or(0);

        tracing::info!(
            "Nostr Collector: Successfully bridged telemetry for {} (Event: {})",
            api_key,
            event_id
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_telemetry_kind_filters_expected_kind() {
        assert!(is_telemetry_kind(telemetry_event_kind()));
        assert!(!is_telemetry_kind(Kind::from(42)));
    }

    #[test]
    fn test_is_event_fresh_within_window() {
        let now = 1_000_000;
        let created_at = Timestamp::from(now - 100);
        assert!(is_event_fresh(created_at, now));
    }

    #[test]
    fn test_is_event_fresh_rejects_stale_event() {
        let now = 1_000_000;
        let created_at = Timestamp::from(now - (MAX_EVENT_AGE_SECONDS + 1));
        assert!(!is_event_fresh(created_at, now));
    }

    #[test]
    fn test_parse_api_key_from_content_accepts_valid_payload() {
        let content = r#"{"api_key":"cxl_valid","signature_hash":"abc"}"#;
        let api_key = parse_api_key_from_content(content).expect("payload should parse");
        assert_eq!(api_key, "cxl_valid");
    }

    #[test]
    fn test_parse_api_key_from_content_rejects_missing_key() {
        let content = r#"{"signature_hash":"abc"}"#;
        let err = parse_api_key_from_content(content).expect_err("missing api_key should error");
        assert!(err.to_string().contains("Missing api_key"));
    }

    #[test]
    fn test_parse_api_key_from_content_rejects_invalid_json() {
        let err = parse_api_key_from_content("{not-json}").expect_err("invalid JSON should error");
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_dedup_key_for_event_is_stable() {
        assert_eq!(
            dedup_key_for_event("abc123"),
            "nostr_dedup:abc123".to_string()
        );
    }

    #[test]
    fn test_determine_bridge_action_variants() {
        assert_eq!(
            determine_bridge_action(false, true),
            BridgeAction::IgnoreDuplicate
        );
        assert_eq!(
            determine_bridge_action(true, false),
            BridgeAction::RejectInvalidApiKey
        );
        assert_eq!(determine_bridge_action(true, true), BridgeAction::Bridge);
    }
}
