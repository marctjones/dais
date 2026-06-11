//! HTTP Signature generation and verification for ActivityPub
//!
//! Implements the HTTP Signatures draft specification
//! Based on: https://tools.ietf.org/html/draft-cavage-http-signatures

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Deserialize;
use sha2::Sha256;
use std::collections::HashMap;

use crate::error::{CoreError, CoreResult};
use crate::traits::HttpProvider;

pub const INBOUND_SIGNATURE_MAX_SKEW_SECONDS: i64 = 12 * 60 * 60;

/// HTTP Signature header components
#[derive(Debug, Clone)]
pub struct HttpSignature {
    pub key_id: String,
    pub algorithm: String,
    pub headers: Vec<String>,
    pub signature: String,
}

impl HttpSignature {
    /// Parse a Signature header value
    pub fn parse(header_value: &str) -> Result<Self, String> {
        let mut key_id = None;
        let mut algorithm = None;
        let mut headers = None;
        let mut signature = None;

        for part in header_value.split(',') {
            let part = part.trim();
            if let Some((key, value)) = part.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match key {
                    "keyId" => key_id = Some(value.to_string()),
                    "algorithm" => algorithm = Some(value.to_string()),
                    "headers" => {
                        headers = Some(value.split_whitespace().map(String::from).collect())
                    }
                    "signature" => signature = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        Ok(HttpSignature {
            key_id: key_id.ok_or("Missing keyId")?,
            algorithm: algorithm.ok_or("Missing algorithm")?,
            headers: headers.ok_or("Missing headers")?,
            signature: signature.ok_or("Missing signature")?,
        })
    }

    /// Format as a Signature header value
    pub fn to_header(&self) -> String {
        format!(
            r#"keyId="{}",algorithm="{}",headers="{}",signature="{}""#,
            self.key_id,
            self.algorithm,
            self.headers.join(" "),
            self.signature
        )
    }
}

/// Sign a message with an RSA private key
pub fn sign_message(private_key_pem: &str, message: &str) -> Result<String, String> {
    // Parse the private key
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|e| format!("Failed to parse private key: {}", e))?;

    let signing_key = SigningKey::<Sha256>::new(private_key);

    // Sign the message
    let signature = signing_key.sign(message.as_bytes());

    // Encode to base64
    Ok(BASE64.encode(signature.to_bytes()))
}

/// Verify a signature with an RSA public key
pub fn verify_signature_raw(
    public_key_pem: &str,
    message: &str,
    signature_b64: &str,
) -> Result<bool, String> {
    // Parse the public key
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|e| format!("Failed to parse public key: {}", e))?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    // Decode the signature from base64
    let signature_bytes = BASE64
        .decode(signature_b64)
        .map_err(|e| format!("Failed to decode signature: {}", e))?;

    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|e| format!("Invalid signature format: {}", e))?;

    // Verify the signature
    match verifying_key.verify(message.as_bytes(), &signature) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Build the signing string from HTTP headers
pub fn build_signing_string(
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    headers_to_sign: &[String],
) -> Result<String, String> {
    let mut parts = Vec::new();

    for header_name in headers_to_sign {
        let header_lower = header_name.to_lowercase();

        if header_lower == "(request-target)" {
            // Special pseudo-header
            parts.push(format!(
                "(request-target): {} {}",
                method.to_lowercase(),
                path
            ));
        } else {
            // Regular header
            let value = headers
                .get(&header_lower)
                .ok_or_else(|| format!("Missing required header: {}", header_name))?;
            parts.push(format!("{}: {}", header_lower, value));
        }
    }

    Ok(parts.join("\n"))
}

/// Sign an HTTP request
pub fn sign_request(
    private_key_pem: &str,
    key_id: &str,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    headers_to_sign: &[String],
) -> Result<HttpSignature, String> {
    // Build the signing string
    let signing_string = build_signing_string(method, path, headers, headers_to_sign)?;

    // Sign it
    let signature = sign_message(private_key_pem, &signing_string)?;

    Ok(HttpSignature {
        key_id: key_id.to_string(),
        algorithm: "rsa-sha256".to_string(),
        headers: headers_to_sign.to_vec(),
        signature,
    })
}

/// Verify an HTTP request signature
pub fn verify_request(
    public_key_pem: &str,
    http_signature: &HttpSignature,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
) -> Result<bool, String> {
    // Build the signing string
    let signing_string = build_signing_string(method, path, headers, &http_signature.headers)?;

    // Verify the signature
    verify_signature_raw(public_key_pem, &signing_string, &http_signature.signature)
}

pub fn validate_inbound_post_signature_policy(
    http_signature: &HttpSignature,
    headers: &HashMap<String, String>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    require_signed_headers(
        http_signature,
        &["(request-target)", "host", "date", "digest"],
    )?;

    let digest_header = headers
        .get("digest")
        .ok_or_else(|| "Missing required Digest header".to_string())?;
    if digest_header.trim().is_empty() {
        return Err("Digest header is empty".to_string());
    }

    let date_header = headers
        .get("date")
        .ok_or_else(|| "Missing required Date header".to_string())?;
    validate_http_date_window(date_header, now, INBOUND_SIGNATURE_MAX_SKEW_SECONDS)?;

    Ok(())
}

pub fn validate_inbound_post_signature_policy_now(
    http_signature: &HttpSignature,
    headers: &HashMap<String, String>,
) -> Result<(), String> {
    validate_inbound_post_signature_policy(http_signature, headers, chrono::Utc::now())
}

pub fn require_signed_headers(
    http_signature: &HttpSignature,
    required_headers: &[&str],
) -> Result<(), String> {
    for required in required_headers {
        if !http_signature
            .headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case(required))
        {
            return Err(format!(
                "Signature does not cover required header: {}",
                required
            ));
        }
    }
    Ok(())
}

pub fn validate_http_date_window(
    date_header: &str,
    now: chrono::DateTime<chrono::Utc>,
    max_skew_seconds: i64,
) -> Result<(), String> {
    let signed_at = chrono::DateTime::parse_from_rfc2822(date_header)
        .map_err(|e| format!("Invalid Date header: {}", e))?
        .with_timezone(&chrono::Utc);
    let skew = now.signed_duration_since(signed_at).num_seconds().abs();
    if skew > max_skew_seconds {
        return Err(format!(
            "Date header outside allowed replay window: {} seconds",
            skew
        ));
    }
    Ok(())
}

/// Verify SHA-256 digest header
pub fn verify_digest(body: &str, digest_header: &str) -> Result<bool, String> {
    use sha2::Digest;

    let body_hash = Sha256::digest(body.as_bytes());
    let expected_digest = format!("SHA-256={}", BASE64.encode(&body_hash));

    Ok(digest_header == expected_digest)
}

// Structs for parsing actor JSON to extract public key

#[derive(Debug, Deserialize)]
struct ActorObject {
    #[serde(rename = "publicKey")]
    public_key: PublicKeyObject,
}

#[derive(Debug, Deserialize)]
struct PublicKeyObject {
    #[serde(rename = "publicKeyPem")]
    public_key_pem: String,
}

/// Fetch an actor's public key from their ActivityPub profile
pub async fn fetch_actor_public_key(
    http: &dyn HttpProvider,
    actor_url: &str,
) -> CoreResult<String> {
    // Build request
    let mut headers = HashMap::new();
    headers.insert(
        "Accept".to_string(),
        "application/activity+json".to_string(),
    );

    let request = crate::traits::Request {
        url: actor_url.to_string(),
        method: crate::traits::Method::Get,
        headers,
        body: None,
        timeout: Some(30),
        follow_redirects: true,
    };

    // Fetch actor profile
    let response = http
        .fetch(request)
        .await
        .map_err(|e| CoreError::Platform(e))?;

    if response.status < 200 || response.status >= 300 {
        return Err(CoreError::Internal(format!(
            "Failed to fetch actor: HTTP {}",
            response.status
        )));
    }

    // Parse response body
    let actor_json = String::from_utf8(response.body)
        .map_err(|e| CoreError::Serialization(format!("Invalid UTF-8: {}", e)))?;

    // Parse the actor object
    let actor: ActorObject = serde_json::from_str(&actor_json)?;

    Ok(actor.public_key.public_key_pem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_signature_header() {
        let header = r#"keyId="https://example.com/users/alice#main-key",algorithm="rsa-sha256",headers="(request-target) host date",signature="abc123""#;

        let sig = HttpSignature::parse(header).unwrap();

        assert_eq!(sig.key_id, "https://example.com/users/alice#main-key");
        assert_eq!(sig.algorithm, "rsa-sha256");
        assert_eq!(sig.headers, vec!["(request-target)", "host", "date"]);
        assert_eq!(sig.signature, "abc123");
    }

    #[test]
    fn test_build_signing_string() {
        let mut headers = HashMap::new();
        headers.insert("host".to_string(), "example.com".to_string());
        headers.insert(
            "date".to_string(),
            "Mon, 01 Jan 2024 00:00:00 GMT".to_string(),
        );

        let signing_string = build_signing_string(
            "POST",
            "/inbox",
            &headers,
            &[
                "(request-target)".to_string(),
                "host".to_string(),
                "date".to_string(),
            ],
        )
        .unwrap();

        let expected =
            "(request-target): post /inbox\nhost: example.com\ndate: Mon, 01 Jan 2024 00:00:00 GMT";
        assert_eq!(signing_string, expected);
    }

    #[test]
    fn inbound_policy_requires_digest_and_core_signed_headers() {
        let sig = HttpSignature {
            key_id: "https://example.com/users/alice#main-key".to_string(),
            algorithm: "rsa-sha256".to_string(),
            headers: vec![
                "(request-target)".to_string(),
                "host".to_string(),
                "date".to_string(),
            ],
            signature: "abc".to_string(),
        };
        let mut headers = HashMap::new();
        headers.insert("host".to_string(), "social.example".to_string());
        headers.insert(
            "date".to_string(),
            "Thu, 11 Jun 2026 12:00:00 GMT".to_string(),
        );

        let err = validate_inbound_post_signature_policy(
            &sig,
            &headers,
            chrono::DateTime::parse_from_rfc2822("Thu, 11 Jun 2026 12:00:00 GMT")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .unwrap_err();

        assert!(err.contains("digest"));
    }

    #[test]
    fn inbound_policy_accepts_required_headers_with_fresh_date() {
        let sig = HttpSignature {
            key_id: "https://example.com/users/alice#main-key".to_string(),
            algorithm: "rsa-sha256".to_string(),
            headers: vec![
                "(request-target)".to_string(),
                "host".to_string(),
                "date".to_string(),
                "digest".to_string(),
            ],
            signature: "abc".to_string(),
        };
        let mut headers = HashMap::new();
        headers.insert("host".to_string(), "social.example".to_string());
        headers.insert(
            "date".to_string(),
            "Thu, 11 Jun 2026 12:00:00 GMT".to_string(),
        );
        headers.insert("digest".to_string(), "SHA-256=abc".to_string());

        validate_inbound_post_signature_policy(
            &sig,
            &headers,
            chrono::DateTime::parse_from_rfc2822("Thu, 11 Jun 2026 12:01:00 GMT")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .expect("fresh signed request policy should pass");
    }

    #[test]
    fn inbound_policy_rejects_stale_date() {
        let err = validate_http_date_window(
            "Thu, 11 Jun 2026 12:00:00 GMT",
            chrono::DateTime::parse_from_rfc2822("Fri, 12 Jun 2026 01:00:01 GMT")
                .unwrap()
                .with_timezone(&chrono::Utc),
            INBOUND_SIGNATURE_MAX_SKEW_SECONDS,
        )
        .unwrap_err();

        assert!(err.contains("outside allowed replay window"));
    }
}
