//! HTTP Signatures for ActivityPub.
//!
//! The pure signing primitives now live in the `dais-shared` crate (shared with the
//! client). This module re-exports them — preserving every existing
//! `crate::activitypub::signatures::*` path — and keeps the one HTTP-coupled helper,
//! [`fetch_actor_public_key`], here in `dais-core` where the `HttpProvider`
//! abstraction lives.

use std::collections::HashMap;
use serde::Deserialize;

use crate::error::{CoreResult, CoreError};
use crate::traits::HttpProvider;

pub use dais_shared::signatures::{
    build_signing_string, sign_message, sign_request, verify_digest, verify_request,
    verify_signature_raw, HttpSignature,
};

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
    headers.insert("Accept".to_string(), "application/activity+json".to_string());

    let request = crate::traits::Request {
        url: actor_url.to_string(),
        method: crate::traits::Method::Get,
        headers,
        body: None,
        timeout: Some(30),
        follow_redirects: true,
    };

    // Fetch actor profile
    let response = http.fetch(request).await
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
