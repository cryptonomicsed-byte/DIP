//! DIP adapter implementations.
//! Each module translates DipEnvelopes to/from a native protocol.
//! Adapters register themselves with the AdapterRegistry at startup.

pub mod nostr;
pub mod a2a;
pub mod mcp;
pub mod meshtastic;
pub mod libp2p;
pub mod freenet;
pub mod zima;
pub mod world_query;

use dip_types::{AdapterCapability, AdapterKind, AdapterManifest, AdapterStatus};

/// Build a minimal AdapterManifest for self-registration.
pub fn make_manifest(
    adapter_id: &str,
    kind: AdapterKind,
    endpoint: &str,
) -> AdapterManifest {
    AdapterManifest {
        adapter_id:   adapter_id.to_string(),
        kind,
        version:      "0.1.0".to_string(),
        capabilities: vec![AdapterCapability::Send, AdapterCapability::Receive],
        networks:     Vec::new(),
        status:       AdapterStatus::Active,
        endpoint:     Some(endpoint.to_string()),
    }
}
