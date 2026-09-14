use dip_types::DipEnvelope;
use serde_json::Value;

/// Send a DipEnvelope via libp2p.
/// to address format: "p2p:<multiaddr>" e.g. p2p:/ip4/1.2.3.4/tcp/4001/p2p/QmXxx
///
/// Phase 1: HTTP gateway relay (libp2p-http-proxy or js-libp2p HTTP API).
/// Phase 2: Direct libp2p via rust-libp2p (requires separate binary).
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let multiaddr = envelope.to.trim_start_matches("p2p:");

    if let Ok(gateway_url) = std::env::var("LIBP2P_GATEWAY_URL") {
        send_via_gateway(&gateway_url, multiaddr, envelope).await
    } else {
        tracing::info!("libp2p adapter: queue msg to {multiaddr} (no gateway configured)");
        Ok(())
    }
}

async fn send_via_gateway(base_url: &str, peer_addr: &str, envelope: &DipEnvelope) -> Result<(), String> {
    let url = format!("{}/api/libp2p/send", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "peer":     peer_addr,
        "protocol": "/dip/1.0.0",
        "payload":  envelope,
    });
    let client = reqwest::Client::new();
    client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Parse an inbound libp2p stream message into a DipEnvelope.
pub fn receive(raw: &Value) -> Option<DipEnvelope> {
    raw.get("payload")
        .and_then(|p| serde_json::from_value::<DipEnvelope>(p.clone()).ok())
        .or_else(|| serde_json::from_value::<DipEnvelope>(raw.clone()).ok())
}
