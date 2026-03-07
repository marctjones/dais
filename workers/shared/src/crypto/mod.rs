//! Cryptography utilities for HTTP signatures
//!
//! Implements HTTP Signature authentication for ActivityPub federation
//! Based on: https://tools.ietf.org/html/draft-cavage-http-signatures

use rsa::{RsaPrivateKey, RsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::signature::{Signer, Verifier, SignatureEncoding};
use rsa::pss::{SigningKey, VerifyingKey, Signature};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub mod http_signature;

pub use http_signature::*;

/// Sign a message with an RSA private key
pub fn sign_message(private_key_pem: &str, message: &str) -> Result<String, String> {
    // Parse the private key
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|e| format!("Failed to parse private key: {}", e))?;

    let signing_key = SigningKey::<Sha256>::new(private_key);

    // Sign the message
    let signature = signing_key
        .sign(message.as_bytes());

    // Encode to base64
    Ok(BASE64.encode(signature.to_bytes()))
}

/// Verify a signature with an RSA public key
pub fn verify_signature(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let private_key_pem = include_str!("../../../../../cli/test_keys/private.pem");
        let public_key_pem = include_str!("../../../../../cli/test_keys/public.pem");

        let message = "test message";
        let signature = sign_message(private_key_pem, message).unwrap();
        let verified = verify_signature(public_key_pem, message, &signature).unwrap();

        assert!(verified);
    }
}
