/// Phase 19.1 — OsoMeshEnvelope: compact signed control-plane message for LoRa.
///
/// LoRa packet budget: ~256 bytes.
/// OsoMeshEnvelope wire size: 32 (agent_id) + 4 (kind) + 32 (payload_hash)
///                           + 64 (signature) + 8 (timestamp) + 1 (version)
///                           = ~141 bytes packed — well within budget.
use dip_types::DipEnvelope;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Sha256, Digest};

/// Send a DipEnvelope via Meshtastic mesh radio.
/// to address format: "mesh:<node_id>" where node_id is a 32-bit hex node number.
///
/// Transport: serial port (MESHTASTIC_SERIAL_PORT env) or HTTP (MESHTASTIC_HTTP_URL env).
pub async fn send(envelope: &DipEnvelope) -> Result<(), String> {
    let node_id = envelope.to.trim_start_matches("mesh:");

    if let Ok(http_url) = std::env::var("MESHTASTIC_HTTP_URL") {
        send_via_http(&http_url, node_id, envelope).await
    } else {
        // Serial path deferred (requires meshtastic serial protocol library).
        tracing::info!("meshtastic adapter: queue msg to {node_id} (no transport configured)");
        Ok(())
    }
}

async fn send_via_http(base_url: &str, node_id: &str, envelope: &DipEnvelope) -> Result<(), String> {
    // Meshtastic HTTP API: POST /api/v1/sendtext or /api/v1/sendpacket
    let payload_json = serde_json::to_string(&envelope.payload).unwrap_or_default();
    // Truncate to Meshtastic 240-byte limit
    let truncated = &payload_json[..payload_json.len().min(240)];

    let body = json!({
        "to":   node_id,
        "text": truncated,
        "wantAck": false,
    });

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/sendtext", base_url.trim_end_matches('/'));
    client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Parse an inbound Meshtastic packet into a DipEnvelope.
pub fn receive(packet: &Value) -> Option<DipEnvelope> {
    let text = packet
        .get("decoded")
        .and_then(|d| d.get("text"))
        .and_then(|t| t.as_str())?;
    serde_json::from_str::<DipEnvelope>(text).ok()
}

// ─── Phase 19.1: OsoMeshEnvelope ───────────────────────────────────────────

/// Control-plane signal kinds for LoRa mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MeshKind {
    /// "Agent X has receipt Y — fetch when back online"
    ReceiptAvailable = 1,
    /// "Job available, capability Z needed"
    WorkRequest      = 2,
    /// Heartbeat: physical agent still alive
    DeviceAlive      = 3,
    /// Halt all work immediately
    EmergencyStop    = 4,
    /// Significant L1 state change occurred
    StateChanged     = 5,
}

impl MeshKind {
    pub fn as_u8(self) -> u8 { self as u8 }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::ReceiptAvailable),
            2 => Some(Self::WorkRequest),
            3 => Some(Self::DeviceAlive),
            4 => Some(Self::EmergencyStop),
            5 => Some(Self::StateChanged),
            _ => None,
        }
    }
}

/// Compact signed envelope for LoRa control-plane messaging.
///
/// Wire format (JSON; packed binary is future work):
/// Total JSON size is ~220 bytes for typical values — fits in one LoRa packet.
///
/// Full payload lives off-mesh; `payload_hash` is a commitment.
/// Receiver fetches full payload over another transport when available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsoMeshEnvelope {
    /// Protocol version (currently 1).
    pub version: u8,
    /// Sending agent's 32-byte pubkey, hex-encoded.
    pub agent_id: String,
    /// Signal kind.
    pub kind: u8,
    /// SHA-256 of full payload bytes (hex). Receiver fetches payload out-of-band.
    pub payload_hash: String,
    /// Ed25519 signature over `signing_bytes()` (hex-encoded, 128 chars).
    pub signature: String,
    /// Unix timestamp (seconds).
    pub timestamp: u64,
    /// Optional destination node id ("mesh:<hex>"). Empty = broadcast.
    pub destination: String,
}

impl OsoMeshEnvelope {
    /// Canonical bytes that must be signed: version || kind || payload_hash_bytes || ts_be
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(1 + 1 + 32 + 8);
        data.push(self.version);
        data.push(self.kind);
        // decode payload_hash into 32 bytes if possible; else use raw UTF-8
        let hash_bytes = hex::decode(&self.payload_hash)
            .unwrap_or_else(|_| self.payload_hash.as_bytes().to_vec());
        data.extend_from_slice(&hash_bytes[..hash_bytes.len().min(32)]);
        data.extend_from_slice(&self.timestamp.to_be_bytes());
        data
    }

    /// Compute SHA-256 of arbitrary payload bytes and return hex string.
    pub fn hash_payload(payload: &[u8]) -> String {
        let digest = Sha256::digest(payload);
        hex::encode(digest)
    }

    /// Build an unsigned envelope (caller must call `sign()` before sending).
    pub fn new(agent_id: &str, kind: MeshKind, payload: &[u8], destination: &str) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        OsoMeshEnvelope {
            version:      1,
            agent_id:     agent_id.to_string(),
            kind:         kind.as_u8(),
            payload_hash: Self::hash_payload(payload),
            signature:    String::new(),
            timestamp,
            destination:  destination.to_string(),
        }
    }

    /// Sign this envelope using an Ed25519 signing key.
    pub fn sign(&mut self, signing_key: &ed25519_dalek::SigningKey) {
        use ed25519_dalek::Signer;
        let bytes = self.signing_bytes();
        let sig = signing_key.sign(&bytes);
        self.signature = hex::encode(sig.to_bytes());
    }

    /// Verify the signature on this envelope.
    ///
    /// `verifying_key` is the sender's Ed25519 public key.
    pub fn verify(&self, verifying_key: &ed25519_dalek::VerifyingKey) -> bool {
        use ed25519_dalek::Verifier;
        let sig_bytes = match hex::decode(&self.signature) {
            Ok(b) if b.len() == 64 => b,
            _ => return false,
        };
        let sig_arr: [u8; 64] = match sig_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        let data = self.signing_bytes();
        verifying_key.verify(&data, &sig).is_ok()
    }

    /// Serialize to a compact JSON string suitable for LoRa transmission.
    pub fn to_wire(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    /// Deserialize from a LoRa packet string.
    pub fn from_wire(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}

/// Send a signed OsoMeshEnvelope over the Meshtastic transport.
///
/// This is the Phase 19.1 control-plane path — call this instead of `send()`
/// when you need Ed25519-signed LoRa control signals.
pub async fn send_mesh_control(env: &OsoMeshEnvelope) -> Result<(), String> {
    let wire = env.to_wire()?;
    let wire_len = wire.len();

    // LoRa packet budget: ~256 bytes. Warn if we're over.
    if wire_len > 256 {
        tracing::warn!(
            "OsoMeshEnvelope wire size {wire_len} bytes exceeds LoRa 256-byte budget"
        );
    }

    let node_id = env.destination.trim_start_matches("mesh:");

    if let Ok(http_url) = std::env::var("MESHTASTIC_HTTP_URL") {
        let body = json!({
            "to":      if node_id.is_empty() { "^all" } else { node_id },
            "text":    wire,
            "wantAck": false,
        });
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/sendtext", http_url.trim_end_matches('/'));
        client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        tracing::info!(
            "meshtastic mesh-control: queue kind={} to {} (no transport configured)",
            env.kind,
            env.destination
        );
        Ok(())
    }
}

/// Parse an inbound LoRa packet as an OsoMeshEnvelope (if it is one).
pub fn receive_mesh_control(packet: &Value) -> Option<OsoMeshEnvelope> {
    let text = packet
        .get("decoded")
        .and_then(|d| d.get("text"))
        .and_then(|t| t.as_str())?;
    OsoMeshEnvelope::from_wire(text).ok()
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn mesh_kind_round_trips() {
        for k in [
            MeshKind::ReceiptAvailable,
            MeshKind::WorkRequest,
            MeshKind::DeviceAlive,
            MeshKind::EmergencyStop,
            MeshKind::StateChanged,
        ] {
            assert_eq!(MeshKind::from_u8(k.as_u8()), Some(k));
        }
        assert!(MeshKind::from_u8(99).is_none());
    }

    #[test]
    fn envelope_constructs_and_serialises() {
        let env = OsoMeshEnvelope::new(
            "deadbeefdeadbeef",
            MeshKind::DeviceAlive,
            b"heartbeat payload",
            "mesh:0xaabbccdd",
        );
        let wire = env.to_wire().expect("serialise");
        assert!(wire.len() <= 256, "wire size {} exceeds LoRa budget", wire.len());
        let back = OsoMeshEnvelope::from_wire(&wire).expect("deserialise");
        assert_eq!(back.kind, MeshKind::DeviceAlive.as_u8());
        assert_eq!(back.version, 1);
    }

    #[test]
    fn envelope_sign_and_verify() {
        let key = SigningKey::generate(&mut OsRng);
        let mut env = OsoMeshEnvelope::new(
            &hex::encode(key.verifying_key().to_bytes()),
            MeshKind::WorkRequest,
            b"job_payload_bytes",
            "mesh:0x12345678",
        );
        env.sign(&key);
        assert!(!env.signature.is_empty());
        assert!(env.verify(&key.verifying_key()), "signature must verify");
    }

    #[test]
    fn tampered_envelope_fails_verification() {
        let key = SigningKey::generate(&mut OsRng);
        let mut env = OsoMeshEnvelope::new(
            &hex::encode(key.verifying_key().to_bytes()),
            MeshKind::EmergencyStop,
            b"stop_payload",
            "",
        );
        env.sign(&key);
        // Tamper with the kind after signing
        env.kind = MeshKind::DeviceAlive.as_u8();
        assert!(!env.verify(&key.verifying_key()), "tampered envelope must fail");
    }

    #[test]
    fn payload_hash_is_sha256_hex() {
        let hash = OsoMeshEnvelope::hash_payload(b"test");
        // SHA-256 of "test" = 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
        assert_eq!(hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
        assert_eq!(hash.len(), 64);
    }
}
