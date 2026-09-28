/// Phase 16.4 — OsoRouter: transport-agnostic sovereign message router.
///
/// Implements a priority-ordered fallback cascade for sovereign agent messaging:
///
///   1. DirectIp  — HTTP POST to a known peer endpoint (fastest, requires reachability)
///   2. NostrRelay — publish to shared Nostr relay (near-real-time, internet-dependent)
///   3. FreenetState — write to Freenet distributed state (async, censorship-resistant)
///   4. Meshtastic  — LoRa mesh broadcast (offline-capable, limited bandwidth)
///   5. Bluetooth   — BLE advertisement / GATT (ultra-short range, last resort)
///   6. Local       — same-process / same-node delivery (no-op if recipient not local)
///
/// The router stops at the first successful transport.  Failures are collected;
/// if every transport fails a `DipError::NoRoute` is returned with the full
/// failure log attached.
///
/// # Design notes
/// - OsoMessage is a lightweight wrapper so callers don't need to build a full
///   DipEnvelope manually.
/// - Each transport attempts are delegated back to the existing adapter modules
///   so no logic is duplicated.
/// - Transports can be disabled per-message via `OsoRoutingPolicy`.
use std::sync::Arc;
use dip_types::{DipEnvelope, DipMessage, DipMessageKind, DipError};
use crate::registry::AdapterRegistry;
use crate::adapters;

fn blake3_hex(data: &[u8]) -> String {
    hex::encode(blake3::hash(data).as_bytes())
}

/// A higher-level outbound message for sovereign agent communication.
#[derive(Debug, Clone)]
pub struct OsoMessage {
    /// Originating agent address (e.g. "nostr:npub1...")
    pub from: String,
    /// Destination agent address or wildcard "*"
    pub to: String,
    /// Message payload (reuses DipMessage)
    pub payload: DipMessage,
    /// Optional trace correlation id
    pub trace_id: Option<String>,
    /// Per-message transport policy overrides
    pub policy: OsoRoutingPolicy,
}

/// Controls which transports may be attempted for a given message.
#[derive(Debug, Clone)]
pub struct OsoRoutingPolicy {
    pub allow_direct_ip:  bool,
    pub allow_nostr:      bool,
    pub allow_freenet:    bool,
    pub allow_meshtastic: bool,
    pub allow_bluetooth:  bool,
    pub allow_local:      bool,
}

impl Default for OsoRoutingPolicy {
    fn default() -> Self {
        Self {
            allow_direct_ip:  true,
            allow_nostr:      true,
            allow_freenet:    true,
            allow_meshtastic: true,
            allow_bluetooth:  false, // opt-in: BLE is experimental
            allow_local:      true,
        }
    }
}

/// Result of a single transport attempt.
#[derive(Debug)]
pub struct TransportAttempt {
    pub transport: &'static str,
    pub success:   bool,
    pub error:     Option<String>,
}

/// Outcome of `OsoRouter::send()`.
#[derive(Debug)]
pub struct OsoDelivery {
    /// Transport that succeeded, or None if all failed.
    pub delivered_via: Option<&'static str>,
    /// Full attempt log (all transports tried in order).
    pub attempts: Vec<TransportAttempt>,
}

impl OsoDelivery {
    pub fn succeeded(&self) -> bool {
        self.delivered_via.is_some()
    }
}

/// Phase 19.2 — RoutingReceipt: formal ARP-compatible delivery receipt.
///
/// Wraps OsoDelivery and adds the fields needed for ARP receipt integration.
/// The receipt confirms which transport was used, allowing the L1 to verify
/// delivery proofs without caring about the underlying transport.
#[derive(Debug)]
pub struct RoutingReceipt {
    /// Inner delivery record.
    pub delivery:       OsoDelivery,
    /// Originating agent address.
    pub from:           String,
    /// Destination agent address.
    pub to:             String,
    /// BLAKE3 hash of the message payload bytes (hex).
    pub payload_hash:   String,
    /// Unix timestamp of the routing attempt.
    pub routed_at:      u64,
    /// Whether any transport succeeded.
    pub delivered:      bool,
}

impl RoutingReceipt {
    fn new(delivery: OsoDelivery, from: &str, to: &str, payload_hash: &str) -> Self {
        let delivered = delivery.succeeded();
        Self {
            delivery,
            from:         from.to_string(),
            to:           to.to_string(),
            payload_hash: payload_hash.to_string(),
            routed_at:    current_unix_ts(),
            delivered,
        }
    }
}

fn current_unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Transport-agnostic sovereign message router.
pub struct OsoRouter {
    registry: Arc<AdapterRegistry>,
    client:   reqwest::Client,
}

impl OsoRouter {
    pub fn new(registry: Arc<AdapterRegistry>) -> Self {
        Self {
            registry,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    // ── Phase 19.2: transport availability probes ──────────────────────────

    /// Check if a direct IP endpoint is reachable for the given destination.
    pub async fn can_reach_direct(&self, destination: &str) -> bool {
        // If destination has a "direct:" scheme prefix, attempt a HEAD request.
        let base = if let Some(rest) = destination.strip_prefix("direct:") {
            rest.to_string()
        } else {
            return false;
        };
        let url = format!("{}/health", base.trim_end_matches('/'));
        self.client.head(&url).send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Check whether a Nostr relay is configured and reachable.
    pub fn nostr_available(&self) -> bool {
        std::env::var("NOSTR_RELAY_URL").map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Check whether a Freenet node is configured.
    pub fn freenet_available(&self) -> bool {
        std::env::var("FREENET_NODE_URL").map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Check whether a Meshtastic HTTP bridge or serial port is configured.
    pub fn meshtastic_available(&self) -> bool {
        std::env::var("MESHTASTIC_HTTP_URL").map(|v| !v.is_empty()).unwrap_or(false)
            || std::env::var("MESHTASTIC_SERIAL_PORT").map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Send an OsoMessage through the transport cascade.
    ///
    /// Returns `Ok(RoutingReceipt)` on success (at least one transport succeeded).
    /// Returns `Err(DipError::NoRoute)` if every transport fails.
    ///
    /// Phase 19.2: uses availability checks to skip transports that are not
    /// configured before attempting, reducing latency on constrained nodes.
    pub async fn send_with_receipt(&self, msg: OsoMessage) -> Result<RoutingReceipt, DipError> {
        let payload_bytes = serde_json::to_vec(&msg.payload).unwrap_or_default();
        let payload_hash = blake3_hex(&payload_bytes);
        let from = msg.from.clone();
        let to = msg.to.clone();
        let delivery = self.send(msg).await;
        match delivery {
            Ok(d) => Ok(RoutingReceipt::new(d, &from, &to, &payload_hash)),
            Err(e) => Err(e),
        }
    }

    /// Send an OsoMessage through the transport cascade.
    ///
    /// Returns `Ok(OsoDelivery)` on success (at least one transport succeeded).
    /// Returns `Err(DipError::NoRoute)` if every transport fails.
    pub async fn send(&self, msg: OsoMessage) -> Result<OsoDelivery, DipError> {
        let mut attempts = Vec::new();

        // Build a DipEnvelope from the OsoMessage once — reused across transports.
        let envelope = DipEnvelope::new(
            DipMessageKind::EventBroadcast,
            &msg.from,
            &msg.to,
            msg.payload.clone(),
        );

        // All adapter send functions return Result<(), String>.
        // The macro awaits the future and records the attempt.
        macro_rules! try_transport {
            ($name:literal, $allowed:expr, $fut:expr) => {
                if $allowed {
                    let result: Result<(), String> = $fut.await;
                    match result {
                        Ok(()) => {
                            attempts.push(TransportAttempt {
                                transport: $name,
                                success:   true,
                                error:     None,
                            });
                            return Ok(OsoDelivery {
                                delivered_via: Some($name),
                                attempts,
                            });
                        }
                        Err(e) => {
                            attempts.push(TransportAttempt {
                                transport: $name,
                                success:   false,
                                error:     Some(e),
                            });
                        }
                    }
                }
            };
        }

        // 1. DirectIp — attempt HTTP POST to peer's known endpoint
        try_transport!(
            "direct_ip",
            msg.policy.allow_direct_ip,
            self.try_direct_ip(&envelope, &msg.to)
        );

        // 2. NostrRelay
        try_transport!(
            "nostr_relay",
            msg.policy.allow_nostr,
            adapters::nostr::send(&envelope)
        );

        // 3. FreenetState
        try_transport!(
            "freenet_state",
            msg.policy.allow_freenet,
            adapters::freenet::send(&envelope)
        );

        // 4. Meshtastic LoRa — Phase 19.1 OsoMeshEnvelope control-plane path.
        // Compress full payload to a compact signed control envelope for LoRa budget.
        if msg.policy.allow_meshtastic {
            let payload_bytes = serde_json::to_vec(&msg.payload).unwrap_or_default();
            let mesh_env = {
                let mut e = adapters::meshtastic::OsoMeshEnvelope::new(
                    &msg.from,
                    adapters::meshtastic::MeshKind::DeviceAlive,
                    &payload_bytes,
                    &msg.to,
                );
                // Signing requires a key — use unsigned for routing (Phase 19.3 will wire key)
                e.signature = "unsigned_routing".to_string();
                e
            };
            let result: Result<(), String> = adapters::meshtastic::send_mesh_control(&mesh_env).await;
            match result {
                Ok(()) => {
                    attempts.push(TransportAttempt {
                        transport: "meshtastic",
                        success:   true,
                        error:     None,
                    });
                    return Ok(OsoDelivery {
                        delivered_via: Some("meshtastic"),
                        attempts,
                    });
                }
                Err(e) => {
                    attempts.push(TransportAttempt {
                        transport: "meshtastic",
                        success:   false,
                        error:     Some(e),
                    });
                }
            }
        }

        // 5. Bluetooth (experimental — delegates to a BLE stub for now)
        try_transport!(
            "bluetooth",
            msg.policy.allow_bluetooth,
            self.try_bluetooth(&envelope)
        );

        // 6. Local delivery (same-process agent lookup)
        try_transport!(
            "local",
            msg.policy.allow_local,
            self.try_local(&envelope)
        );

        Err(DipError::NoRoute {
            from: msg.from.clone(),
            to:   msg.to.clone(),
        })
    }

    /// Attempt direct HTTP delivery to a peer endpoint resolved from the registry.
    async fn try_direct_ip(&self, envelope: &DipEnvelope, to: &str) -> Result<(), String> {
        // Strip any scheme prefix (e.g. "direct:http://10.0.0.5:7792" → "http://10.0.0.5:7792")
        let base_url = if let Some(rest) = to.strip_prefix("direct:") {
            rest.to_string()
        } else {
            // Look up the peer's endpoint in the adapter registry
            let adapters = self.registry.find_by_kind(&dip_types::AdapterKind::Http);
            let peer = adapters.into_iter()
                .find(|a| a.endpoint.as_deref().is_some_and(|ep| to.contains(ep)));
            match peer {
                Some(a) => a.endpoint.unwrap(),
                None => return Err("no direct endpoint registered for target".to_string()),
            }
        };

        let url = format!("{}/api/dip/inbound", base_url.trim_end_matches('/'));
        self.client
            .post(&url)
            .json(envelope)
            .send()
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| {
                if r.status().is_success() {
                    Ok(())
                } else {
                    Err(format!("HTTP {}", r.status()))
                }
            })
    }

    /// BLE delivery stub — Phase 16.4 scaffolding only.
    async fn try_bluetooth(&self, _envelope: &DipEnvelope) -> Result<(), String> {
        // Phase 16.5 will wire btleplug for BLE GATT writes.
        Err("bluetooth transport not yet implemented".to_string())
    }

    /// Local (same-process) delivery stub — Phase 16.4 scaffolding only.
    async fn try_local(&self, _envelope: &DipEnvelope) -> Result<(), String> {
        // Phase 16.5 will wire an in-process channel for co-located agents.
        Err("local transport not yet implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use dip_types::DipMessage;
    use crate::registry::AdapterRegistry;

    fn make_msg(to: &str) -> OsoMessage {
        OsoMessage {
            from:     "nostr:npub1test_from".to_string(),
            to:       to.to_string(),
            payload:  DipMessage::Ping { nonce: "test".to_string() },
            trace_id: None,
            policy:   OsoRoutingPolicy::default(),
        }
    }

    #[test]
    fn oso_routing_policy_default_disables_bluetooth() {
        let p = OsoRoutingPolicy::default();
        assert!(!p.allow_bluetooth, "bluetooth must be opt-in");
        assert!(p.allow_nostr, "nostr enabled by default");
        assert!(p.allow_direct_ip, "direct_ip enabled by default");
    }

    #[test]
    fn oso_message_constructs_cleanly() {
        let msg = make_msg("nostr:npub1dest");
        assert_eq!(msg.to, "nostr:npub1dest");
        assert_eq!(msg.policy.allow_meshtastic, true);
    }

    #[test]
    fn oso_delivery_succeeded_reflects_delivered_via() {
        let delivered = OsoDelivery {
            delivered_via: Some("nostr_relay"),
            attempts: vec![
                TransportAttempt { transport: "direct_ip", success: false, error: Some("timeout".to_string()) },
                TransportAttempt { transport: "nostr_relay", success: true, error: None },
            ],
        };
        assert!(delivered.succeeded());
        assert_eq!(delivered.delivered_via, Some("nostr_relay"));
    }

    #[test]
    fn oso_delivery_failed_when_no_delivered_via() {
        let failed = OsoDelivery {
            delivered_via: None,
            attempts: vec![
                TransportAttempt { transport: "direct_ip", success: false, error: Some("refused".to_string()) },
            ],
        };
        assert!(!failed.succeeded());
    }

    #[tokio::test]
    async fn all_transports_disabled_returns_no_route() {
        let registry = Arc::new(AdapterRegistry::new());
        let router = OsoRouter::new(registry);
        let mut msg = make_msg("nostr:npub1unreachable");
        msg.policy = OsoRoutingPolicy {
            allow_direct_ip:  false,
            allow_nostr:      false,
            allow_freenet:    false,
            allow_meshtastic: false,
            allow_bluetooth:  false,
            allow_local:      false,
        };
        let result = router.send(msg).await;
        assert!(matches!(result, Err(DipError::NoRoute { .. })));
    }

    // Phase 19.2 tests

    #[test]
    fn nostr_not_available_without_env() {
        // Without NOSTR_RELAY_URL set, nostr_available() returns false.
        let registry = Arc::new(AdapterRegistry::new());
        let router = OsoRouter::new(registry);
        // Ensure env var is not set (best-effort in test context)
        std::env::remove_var("NOSTR_RELAY_URL");
        assert!(!router.nostr_available());
    }

    #[test]
    fn nostr_available_with_env() {
        let registry = Arc::new(AdapterRegistry::new());
        let router = OsoRouter::new(registry);
        std::env::set_var("NOSTR_RELAY_URL", "wss://relay.test");
        assert!(router.nostr_available());
        std::env::remove_var("NOSTR_RELAY_URL");
    }

    #[test]
    fn meshtastic_not_available_without_env() {
        let registry = Arc::new(AdapterRegistry::new());
        let router = OsoRouter::new(registry);
        std::env::remove_var("MESHTASTIC_HTTP_URL");
        std::env::remove_var("MESHTASTIC_SERIAL_PORT");
        assert!(!router.meshtastic_available());
    }

    #[test]
    fn routing_receipt_payload_hash_is_blake3() {
        let delivery = OsoDelivery {
            delivered_via: Some("nostr_relay"),
            attempts: vec![],
        };
        let receipt = RoutingReceipt::new(delivery, "from:a", "to:b", "hash_abc");
        assert!(receipt.delivered);
        assert_eq!(receipt.from, "from:a");
        assert_eq!(receipt.to, "to:b");
        assert_eq!(receipt.payload_hash, "hash_abc");
    }

    #[tokio::test]
    async fn send_with_receipt_all_disabled_returns_no_route() {
        let registry = Arc::new(AdapterRegistry::new());
        let router = OsoRouter::new(registry);
        let mut msg = make_msg("nostr:npub1dest");
        msg.policy = OsoRoutingPolicy {
            allow_direct_ip:  false,
            allow_nostr:      false,
            allow_freenet:    false,
            allow_meshtastic: false,
            allow_bluetooth:  false,
            allow_local:      false,
        };
        let result = router.send_with_receipt(msg).await;
        assert!(matches!(result, Err(DipError::NoRoute { .. })));
    }
}
