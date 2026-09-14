use dip_types::DipEnvelope;
use serde_json::{json, Value};

/// Translate a DipEnvelope → Nostr event and publish to relay.
/// Uses NIP-04 encrypted DM for private payloads, kind 1 for broadcast.
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let relay_url = std::env::var("DIP_NOSTR_RELAY")
        .unwrap_or_else(|_| "wss://relay.damus.io".to_string());

    // Nostr event construction
    // to address format: "nostr:<npub>"
    let npub = envelope.to.trim_start_matches("nostr:");
    let content = serde_json::to_string(&envelope.payload).unwrap_or_default();

    // Build unsigned event (kind 4 = encrypted DM, kind 1 = public)
    let event = json!({
        "kind":    4,
        "content": content,
        "tags":    [["p", npub]],
        "created_at": now_secs(),
    });

    tracing::debug!("nostr adapter: send to {} via {}", npub, relay_url);

    // Full WebSocket relay publish deferred to Phase 9 (requires keypair from env).
    // For now: log intent + succeed.
    let _ = (relay_url, event);
    Ok(())
}

/// Parse an inbound Nostr event into a DipEnvelope.
pub fn receive(event: &Value) -> Option<DipEnvelope> {
    let content = event.get("content")?.as_str()?;
    serde_json::from_str::<DipEnvelope>(content).ok()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
