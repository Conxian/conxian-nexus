//! [CON-473] PoC: Nostr relay + collector bridge for Nexus telemetry.
//! Publishes signed telemetry events to a Nostr relay.

use nostr_sdk::prelude::*;
use serde_json::json;
use anyhow::Context;

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

        // Using a custom event kind for telemetry (e.g., 26001 as a placeholder)
        let builder = EventBuilder::new(Kind::Custom(26001), content, []);
        let output = self.client.send_event_builder(builder).await?;
        let event_id = output.id();

        tracing::info!("Published telemetry to Nostr. EventId: {:?}, PubKey: {}", event_id, self.pubkey_bech32);
        Ok(*event_id)
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.client.disconnect().await?;
        Ok(())
    }
}
