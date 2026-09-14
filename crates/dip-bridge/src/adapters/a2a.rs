use dip_types::DipEnvelope;
use serde_json::{json, Value};

/// Send a DipEnvelope via Google A2A protocol.
/// to address format: "a2a:<agent_url>"
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let agent_url = envelope.to.trim_start_matches("a2a:").to_string();

    // A2A message format: { "message": { "parts": [{ "text": ... }] } }
    let a2a_msg = json!({
        "message": {
            "role":  "user",
            "parts": [{
                "kind": "data",
                "data": envelope.payload,
            }],
            "metadata": {
                "dip_envelope_id": envelope.envelope_id,
                "from": envelope.from,
            }
        }
    });

    let url = format!("{}/tasks/send", agent_url.trim_end_matches('/'));
    tracing::debug!("a2a adapter: send to {}", url);

    let client = reqwest::Client::new();
    client
        .post(&url)
        .json(&a2a_msg)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Parse an inbound A2A task notification into a DipEnvelope.
pub fn receive(task_body: &Value) -> Option<DipEnvelope> {
    task_body
        .get("message")
        .and_then(|m| m.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|parts| parts.first())
        .and_then(|part| part.get("data"))
        .and_then(|d| serde_json::from_value::<DipEnvelope>(d.clone()).ok())
}
