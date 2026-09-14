use serde::{Deserialize, Serialize};

/// A DIP adapter connects one heterogeneous network to the DIP message bus.
///
/// Adapters: Nostr · A2A · MCP · Meshtastic · libp2p · Freenet
/// Each adapter translates between its native protocol and DipEnvelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub adapter_id:   String,
    pub kind:         AdapterKind,
    pub version:      String,
    pub capabilities: Vec<AdapterCapability>,
    pub networks:     Vec<String>,
    pub status:       AdapterStatus,
    pub endpoint:     Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    Nostr,
    A2A,
    Mcp,
    Meshtastic,
    Libp2p,
    Freenet,
    Http,
    HomeAssistant,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterCapability {
    Send,
    Receive,
    Relay,
    Broadcast,
    Subscribe,
    Authenticate,
    ReceiptRelay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterStatus {
    Active,
    Degraded,
    Offline,
}
