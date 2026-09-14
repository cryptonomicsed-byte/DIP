use serde::{Deserialize, Serialize};

/// Canonical agent identity with per-network representations.
///
/// Born once (in Omo-Koda2). Every network gets a projection of the same
/// identity, not a separate agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DipIdentity {
    /// Canonical DID — the root of all network representations
    pub did:            String,
    pub kind:           IdentityKind,
    pub public_key_hex: String,
    pub networks:       Vec<NetworkRepr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    Human,
    Agent,
    Daemon,
    Contract,
    Federated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRepr {
    pub network: String,
    pub address: String,
    pub public_key: Option<String>,
    pub metadata: serde_json::Value,
}

impl NetworkRepr {
    pub fn nostr(npub: &str) -> Self {
        Self {
            network: "nostr".to_string(),
            address: npub.to_string(),
            public_key: Some(npub.to_string()),
            metadata: serde_json::Value::Null,
        }
    }

    pub fn a2a(agent_id: &str) -> Self {
        Self {
            network: "a2a".to_string(),
            address: agent_id.to_string(),
            public_key: None,
            metadata: serde_json::Value::Null,
        }
    }

    pub fn meshtastic(node_id: u32) -> Self {
        Self {
            network: "meshtastic".to_string(),
            address: format!("!{:08x}", node_id),
            public_key: None,
            metadata: serde_json::Value::Null,
        }
    }
}
