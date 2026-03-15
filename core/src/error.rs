/// Error types for dais core library

use std::fmt;

/// Result type for core operations
pub type CoreResult<T> = Result<T, CoreError>;

/// Errors that can occur in core logic
#[derive(Debug)]
pub enum CoreError {
    /// Platform error (database, storage, queue, HTTP)
    Platform(crate::traits::PlatformError),

    /// Invalid ActivityPub activity
    InvalidActivity(String),

    /// Invalid AT Protocol data
    InvalidAtProto(String),

    /// Serialization/deserialization error
    Serialization(String),

    /// Signature verification failed
    SignatureError(String),

    /// Resource not found
    NotFound(String),

    /// Unauthorized access
    Unauthorized(String),

    /// Internal error
    Internal(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Platform(e) => write!(f, "Platform error: {}", e),
            CoreError::InvalidActivity(msg) => write!(f, "Invalid activity: {}", msg),
            CoreError::InvalidAtProto(msg) => write!(f, "Invalid AT Protocol data: {}", msg),
            CoreError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            CoreError::SignatureError(msg) => write!(f, "Signature error: {}", msg),
            CoreError::NotFound(msg) => write!(f, "Not found: {}", msg),
            CoreError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            CoreError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for CoreError {}

// Conversions from other error types

impl From<crate::traits::PlatformError> for CoreError {
    fn from(err: crate::traits::PlatformError) -> Self {
        CoreError::Platform(err)
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(err: serde_json::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for CoreError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        CoreError::Serialization(format!("UTF-8 error: {}", err))
    }
}

// Convert to JsValue for WASM

#[cfg(target_arch = "wasm32")]
impl From<CoreError> for wasm_bindgen::JsValue {
    fn from(err: CoreError) -> Self {
        wasm_bindgen::JsValue::from_str(&err.to_string())
    }
}
