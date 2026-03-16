/// Platform abstraction traits for dais
///
/// These traits define the interfaces that platform-specific implementations
/// must provide. This allows the core ActivityPub/AT Protocol logic to remain
/// platform-agnostic.

pub mod database;
pub mod storage;
pub mod queue;
pub mod http;

pub use database::{DatabaseProvider, DatabaseDialect};
pub use storage::{StorageProvider, StorageMetadata, ObjectInfo, ListOptions, ListResult};
pub use queue::{QueueProvider, QueueHandler, QueueMessage, DeliveryMessage, SyncMessage, MediaProcessingMessage, MediaTask};
pub use http::{HttpProvider, Request, Response, Method};

use serde_json::Value;
use std::collections::HashMap;

/// Result type for platform operations
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Errors that can occur during platform operations
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Queue error: {0}")]
    Queue(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Database row representation
#[derive(Debug, Clone)]
pub struct Row {
    pub columns: HashMap<String, Value>,
}

impl Row {
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, value: Value) {
        self.columns.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.columns.get(key)
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_i64())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

/// SQL statement with parameters
#[derive(Debug, Clone)]
pub struct Statement {
    pub sql: String,
    pub params: Vec<Value>,
}

impl Statement {
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            params: Vec::new(),
        }
    }

    pub fn bind(mut self, param: Value) -> Self {
        self.params.push(param);
        self
    }

    pub fn bind_all(mut self, params: Vec<Value>) -> Self {
        self.params.extend(params);
        self
    }
}
