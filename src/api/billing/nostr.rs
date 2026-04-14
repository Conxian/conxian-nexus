//! [CON-473] PoC: Nostr relay + collector bridge for Nexus telemetry.
//! Publishes and consumes signed telemetry events from a Nostr relay.

use nostr_sdk::prelude::*;
use serde_json::json;
use anyhow::{anyhow, Context};
use std::sync::Arc;
use crate::storage::Storage;

pub struct NostrTelemetry {
    client: Client,
    pubkey_bech32: String,
}

impl NostrTelemetry {
    pub async fn new(secret_key: &str, relays: Vec<String>) -> anyhow::Result<Self> {
        let keys = Keys::parse(secret_key).context("Failed to parse Nostr secret key")?;
        let pubkey_bech32 = keys.public_key().to_bech32().context("Failed to encode pubkey")?;

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
        }).to_string();

        // Using a custom event kind for telemetry (Kind 26001)
        let builder = EventBuilder::new(Kind::Custom(26001), content, []);
        let output = self.client.send_event_builder(builder).await?;
        let event_id = output.id();

        tracing::info!("Published telemetry to Nostr. EventId: {:?}, PubKey: {}", event_id, self.pubkey_bech32);
        Ok(*event_id)
    }

    /// [NEXUS-04] Sovereign Health Reporting via Nostr.
    /// Periodically reports Nexus health status to decentralized relays.
    pub async fn report_health_nostr(
        &self,
        status: &str,
        processed_height: u64,
        state_root: &str,
        drift: Option<u64>,
    ) -> anyhow::Result<EventId> {
        let mut payload = serde_json::Map::from_iter([
            ("status".to_string(), json!(status)),
            (
                "processed_height".to_string(),
                json!(processed_height),
            ),
            ("state_root".to_string(), json!(state_root)),
            (
                "timestamp".to_string(),
                json!(Timestamp::now().as_u64()),
            ),
            ("kind".to_string(), json!("nexus_health_v1")),
        ]);

        if let Some(drift) = drift {
            payload.insert("drift".to_string(), json!(drift));
        }

        let content = serde_json::Value::Object(payload).to_string();

        // Using Kind 26002 for health reporting
        let builder = EventBuilder::new(Kind::Custom(26002), content, []);
        let output = self.client.send_event_builder(builder).await?;
        let event_id = output.id();

        tracing::info!("Reported health to Nostr. EventId: {:?}, Status: {}", event_id, status);
        Ok(*event_id)
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.client.disconnect().await?;
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
        let filter = Filter::new().kind(Kind::Custom(26001));
        self.client.subscribe(vec![filter], None).await?;

        tracing::info!("Nostr Collector started, listening for telemetry events (Kind 26001)...");

        let mut notifications = self.client.notifications();
        while let Ok(notification) = notifications.recv().await {
            if let RelayPoolNotification::Event { event, .. } = notification {
                if event.kind() == Kind::Custom(26001) {
                    if let Err(e) = self.handle_telemetry_event(event).await {
                        tracing::error!("Failed to handle Nostr telemetry event: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_telemetry_event(&self, event: Box<Event>) -> anyhow::Result<()> {
        let event_id = event.id().to_hex();

        // 1. Verify freshness (e.g., not older than 1 hour)
        let now = Timestamp::now().as_u64();
        if event.created_at().as_u64() < now - 3600 {
            tracing::warn!("Nostr Collector: ignoring stale event: {}", event_id);
            return Ok(());
        }

        let payload: serde_json::Value = serde_json::from_str(event.content())?;

        let api_key = payload.get("api_key").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing api_key in Nostr event"))?;

        tracing::debug!("Nostr Collector: processing event {} from api_key: {}", event_id, api_key);

        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;

        // 2. Deduplication check using Redis
        let dedup_key = format!("nostr_dedup:{}", event_id);
        let is_new: bool = redis::cmd("SET")
            .arg(&dedup_key)
            .arg(1)
            .arg("NX")
            .arg("EX")
            .arg(86400) // Keep dedup for 24h
            .query_async(&mut conn)
            .await?;

        if !is_new {
            tracing::debug!("Nostr Collector: duplicate event ignored: {}", event_id);
            return Ok(());
        }

        let redis_key = format!("apikey:{}", api_key);

        // 3. Check if API Key exists
        let exists: bool = redis::cmd("EXISTS").arg(&redis_key).query_async(&mut conn).await?;
        if !exists {
            return Err(anyhow!("Invalid API Key in Nostr telemetry: {}", api_key));
        }

        // 4. Increment usage
        let _: u64 = redis::cmd("HINCRBY")
            .arg(&redis_key)
            .arg("usage")
            .arg(1)
            .query_async(&mut conn)
            .await
            .unwrap_or(0);

        tracing::info!("Nostr Collector: Successfully bridged telemetry for {} (Event: {})", api_key, event_id);
        Ok(())
    }
}
