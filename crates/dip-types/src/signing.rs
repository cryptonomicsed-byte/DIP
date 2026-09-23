//! Ed25519 signing and verification for DIP envelopes.
//!
//! Usage pattern:
//!   - At startup, load `DipSigningKey::from_hex(seed_hex)`.
//!   - Call `envelope.sign(&key)` before routing any outbound envelope.
//!   - On inbound, call `verify_envelope_signature(pubkey_hex, data, sig_hex)`.
//!
//! Fail-open by design: if no key is configured, envelopes are sent unsigned
//! and `verify_envelope_signature` returns `false` (caller decides whether to
//! reject based on `DIP_REQUIRE_SIGNATURES`).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// Wraps an Ed25519 signing key for DIP envelope signing.
pub struct DipSigningKey(SigningKey);

impl DipSigningKey {
    /// Load from a 32-byte hex seed (64 hex chars).
    pub fn from_hex(hex_seed: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = hex::decode(hex_seed)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "Ed25519 seed must be exactly 32 bytes (64 hex chars)")?;
        Ok(DipSigningKey(SigningKey::from_bytes(&arr)))
    }

    /// Sign `data` and return the signature as lowercase hex (128 hex chars).
    pub fn sign_envelope_data(&self, data: &str) -> String {
        let sig: Signature = self.0.sign(data.as_bytes());
        hex::encode(sig.to_bytes())
    }

    /// Return the corresponding verifying (public) key as lowercase hex (64 hex chars).
    pub fn verifying_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().as_bytes())
    }
}

/// Verify an Ed25519 signature over `data`.
///
/// Returns `false` (not `Err`) on any decoding or verification failure so callers
/// can apply policy without unwrapping.
///
/// # Arguments
/// * `pubkey_hex` — 32-byte verifying key, lowercase hex (64 chars)
/// * `data`       — the exact bytes that were signed (typically `envelope.canonical_hash()`)
/// * `sig_hex`    — 64-byte signature, lowercase hex (128 chars)
pub fn verify_envelope_signature(pubkey_hex: &str, data: &str, sig_hex: &str) -> bool {
    let Ok(pk_bytes) = hex::decode(pubkey_hex) else {
        return false;
    };
    let Ok(sig_bytes) = hex::decode(sig_hex) else {
        return false;
    };
    let Ok(pk_arr): Result<[u8; 32], _> = pk_bytes.try_into() else {
        return false;
    };
    let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
        return false;
    };
    let Ok(vk) = VerifyingKey::from_bytes(&pk_arr) else {
        return false;
    };
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(data.as_bytes(), &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known 32-byte seed — deterministic for tests; never use in production.
    const TEST_SEED_HEX: &str =
        "0101010101010101010101010101010101010101010101010101010101010101";

    #[test]
    fn round_trip_sign_and_verify() {
        let key = DipSigningKey::from_hex(TEST_SEED_HEX).unwrap();
        let pubkey = key.verifying_key_hex();
        let data = "envelope_hash_goes_here";
        let sig = key.sign_envelope_data(data);

        assert!(
            verify_envelope_signature(&pubkey, data, &sig),
            "valid signature must verify"
        );
    }

    #[test]
    fn wrong_data_fails_verification() {
        let key = DipSigningKey::from_hex(TEST_SEED_HEX).unwrap();
        let pubkey = key.verifying_key_hex();
        let sig = key.sign_envelope_data("original");

        assert!(
            !verify_envelope_signature(&pubkey, "tampered", &sig),
            "signature over different data must not verify"
        );
    }

    #[test]
    fn bad_hex_returns_false() {
        assert!(!verify_envelope_signature("notHex!", "data", "sig"));
        assert!(!verify_envelope_signature(
            "0101010101010101010101010101010101010101010101010101010101010101",
            "data",
            "notHex!"
        ));
    }

    #[test]
    fn wrong_length_pubkey_returns_false() {
        // 31 bytes → should fail gracefully
        assert!(!verify_envelope_signature("010101", "data", &"01".repeat(64)));
    }

    #[test]
    fn verifying_key_hex_is_64_chars() {
        let key = DipSigningKey::from_hex(TEST_SEED_HEX).unwrap();
        assert_eq!(key.verifying_key_hex().len(), 64);
    }
}
