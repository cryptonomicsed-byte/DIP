/// Phase 17.2 — Freenet → L1 State Commitment Bridge.
///
/// When agent-state changes need L1 canonicalization, this module constructs
/// the `StateCommitmentTx` and submits it to the Ọ̀ṢỌ́ L1 via ABCI.
///
/// Only significant state transitions trigger an L1 commitment:
///   - Reputation change
///   - New capability grant or revocation
///   - Work completion
///   - Agent tier change
///   - L1 state root update from NIP-OSO-07
///
/// NOT every Nostr event or profile update needs L1 finality.

use serde::{Deserialize, Serialize};

/// Kinds of state transitions that warrant L1 canonicalization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentKind {
    ReputationChange,
    CapabilityGranted,
    CapabilityRevoked,
    WorkCompletion,
    TierChange,
    L1StateRootUpdate,
    AgentMigration,
    /// Catch-all for caller-defined significant events.
    Custom(String),
}

/// A transaction submitted to the Ọ̀ṢỌ́ L1 to canonicalize a Freenet state update.
///
/// Flow:
///   1. Ọmọ Kọ́dà2 detects significant state change
///   2. Constructs StateCommitmentTx: hash of new Freenet state + ARP receipt hash
///   3. Submits to Ọ̀ṢỌ́ L1 via ABCI (Phase 15.4 endpoint, currently stubbed)
///   4. L1 updates agent_state_root
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCommitmentTx {
    /// Hex Ed25519 pubkey of the submitting agent.
    pub agent_npub: String,
    /// BLAKE3 hash of the Freenet AgentPublicState JSON at commit time.
    pub freenet_state_hash: String,
    /// ARP receipt hash covering the triggering action (hex).
    pub arp_receipt_hash: String,
    /// What kind of transition is being committed.
    pub commitment_kind: CommitmentKind,
    /// Unix timestamp.
    pub timestamp: u64,
    /// Freenet state version at commit time.
    pub freenet_version: u64,
    /// Optional Nostr event id that triggered this commitment (NIP-OSO-07).
    pub nostr_event_id: Option<String>,
    /// Ed25519 signature over `signing_bytes()` by the agent's npub key.
    pub signature: String,
}

impl StateCommitmentTx {
    /// Canonical bytes that the agent signs to authorize this commitment.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let signable = serde_json::json!({
            "agent_npub":          &self.agent_npub,
            "freenet_state_hash":  &self.freenet_state_hash,
            "arp_receipt_hash":    &self.arp_receipt_hash,
            "commitment_kind":     &self.commitment_kind,
            "timestamp":           self.timestamp,
            "freenet_version":     self.freenet_version,
        });
        signable.to_string().into_bytes()
    }

    /// Verify the agent's signature on this transaction.
    pub fn verify_signature(&self) -> bool {
        use ed25519_dalek::Verifier;
        let pub_bytes = match hex::decode(&self.agent_npub) {
            Ok(b) if b.len() == 32 => b,
            _ => return false,
        };
        let pub_arr: [u8; 32] = match pub_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let vk = match ed25519_dalek::VerifyingKey::from_bytes(&pub_arr) {
            Ok(k) => k,
            Err(_) => return false,
        };
        let sig_bytes = match hex::decode(&self.signature) {
            Ok(b) if b.len() == 64 => b,
            _ => return false,
        };
        let sig_arr: [u8; 64] = match sig_bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        vk.verify(&self.signing_bytes(), &sig).is_ok()
    }
}

/// Client for submitting StateCommitmentTx to the Ọ̀ṢỌ́ L1 ABCI endpoint.
pub struct L1CommitmentClient {
    /// Base URL of the ABCI RPC endpoint (e.g. "http://localhost:26657").
    /// Phase 15.4 will wire a real osovm-chain devnet here.
    abci_url: String,
    client:   reqwest::Client,
}

impl L1CommitmentClient {
    pub fn new(abci_url: impl Into<String>) -> Self {
        Self {
            abci_url: abci_url.into(),
            client:   reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Submit a StateCommitmentTx to the L1 ABCI broadcast_tx endpoint.
    ///
    /// Fail-open: returns Ok(None) if ABCI is unreachable (not yet deployed).
    /// Returns Ok(Some(tx_hash)) on successful submission.
    pub async fn submit(
        &self,
        tx: &StateCommitmentTx,
    ) -> Result<Option<String>, String> {
        if self.abci_url.is_empty() {
            tracing::debug!("L1CommitmentClient: no ABCI URL configured — skipping L1 commit");
            return Ok(None);
        }

        let tx_bytes = serde_json::to_vec(tx).map_err(|e| e.to_string())?;
        // ABCI broadcast_tx_async: POST /broadcast_tx_async?tx=<hex>
        let tx_hex = hex::encode(&tx_bytes);
        let url = format!(
            "{}/broadcast_tx_async?tx={}",
            self.abci_url.trim_end_matches('/'),
            tx_hex
        );

        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await
                    .unwrap_or(serde_json::Value::Null);
                let hash = body["result"]["hash"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                tracing::info!(
                    agent_npub = %tx.agent_npub,
                    kind = ?tx.commitment_kind,
                    tx_hash = %hash,
                    "L1CommitmentClient: state commitment submitted"
                );
                Ok(Some(hash))
            }
            Ok(resp) => {
                let status = resp.status();
                tracing::warn!(
                    abci_url = %self.abci_url,
                    status = %status,
                    "L1CommitmentClient: ABCI returned non-success"
                );
                Ok(None)
            }
            Err(e) => {
                tracing::warn!(
                    abci_url = %self.abci_url,
                    err = %e,
                    "L1CommitmentClient: ABCI unreachable (fail-open)"
                );
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn make_tx(key: &SigningKey, kind: CommitmentKind) -> StateCommitmentTx {
        use ed25519_dalek::Signer;
        let mut tx = StateCommitmentTx {
            agent_npub:         hex::encode(key.verifying_key().to_bytes()),
            freenet_state_hash: "abc123".to_string(),
            arp_receipt_hash:   "def456".to_string(),
            commitment_kind:    kind,
            timestamp:          1_700_000_000,
            freenet_version:    7,
            nostr_event_id:     None,
            signature:          String::new(),
        };
        let sig = key.sign(&tx.signing_bytes());
        tx.signature = hex::encode(sig.to_bytes());
        tx
    }

    #[test]
    fn state_commitment_tx_sign_and_verify() {
        let key = SigningKey::generate(&mut OsRng);
        let tx = make_tx(&key, CommitmentKind::ReputationChange);
        assert!(tx.verify_signature(), "signature must verify");
    }

    #[test]
    fn tampered_tx_fails_verification() {
        let key = SigningKey::generate(&mut OsRng);
        let mut tx = make_tx(&key, CommitmentKind::WorkCompletion);
        tx.freenet_state_hash = "tampered".to_string();
        assert!(!tx.verify_signature(), "tampered tx must fail");
    }

    #[test]
    fn commitment_kind_serialises() {
        let kinds = vec![
            CommitmentKind::ReputationChange,
            CommitmentKind::CapabilityGranted,
            CommitmentKind::CapabilityRevoked,
            CommitmentKind::WorkCompletion,
            CommitmentKind::TierChange,
            CommitmentKind::L1StateRootUpdate,
            CommitmentKind::AgentMigration,
            CommitmentKind::Custom("custom_event".to_string()),
        ];
        for k in &kinds {
            let s = serde_json::to_string(k).expect("serialise");
            let back: CommitmentKind = serde_json::from_str(&s).expect("deserialise");
            assert_eq!(*k, back);
        }
    }

    #[tokio::test]
    async fn client_fails_open_when_no_abci_url() {
        let client = L1CommitmentClient::new("");
        let key = SigningKey::generate(&mut OsRng);
        let tx = make_tx(&key, CommitmentKind::TierChange);
        let result = client.submit(&tx).await.expect("should not error");
        assert!(result.is_none(), "no ABCI URL → should return None");
    }

    #[tokio::test]
    async fn client_fails_open_when_abci_unreachable() {
        let client = L1CommitmentClient::new("http://127.0.0.1:19999");
        let key = SigningKey::generate(&mut OsRng);
        let tx = make_tx(&key, CommitmentKind::ReputationChange);
        let result = client.submit(&tx).await.expect("fail-open should not Err");
        assert!(result.is_none(), "unreachable ABCI → should return None");
    }
}
