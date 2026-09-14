use dip_types::DipEnvelope;
use serde_json::Value;

/// Send a DipEnvelope via Freenet (decentralized network).
/// to address format: "free:<contract_key>" where contract_key is a Freenet contract hash.
///
/// Freenet uses content-addressed contracts. DipEnvelopes are published as
/// contract state updates under the recipient's contract key.
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let contract_key = envelope.to.trim_start_matches("free:");

    if let Ok(freenet_url) = std::env::var("FREENET_GATEWAY_URL") {
        send_via_gateway(&freenet_url, contract_key, envelope).await
    } else {
        tracing::info!("freenet adapter: queue msg for {contract_key} (no gateway configured)");
        Ok(())
    }
}

async fn send_via_gateway(base_url: &str, contract_key: &str, envelope: &DipEnvelope) -> Result<(), String> {
    // Freenet gateway HTTP API: PUT /v1/contract/<key>/state
    let url = format!("{}/v1/contract/{}/state", base_url.trim_end_matches('/'), contract_key);
    let body = serde_json::json!({
        "state":   envelope,
        "summary": null,
        "related_contracts": {},
    });
    let client = reqwest::Client::new();
    client
        .put(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Parse an inbound Freenet contract state notification into a DipEnvelope.
pub fn receive(state: &Value) -> Option<DipEnvelope> {
    serde_json::from_value::<DipEnvelope>(state.clone()).ok()
}
