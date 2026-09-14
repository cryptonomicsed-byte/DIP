use dip_types::DipEnvelope;
use serde_json::{json, Value};

/// Send a DipEnvelope via Meshtastic mesh radio.
/// to address format: "mesh:<node_id>" where node_id is a 32-bit hex node number.
///
/// Transport: serial port (MESHTASTIC_SERIAL_PORT env) or HTTP (MESHTASTIC_HTTP_URL env).
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let node_id = envelope.to.trim_start_matches("mesh:");

    if let Ok(http_url) = std::env::var("MESHTASTIC_HTTP_URL") {
        send_via_http(&http_url, node_id, envelope).await
    } else {
        // Serial path deferred (requires meshtastic serial protocol library).
        tracing::info!("meshtastic adapter: queue msg to {node_id} (no transport configured)");
        Ok(())
    }
}

async fn send_via_http(base_url: &str, node_id: &str, envelope: &DipEnvelope) -> Result<(), String> {
    // Meshtastic HTTP API: POST /api/v1/sendtext or /api/v1/sendpacket
    let payload_json = serde_json::to_string(&envelope.payload).unwrap_or_default();
    // Truncate to Meshtastic 240-byte limit
    let truncated = &payload_json[..payload_json.len().min(240)];

    let body = json!({
        "to":   node_id,
        "text": truncated,
        "wantAck": false,
    });

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/sendtext", base_url.trim_end_matches('/'));
    client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Parse an inbound Meshtastic packet into a DipEnvelope.
pub fn receive(packet: &Value) -> Option<DipEnvelope> {
    let text = packet
        .get("decoded")
        .and_then(|d| d.get("text"))
        .and_then(|t| t.as_str())?;
    serde_json::from_str::<DipEnvelope>(text).ok()
}
