use std::sync::Arc;
use dip_types::{DipEnvelope, DipError, AdapterKind, AdapterStatus};
use crate::registry::AdapterRegistry;

/// Routes a DipEnvelope to the correct outbound adapter.
///
/// Routing strategy:
///   1. Parse the `to` field for a network prefix (e.g. "nostr:", "a2a:", "mesh:")
///   2. Try built-in adapter module first
///   3. Fall back to registered external adapter endpoint
pub struct AdapterRouter {
    registry: Arc<AdapterRegistry>,
    client:   reqwest::Client,
}

impl AdapterRouter {
    pub fn new(registry: Arc<AdapterRegistry>) -> Self {
        Self {
            registry,
            client: reqwest::Client::new(),
        }
    }

    pub async fn route(&self, envelope: DipEnvelope) -> Result<(), DipError> {
        let kind = self.parse_network_kind(&envelope.to);

        // Try built-in adapter modules
        match kind {
            AdapterKind::Nostr => {
                return crate::adapters::nostr::send(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            AdapterKind::A2A => {
                return crate::adapters::a2a::send(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            AdapterKind::Mcp => {
                return crate::adapters::mcp::send(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            AdapterKind::Meshtastic => {
                return crate::adapters::meshtastic::send(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            AdapterKind::Libp2p => {
                return crate::adapters::libp2p::send(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            AdapterKind::Freenet => {
                return crate::adapters::freenet::send(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            AdapterKind::Custom(ref scheme) if scheme == "zima" => {
                return crate::adapters::zima::send_envelope(&envelope).await
                    .map_err(|e| DipError::Network(e));
            }
            _ => {}
        }

        // Fall back to registered external adapter endpoint
        let adapters = self.registry.find_by_kind(&kind);
        let active = adapters.into_iter()
            .find(|a| a.status == AdapterStatus::Active && a.endpoint.is_some());

        let adapter = active.ok_or_else(|| DipError::NoRoute {
            from: envelope.from.clone(),
            to: envelope.to.clone(),
        })?;

        let endpoint = format!("{}/api/dip/inbound", adapter.endpoint.unwrap());
        self.client
            .post(&endpoint)
            .json(&envelope)
            .send()
            .await
            .map_err(|e| DipError::Network(e.to_string()))?;

        Ok(())
    }

    fn parse_network_kind(&self, address: &str) -> AdapterKind {
        if address.starts_with("nostr:") { return AdapterKind::Nostr; }
        if address.starts_with("a2a:")   { return AdapterKind::A2A; }
        if address.starts_with("mcp:")   { return AdapterKind::Mcp; }
        if address.starts_with("mesh:")  { return AdapterKind::Meshtastic; }
        if address.starts_with("p2p:")   { return AdapterKind::Libp2p; }
        if address.starts_with("free:")  { return AdapterKind::Freenet; }
        if address.starts_with("habitat:") || address.starts_with("ha:") {
            return AdapterKind::HomeAssistant;
        }
        if address.starts_with("zima:") {
            return AdapterKind::Custom("zima".to_string());
        }
        AdapterKind::Http
    }
}
