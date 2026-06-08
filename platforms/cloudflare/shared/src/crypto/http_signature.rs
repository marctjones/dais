//! HTTP Signature generation and verification
//!
//! Implements the HTTP Signatures draft specification for ActivityPub

use super::{sign_message, verify_signature};
use std::collections::HashMap;

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
    verify_signature(public_key_pem, &signing_string, &http_signature.signature)
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
        headers.insert("date".to_string(), "Mon, 01 Jan 2024 00:00:00 GMT".to_string());

        let signing_string = build_signing_string(
            "POST",
            "/inbox",
            &headers,
            &["(request-target)".to_string(), "host".to_string(), "date".to_string()],
        )
        .unwrap();

        let expected = "(request-target): post /inbox\nhost: example.com\ndate: Mon, 01 Jan 2024 00:00:00 GMT";
        assert_eq!(signing_string, expected);
    }
}
