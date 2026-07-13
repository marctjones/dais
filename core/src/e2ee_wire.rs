//! Wire-level identity of the dais encrypted-message envelope.
//!
//! These constants and the envelope check live outside `e2ee_mls` because that
//! module is gated behind the `mls` feature and pulls in OpenMLS, which the
//! Cloudflare workers do not build. The inbox runs in the workers and still has
//! to decide whether an inbound `daisEncryptedMessage` is something this build
//! understands, so the check has to be available without OpenMLS.
//!
//! dais speaks exactly one encrypted-message format: MLS/RFC 9420 carried in a
//! v2 `daisEncryptedMessage` envelope. The retired v1/RSA `encryptedMessage`
//! format is not accepted anywhere.

use serde_json::Value;

/// Protocol identifier stored on every E2EE row and carried in every envelope.
pub const DAIS_MLS_PROTOCOL: &str = "mls-rfc9420";

/// Envelope version. v1 was the retired RSA fallback format.
pub const DAIS_MLS_ENVELOPE_VERSION: u8 = 2;

/// True when `value` is an envelope this build can carry.
///
/// The inbox gates persistence on this. Without it, a peer could hand us any
/// JSON under `daisEncryptedMessage` and we would store it as an `mls-rfc9420`
/// row, because the persistence path hardcodes the protocol column rather than
/// reading it from the envelope. That would let retired formats walk back into
/// the database over federation after they had been purged.
pub fn is_supported_envelope(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };

    let version_matches = object
        .get("v")
        .and_then(Value::as_u64)
        .is_some_and(|version| version == u64::from(DAIS_MLS_ENVELOPE_VERSION));

    let protocol_matches = object
        .get("protocol")
        .and_then(Value::as_str)
        .is_some_and(|protocol| protocol == DAIS_MLS_PROTOCOL);

    // An envelope with no ciphertext is not a message, whatever it claims to be.
    let has_ciphertext = object
        .get("ciphertext")
        .and_then(Value::as_str)
        .is_some_and(|ciphertext| !ciphertext.trim().is_empty());

    version_matches && protocol_matches && has_ciphertext
}

#[cfg(test)]
mod tests {
    use super::is_supported_envelope;
    use serde_json::json;

    fn v2_envelope() -> serde_json::Value {
        json!({
            "v": 2,
            "protocol": "mls-rfc9420",
            "groupId": "Z3JvdXA=",
            "epoch": 1,
            "senderActorId": "https://social.dais.social/users/social",
            "senderDeviceId": "alice-mac",
            "ciphertext": "Y2lwaGVy"
        })
    }

    #[test]
    fn accepts_a_v2_mls_envelope() {
        assert!(is_supported_envelope(&v2_envelope()));
    }

    #[test]
    fn rejects_the_retired_v1_rsa_envelope() {
        assert!(!is_supported_envelope(&json!({
            "v": 1,
            "alg": "RSA-OAEP",
            "ciphertext": "bGVnYWN5"
        })));
    }

    #[test]
    fn rejects_a_v1_envelope_that_claims_the_mls_protocol() {
        let mut envelope = v2_envelope();
        envelope["v"] = json!(1);
        assert!(!is_supported_envelope(&envelope));
    }

    #[test]
    fn rejects_a_v2_envelope_carrying_an_unknown_protocol() {
        let mut envelope = v2_envelope();
        envelope["protocol"] = json!("dais-mls-v1");
        assert!(!is_supported_envelope(&envelope));
    }

    #[test]
    fn rejects_envelopes_with_no_usable_ciphertext() {
        let mut envelope = v2_envelope();
        envelope["ciphertext"] = json!("   ");
        assert!(!is_supported_envelope(&envelope));

        let mut missing = v2_envelope();
        missing.as_object_mut().unwrap().remove("ciphertext");
        assert!(!is_supported_envelope(&missing));
    }

    #[test]
    fn rejects_values_that_are_not_envelopes() {
        assert!(!is_supported_envelope(&json!("mls-rfc9420")));
        assert!(!is_supported_envelope(&json!(null)));
        assert!(!is_supported_envelope(&json!([2, "mls-rfc9420"])));
    }

    #[test]
    fn rejects_a_version_smuggled_as_a_string() {
        let mut envelope = v2_envelope();
        envelope["v"] = json!("2");
        assert!(!is_supported_envelope(&envelope));
    }
}
