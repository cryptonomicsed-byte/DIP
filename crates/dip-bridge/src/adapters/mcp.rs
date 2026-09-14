use dip_types::DipEnvelope;
use serde_json::{json, Value};

/// Send a DipEnvelope via MCP (Model Context Protocol).
/// to address format: "mcp:<server_url>/<tool_name>"
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let target = envelope.to.trim_start_matches("mcp:");
    let (server_url, tool_name) = target
        .rsplit_once('/')
        .unwrap_or((target, "dip_receive"));

    // MCP tools/call format
    let mcp_request = json!({
        "jsonrpc": "2.0",
        "method":  "tools/call",
        "id":      1,
        "params": {
            "name":      tool_name,
            "arguments": {
                "envelope_id": envelope.envelope_id,
                "from":        envelope.from,
                "payload":     envelope.payload,
            }
        }
    });

    let url = format!("{}/mcp", server_url.trim_end_matches('/'));
    tracing::debug!("mcp adapter: call {} on {}", tool_name, url);

    let client = reqwest::Client::new();
    client
        .post(&url)
        .json(&mcp_request)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Parse an MCP tool_call params into a DipEnvelope.
pub fn receive(params: &Value) -> Option<DipEnvelope> {
    let args = params.get("arguments")?;
    serde_json::from_value::<DipEnvelope>(args.clone()).ok()
}
