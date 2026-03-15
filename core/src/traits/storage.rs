/// Storage abstraction trait for platform-agnostic object storage
///
/// Implementations:
/// - Cloudflare: R2
/// - Vercel: Vercel Blob
/// - Netlify: Netlify Blobs
/// - Railway: S3-compatible storage
/// - Generic: AWS S3, MinIO, Backblaze B2

use super::PlatformResult;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait StorageProvider {
    /// Store an object with the given key and content
    ///
    /// # Arguments
    /// * `key` - Object key/path (e.g., "media/images/123.jpg")
    /// * `data` - Object content as bytes
    /// * `content_type` - MIME type (e.g., "image/jpeg")
    ///
    /// # Returns
    /// Public URL to access the object
    ///
    /// # Example
    /// ```rust,ignore
    /// let url = storage.put(
    ///     "media/profile.jpg",
    ///     image_bytes,
    ///     "image/jpeg"
    /// ).await?;
    /// // url: "https://media.dais.social/media/profile.jpg"
    /// ```
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> PlatformResult<String>;

    /// Store an object with additional metadata
    async fn put_with_metadata(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
        metadata: StorageMetadata,
    ) -> PlatformResult<String>;

    /// Retrieve an object by key
    ///
    /// # Returns
    /// Object content as bytes
    async fn get(&self, key: &str) -> PlatformResult<Vec<u8>>;

    /// Get object metadata without downloading content
    async fn head(&self, key: &str) -> PlatformResult<ObjectInfo>;

    /// Delete an object by key
    async fn delete(&self, key: &str) -> PlatformResult<()>;

    /// List objects with the given prefix
    ///
    /// # Example
    /// ```rust,ignore
    /// let objects = storage.list("media/images/").await?;
    /// // Returns all keys starting with "media/images/"
    /// ```
    async fn list(&self, prefix: &str) -> PlatformResult<Vec<String>>;

    /// List objects with pagination and filtering
    async fn list_detailed(&self, options: ListOptions) -> PlatformResult<ListResult>;

    /// Copy an object to a new key
    async fn copy(&self, from: &str, to: &str) -> PlatformResult<()>;

    /// Check if an object exists
    async fn exists(&self, key: &str) -> PlatformResult<bool> {
        match self.head(key).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get the public URL for an object (without fetching content)
    fn public_url(&self, key: &str) -> String;

    /// Generate a signed URL for temporary access
    ///
    /// # Arguments
    /// * `key` - Object key
    /// * `expires_in` - Duration in seconds until URL expires
    async fn signed_url(&self, key: &str, expires_in: u32) -> PlatformResult<String>;
}

/// Storage metadata for objects
#[derive(Debug, Clone, Default)]
pub struct StorageMetadata {
    /// Custom metadata key-value pairs
    pub custom: std::collections::HashMap<String, String>,

    /// Cache-Control header
    pub cache_control: Option<String>,

    /// Content-Disposition header
    pub content_disposition: Option<String>,

    /// Content-Encoding header
    pub content_encoding: Option<String>,
}

impl StorageMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cache_control(mut self, value: impl Into<String>) -> Self {
        self.cache_control = Some(value.into());
        self
    }

    pub fn with_content_disposition(mut self, value: impl Into<String>) -> Self {
        self.content_disposition = Some(value.into());
        self
    }

    pub fn add_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom.insert(key.into(), value.into());
        self
    }
}

/// Information about a stored object
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    /// Object key
    pub key: String,

    /// Content type
    pub content_type: String,

    /// Size in bytes
    pub size: u64,

    /// Last modified timestamp (RFC3339)
    pub last_modified: String,

    /// ETag (for cache validation)
    pub etag: Option<String>,

    /// Custom metadata
    pub metadata: StorageMetadata,
}

/// Options for listing objects
#[derive(Debug, Clone)]
pub struct ListOptions {
    /// Prefix to filter keys
    pub prefix: Option<String>,

    /// Delimiter for grouping (e.g., "/" for folders)
    pub delimiter: Option<String>,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Continuation token for pagination
    pub cursor: Option<String>,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            prefix: None,
            delimiter: None,
            limit: Some(1000),
            cursor: None,
        }
    }
}

/// Result of listing objects
#[derive(Debug, Clone)]
pub struct ListResult {
    /// List of objects
    pub objects: Vec<ObjectInfo>,

    /// Continuation token for next page (if any)
    pub cursor: Option<String>,

    /// Whether there are more results
    pub has_more: bool,
}
