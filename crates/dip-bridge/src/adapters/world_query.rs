//! WorldQuery adapter — bridges DipEnvelope to the Open Reality spatial query service.
//!
//! The Open Reality MCP service exposes spatial perception endpoints (planes, objects,
//! path planning, measurements, Gaussian splat status). This adapter routes DipEnvelope
//! payloads to the correct endpoint based on the query type encoded in the envelope.
//!
//! Configuration:
//!   `OPEN_REALITY_URL` — base URL for the Open Reality service (default: http://localhost:8790)
//!
//! Routing (via `send`):
//!   ToolCall { tool_name: "planes" }      → GET  /v1/planes
//!   ToolCall { tool_name: "objects" }     → GET  /v1/objects
//!   ToolCall { tool_name: "path_plan" }   → POST /v1/pathplan   { from, to }
//!   ToolCall { tool_name: "measure" }     → POST /v1/measure    { point_a, point_b }
//!   ToolCall { tool_name: "splat_status" }→ GET  /v1/splat/status
//!   anything else                         → POST /v1/query      (full payload)

use dip_types::{DipEnvelope, DipMessage, DipMessageKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

// ─── Typed request / result structs ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldQueryRequest {
    pub query_type: String,
    #[serde(flatten)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneResult {
    pub plane_id: String,
    pub kind: String,   // "floor" | "wall" | "ceiling"
    pub normal: [f32; 3],
    pub center: [f32; 3],
    pub extent: [f32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectResult {
    pub object_id: String,
    pub label: String,
    pub confidence: f32,
    pub bounding_box: Value,
    pub pose: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPlanResult {
    pub success: bool,
    pub waypoints: Vec<[f32; 3]>,
    pub distance_m: f32,
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn base_url() -> Option<String> {
    match std::env::var("OPEN_REALITY_URL") {
        Ok(url) => Some(url.trim_end_matches('/').to_string()),
        Err(_) => {
            tracing::warn!(
                "world_query adapter: OPEN_REALITY_URL not set; defaulting to http://localhost:8790"
            );
            Some("http://localhost:8790".to_string())
        }
    }
}

/// Extract the query_type string from a DipMessage.
/// Prefers `ToolCall.tool_name`; falls back to serialising the whole payload
/// and reading a top-level `query_type` field for forward-compatibility.
fn query_type_of(msg: &DipMessage) -> String {
    match msg {
        DipMessage::ToolCall { tool_name, .. } => tool_name.clone(),
        other => {
            // serialise and look for a query_type key
            if let Ok(v) = serde_json::to_value(other) {
                if let Some(qt) = v.get("query_type").and_then(|s| s.as_str()) {
                    return qt.to_string();
                }
            }
            "unknown".to_string()
        }
    }
}

/// Pull the `arguments` Value out of a DipMessage (ToolCall) or return an
/// empty object so callers never have to handle Option.
fn arguments_of(msg: &DipMessage) -> Value {
    match msg {
        DipMessage::ToolCall { arguments, .. } => arguments.clone(),
        other => serde_json::to_value(other).unwrap_or_else(|_| json!({})),
    }
}

// ─── Public adapter interface ─────────────────────────────────────────────────

/// Route a DipEnvelope to the appropriate Open Reality endpoint.
///
/// Fail-open: if `OPEN_REALITY_URL` is not set, logs a warning and returns Ok(()).
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let base = match base_url() {
        Some(u) => u,
        None => return Ok(()),
    };

    let qt = query_type_of(&envelope.payload);
    let args = arguments_of(&envelope.payload);
    let client = reqwest::Client::new();

    tracing::debug!("world_query adapter: send query_type={} to {}", qt, base);

    match qt.as_str() {
        "planes" => {
            let url = format!("{}/v1/planes", base);
            client
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        "objects" => {
            let url = format!("{}/v1/objects", base);
            client
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        "path_plan" => {
            let body = json!({
                "from": args.get("from").cloned().unwrap_or(Value::Null),
                "to":   args.get("to").cloned().unwrap_or(Value::Null),
            });
            let url = format!("{}/v1/pathplan", base);
            client
                .post(&url)
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        "measure" => {
            let body = json!({
                "point_a": args.get("point_a").cloned().unwrap_or(Value::Null),
                "point_b": args.get("point_b").cloned().unwrap_or(Value::Null),
            });
            let url = format!("{}/v1/measure", base);
            client
                .post(&url)
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        "splat_status" => {
            let url = format!("{}/v1/splat/status", base);
            client
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        _ => {
            // Generic fallback: POST full serialised payload to /v1/query
            let body = serde_json::to_value(&envelope.payload)
                .unwrap_or_else(|_| json!({}));
            let url = format!("{}/v1/query", base);
            client
                .post(&url)
                .json(&body)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }
}

/// Parse an inbound Open Reality event JSON into a DipEnvelope.
///
/// Expected event shape:
/// ```json
/// { "event_type": "...", "scene_id": "...", "data": { ... }, "timestamp": 0 }
/// ```
pub fn receive(event: &Value) -> Option<DipEnvelope> {
    let event_type = event.get("event_type")?.as_str()?.to_string();
    let scene_id = event
        .get("scene_id")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();
    let data = event.get("data").cloned().unwrap_or(Value::Null);

    let from = format!("world_query:{}", scene_id);

    let payload = DipMessage::ToolResult {
        call_id: Uuid::new_v4().to_string(),
        outcome: event_type,
        result: data,
    };

    Some(DipEnvelope::new(
        DipMessageKind::ToolResult,
        &from,
        "agent",
        payload,
    ))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dip_types::DipMessage;
    use serde_json::json;

    // ── receive ───────────────────────────────────────────────────────────────

    #[test]
    fn receive_parses_full_event() {
        let ev = json!({
            "event_type": "plane_detected",
            "scene_id":   "room-42",
            "data":       { "plane_id": "p1", "kind": "floor" },
            "timestamp":  1_700_000_000u64,
        });
        let env = receive(&ev).expect("should parse");
        assert_eq!(env.from, "world_query:room-42");
        assert_eq!(env.to, "agent");
        match &env.payload {
            DipMessage::ToolResult { outcome, result, .. } => {
                assert_eq!(outcome, "plane_detected");
                assert_eq!(result["kind"], "floor");
            }
            other => panic!("unexpected payload variant: {:?}", other),
        }
    }

    #[test]
    fn receive_handles_missing_scene_id() {
        let ev = json!({ "event_type": "heartbeat", "data": {} });
        let env = receive(&ev).expect("should parse without scene_id");
        assert_eq!(env.from, "world_query:unknown");
    }

    #[test]
    fn receive_returns_none_for_missing_event_type() {
        let ev = json!({ "scene_id": "s1", "data": {} });
        assert!(receive(&ev).is_none());
    }

    // ── query_type routing (synchronous helpers) ──────────────────────────────

    fn tool_call(tool_name: &str, args: Value) -> DipMessage {
        DipMessage::ToolCall {
            tool_name:  tool_name.to_string(),
            arguments:  args,
            session_id: None,
        }
    }

    #[test]
    fn query_type_extracted_from_tool_call() {
        let msg = tool_call("planes", json!({}));
        assert_eq!(query_type_of(&msg), "planes");
    }

    #[test]
    fn query_type_objects() {
        let msg = tool_call("objects", json!({}));
        assert_eq!(query_type_of(&msg), "objects");
    }

    #[test]
    fn query_type_path_plan_args() {
        let args = json!({ "from": [0,0,0], "to": [1,2,3] });
        let msg = tool_call("path_plan", args.clone());
        assert_eq!(query_type_of(&msg), "path_plan");
        let got = arguments_of(&msg);
        assert_eq!(got["from"], args["from"]);
        assert_eq!(got["to"],   args["to"]);
    }

    #[test]
    fn query_type_measure_args() {
        let args = json!({ "point_a": [0,0,0], "point_b": [3,4,0] });
        let msg = tool_call("measure", args.clone());
        assert_eq!(query_type_of(&msg), "measure");
        let got = arguments_of(&msg);
        assert_eq!(got["point_a"], args["point_a"]);
    }

    #[test]
    fn query_type_splat_status() {
        let msg = tool_call("splat_status", json!({}));
        assert_eq!(query_type_of(&msg), "splat_status");
    }

    #[test]
    fn query_type_unknown_falls_through() {
        let msg = tool_call("lidar_scan", json!({}));
        assert_eq!(query_type_of(&msg), "lidar_scan");
    }

    // ── missing env var (fail-open) ───────────────────────────────────────────

    #[test]
    fn base_url_returns_default_when_env_not_set() {
        // ensure the var is absent for this test
        std::env::remove_var("OPEN_REALITY_URL");
        let url = base_url();
        assert_eq!(url.as_deref(), Some("http://localhost:8790"));
    }

    #[test]
    fn base_url_uses_env_var_when_set() {
        std::env::set_var("OPEN_REALITY_URL", "http://10.0.0.5:9000/");
        let url = base_url();
        // trailing slash should be stripped
        assert_eq!(url.as_deref(), Some("http://10.0.0.5:9000"));
        std::env::remove_var("OPEN_REALITY_URL");
    }

    // ── typed struct round-trips ──────────────────────────────────────────────

    #[test]
    fn plane_result_roundtrip() {
        let p = PlaneResult {
            plane_id: "p1".into(),
            kind:     "floor".into(),
            normal:   [0.0, 1.0, 0.0],
            center:   [1.0, 0.0, 2.0],
            extent:   [3.0, 4.0],
        };
        let v = serde_json::to_value(&p).unwrap();
        let p2: PlaneResult = serde_json::from_value(v).unwrap();
        assert_eq!(p.plane_id, p2.plane_id);
        assert_eq!(p.kind, p2.kind);
    }

    #[test]
    fn object_result_roundtrip() {
        let o = ObjectResult {
            object_id:    "obj-7".into(),
            label:        "chair".into(),
            confidence:   0.92,
            bounding_box: json!({"min": [0,0,0], "max": [1,1,1]}),
            pose:         json!({"position": [0.5, 0.0, 0.5]}),
        };
        let v = serde_json::to_value(&o).unwrap();
        let o2: ObjectResult = serde_json::from_value(v).unwrap();
        assert_eq!(o2.label, "chair");
        assert!((o2.confidence - 0.92).abs() < 1e-5);
    }

    #[test]
    fn path_plan_result_roundtrip() {
        let r = PathPlanResult {
            success:    true,
            waypoints:  vec![[0.0, 0.0, 0.0], [1.0, 0.0, 2.0]],
            distance_m: 2.236,
        };
        let v = serde_json::to_value(&r).unwrap();
        let r2: PathPlanResult = serde_json::from_value(v).unwrap();
        assert!(r2.success);
        assert_eq!(r2.waypoints.len(), 2);
    }
}
