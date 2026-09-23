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

    /// Sign this envelope's `canonical_hash()` with the provided key.
    ///
    /// After calling `sign`, `self.signature` holds a 128-char lowercase hex
    /// Ed25519 signature.  The verifying key is stored separately by the
    /// caller (e.g. in the agent's identity manifest).
    pub fn sign(&mut self, key: &crate::signing::DipSigningKey) {
        let hash = self.canonical_hash();
        self.signature = key.sign_envelope_data(&hash);
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

/// SHA-256 of `data`, lowercase hex.
///
/// This is a real cryptographic hash. It previously used
/// `std::collections::hash_map::DefaultHasher` (SipHash-1-3, 64-bit,
/// randomly seeded per process in some configurations) with a second
/// `finish()` call padding the output to 128 bits — non-cryptographic,
/// and therefore forgeable by anyone who wanted to make two distinct
/// envelopes share a `canonical_hash`. Since `canonical_hash()` is the
/// message that `signature` is defined over, envelope integrity was
/// only as strong as SipHash.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
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

    /// Known-answer test against FIPS 180-4. If this ever regresses to a
    /// non-cryptographic hasher, envelope integrity silently weakens — that
    /// is exactly the bug this test exists to prevent.
    #[test]
    fn sha256_hex_matches_fips_180_4_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// `canonical_hash()` must be 64 hex chars (32 bytes), not the 32-char
    /// double-`finish()` output the DefaultHasher version produced.
    #[test]
    fn canonical_hash_is_32_bytes_of_hex() {
        let env = DipEnvelope::new(DipMessageKind::Ping, "a", "b", DipMessage::Ping { nonce: "1".into() });
        let h = env.canonical_hash();
        assert_eq!(h.len(), 64, "expected 32-byte hex digest, got {:?}", h);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
