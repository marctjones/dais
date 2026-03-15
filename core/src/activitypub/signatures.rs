/// HTTP Signatures for ActivityPub
///
/// TODO: Migrate signature logic from workers

use crate::{CoreResult, CoreError};

pub fn sign_request(
    method: &str,
    path: &str,
    body: &str,
    private_key: &str,
    key_id: &str,
) -> CoreResult<String> {
    // TODO: Implement HTTP signature
    // - Generate digest
    // - Sign with RSA
    // - Return Signature header value
    Err(CoreError::Internal("Not implemented".to_string()))
}

pub fn verify_signature(
    signature: &str,
    public_key: &str,
    method: &str,
    path: &str,
    body: &str,
) -> CoreResult<bool> {
    // TODO: Implement signature verification
    Err(CoreError::Internal("Not implemented".to_string()))
}
