use chrono::{DateTime, Utc};
use gix_types::{Gix1, GixKind, GixNamespace, RoutingHints};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DIP interoperability message — the grammar that heterogeneous networks speak.
///
/// DIP does NOT replace A2A/MCP/Nostr/Meshtastic.  It wraps them so an agent
/// that speaks Nostr can collaborate with one that speaks A2A without either
/// knowing about the other's transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DipEnvelope {
    pub envelope_id:    String,
    pub kind:           DipMessageKind,
    /// Originating network identifier (e.g. "nostr:npub1...", "a2a:agent-id", "mesh:abc")
    pub from:           String,
    /// Destination — may be a DID, a network address, or a broadcast wildcard "*"
    pub to:             String,
    pub payload:        DipMessage,
    pub trace_id:       Option<String>,
    pub reply_to:       Option<String>,
    pub ttl_secs:       Option<u32>,
    pub created_at:     DateTime<Utc>,
    /// Ed25519 signature over canonical_hash()
    pub signature:      String,

    /// GIX1 canonical_id (hex) stamped at envelope creation.
    /// `GixNamespace::Mycelium` — every DIP message is GIX-addressable.
    #[serde(default)]
    pub gix1_canonical_id: Option<String>,
}

impl DipEnvelope {
    pub fn new(kind: DipMessageKind, from: &str, to: &str, payload: DipMessage) -> Self {
        let envelope_id = Uuid::new_v4().to_string();
        let created_at  = Utc::now();
        let created_at_ms = created_at.timestamp_millis() as u64;
        let env = Gix1::new(
            GixKind::Receipt,
            GixNamespace::Mycelium,
            envelope_id.as_bytes(),
            None,
            created_at_ms,
            RoutingHints::default(),
        );
        Self {
            envelope_id,
            kind,
            from: from.to_string(),
            to: to.to_string(),
            payload,
            trace_id: None,
            reply_to: None,
            ttl_secs: Some(300),
            created_at,
            signature: String::new(),
            gix1_canonical_id: Some(hex::encode(env.canonical_id)),
        }
    }

    pub fn canonical_hash(&self) -> String {
        use std::collections::BTreeMap;
        let map: BTreeMap<&str, &str> = [
            ("envelope_id", self.envelope_id.as_str()),
            ("from", self.from.as_str()),
            ("to", self.to.as_str()),
        ].into();
        let data = serde_json::to_string(&map).unwrap_or_default();
        sha256_hex(data.as_bytes())
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    data.hash(&mut h);
    format!("{:016x}{:016x}", h.finish(), h.finish().wrapping_mul(0xdeadbeef))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DipMessageKind {
    /// Agent-to-agent task delegation
    AgentDelegate,
    /// Tool invocation (MCP-style)
    ToolCall,
    /// Tool result
    ToolResult,
    /// Capability advertisement
    CapabilityAd,
    /// Identity claim (DID assertion)
    IdentityClaim,
    /// Event broadcast (Nostr-style)
    EventBroadcast,
    /// Receipt relay — cross-network ARP propagation
    ReceiptRelay,
    /// Ping/health check
    Ping,
    /// Pong
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DipMessage {
    AgentDelegate {
        task: String,
        params: serde_json::Value,
        capabilities_required: Vec<String>,
        deadline_secs: Option<u32>,
    },
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
        session_id: Option<String>,
    },
    ToolResult {
        call_id: String,
        outcome: String,
        result: serde_json::Value,
    },
    CapabilityAd {
        capabilities: Vec<String>,
        networks: Vec<String>,
        ttl_secs: u32,
    },
    IdentityClaim {
        did: String,
        proof: String,
        networks: Vec<NetworkBinding>,
    },
    EventBroadcast {
        event_kind: u32,
        content: serde_json::Value,
        tags: Vec<Vec<String>>,
    },
    ReceiptRelay {
        receipt: serde_json::Value,
        receipt_kind: String,
        canonical_hash: String,
    },
    Ping { nonce: String },
    Pong { nonce: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkBinding {
    pub network: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DipRoute {
    pub from_network: AdapterKindStr,
    pub to_network:   AdapterKindStr,
    pub via:          Option<String>,
    pub priority:     u8,
}

pub type AdapterKindStr = String;

#[cfg(test)]
mod gix_tests {
    use super::*;

    #[test]
    fn new_envelope_has_gix1_canonical_id() {
        let env = DipEnvelope::new(
            DipMessageKind::Ping,
            "nostr:npub1test",
            "*",
            DipMessage::Ping { nonce: "abc".into() },
        );
        let id = env.gix1_canonical_id.as_ref().expect("gix1_canonical_id must be set");
        assert_eq!(id.len(), 64, "canonical_id should be 32-byte hex");
    }

    #[test]
    fn two_envelopes_have_distinct_gix1_ids() {
        let e1 = DipEnvelope::new(DipMessageKind::Ping, "a", "b", DipMessage::Ping { nonce: "1".into() });
        let e2 = DipEnvelope::new(DipMessageKind::Ping, "a", "b", DipMessage::Ping { nonce: "2".into() });
        assert_ne!(e1.gix1_canonical_id, e2.gix1_canonical_id);
    }
}
