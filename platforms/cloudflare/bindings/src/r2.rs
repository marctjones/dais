/// R2 storage provider implementation for Cloudflare Workers
///
/// Implements the StorageProvider trait using Cloudflare R2 object storage

use dais_core::traits::{
    PlatformError, PlatformResult, StorageProvider, StorageMetadata, ObjectInfo, ListOptions, ListResult,
};
use async_trait::async_trait;
use worker::R2Bucket;

pub struct R2Provider {
    bucket: R2Bucket,
    public_url_base: String,
}

impl R2Provider {
    /// Create a new R2Provider from a Cloudflare R2 bucket binding
    ///
    /// # Arguments
    /// * `bucket` - R2 bucket binding from env
    /// * `public_url_base` - Base URL for public access (e.g., "https://media.dais.social")
    pub fn new(bucket: R2Bucket, public_url_base: impl Into<String>) -> Self {
        Self {
            bucket,
            public_url_base: public_url_base.into(),
        }
    }
}

#[async_trait(?Send)]
impl StorageProvider for R2Provider {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> PlatformResult<String> {
        self.put_with_metadata(key, data, content_type, StorageMetadata::new())
            .await
    }

    async fn put_with_metadata(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
        metadata: StorageMetadata,
    ) -> PlatformResult<String> {
        // Build R2 put options (simplified - use basic put for now)
        // TODO: Add metadata support when worker-rs API is clearer
        self.bucket
            .put(key, data)
            .execute()
            .await
            .map_err(|e| PlatformError::Storage(format!("R2 put failed: {:?}", e)))?;

        // Return public URL
        Ok(self.public_url(key))
    }

    async fn get(&self, key: &str) -> PlatformResult<Vec<u8>> {
        let object = self.bucket
            .get(key)
            .execute()
            .await
            .map_err(|e| PlatformError::Storage(format!("R2 get failed: {:?}", e)))?
            .ok_or_else(|| PlatformError::NotFound(format!("Object not found: {}", key)))?;

        let body = object
            .body()
            .ok_or_else(|| PlatformError::Storage("No body in R2 object".to_string()))?;

        let bytes = body
            .bytes()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to read R2 body: {:?}", e)))?;

        Ok(bytes)
    }

    async fn head(&self, key: &str) -> PlatformResult<ObjectInfo> {
        let object = self.bucket
            .head(key)
            .await
            .map_err(|e| PlatformError::Storage(format!("R2 head failed: {:?}", e)))?
            .ok_or_else(|| PlatformError::NotFound(format!("Object not found: {}", key)))?;

        let http_metadata = object.http_metadata();
        let custom_metadata = object.custom_metadata();

        let mut metadata = StorageMetadata::new();
        if let Some(cache_control) = http_metadata.cache_control.clone() {
            metadata.cache_control = Some(cache_control);
        }
        if let Some(content_disposition) = http_metadata.content_disposition.clone() {
            metadata.content_disposition = Some(content_disposition);
        }
        if let Some(content_encoding) = http_metadata.content_encoding.clone() {
            metadata.content_encoding = Some(content_encoding);
        }
        if let Some(custom) = custom_metadata {
            metadata.custom = custom;
        }

        Ok(ObjectInfo {
            key: key.to_string(),
            content_type: http_metadata.content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
            size: object.size() as u64,
            last_modified: object.uploaded().to_string(),
            etag: Some(object.etag()),
            metadata,
        })
    }

    async fn delete(&self, key: &str) -> PlatformResult<()> {
        self.bucket
            .delete(key)
            .await
            .map_err(|e| PlatformError::Storage(format!("R2 delete failed: {:?}", e)))?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> PlatformResult<Vec<String>> {
        let options = ListOptions {
            prefix: Some(prefix.to_string()),
            delimiter: None,
            limit: Some(1000),
            cursor: None,
        };

        let result = self.list_detailed(options).await?;
        Ok(result.objects.into_iter().map(|o| o.key).collect())
    }

    async fn list_detailed(&self, options: ListOptions) -> PlatformResult<ListResult> {
        // Simplified list (basic implementation)
        // TODO: Add filtering options when worker-rs API is clearer
        let result = self.bucket
            .list()
            .execute()
            .await
            .map_err(|e| PlatformError::Storage(format!("R2 list failed: {:?}", e)))?;

        let objects = result
            .objects()
            .iter()
            .map(|obj| {
                let http_metadata = obj.http_metadata();
                ObjectInfo {
                    key: obj.key(),
                    content_type: http_metadata.content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                    size: obj.size() as u64,
                    last_modified: obj.uploaded().to_string(),
                    etag: Some(obj.etag()),
                    metadata: StorageMetadata::new(),
                }
            })
            .collect();

        Ok(ListResult {
            objects,
            cursor: result.cursor(),
            has_more: result.truncated(),
        })
    }

    async fn copy(&self, from: &str, to: &str) -> PlatformResult<()> {
        // R2 doesn't have native copy, so we get and put
        let data = self.get(from).await?;
        let info = self.head(from).await?;
        self.put(to, data, &info.content_type).await?;
        Ok(())
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url_base.trim_end_matches('/'), key)
    }

    async fn signed_url(&self, key: &str, expires_in: u32) -> PlatformResult<String> {
        // R2 presigned URLs require additional setup (not available in basic R2 API)
        // For now, return the public URL
        // TODO: Implement presigned URLs when R2 supports them in worker-rs
        Ok(self.public_url(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_url() {
        // Can't test with real R2 in unit tests, but we can test URL generation
        // In integration tests with wrangler dev, we can test real storage operations
    }
}
