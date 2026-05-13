// Vercel Blob storage provider
//
// This provider uses Vercel Blob API for object storage (images, videos, etc.)
// Vercel Blob is S3-compatible and provides global CDN delivery.

use async_trait::async_trait;
use dais_core::traits::{StorageProvider, StorageMetadata, ObjectInfo, ListOptions, ListResult, PlatformResult, PlatformError};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Vercel Blob storage provider
///
/// Uses Vercel Blob API for object storage with CDN delivery.
pub struct VercelBlobProvider {
    client: Client,
    token: String,
    base_url: String,
}

#[derive(Serialize)]
#[allow(dead_code)]
struct PutRequest {
    pathname: String,
    #[serde(rename = "contentType")]
    content_type: Option<String>,
}

#[derive(Deserialize)]
struct PutResponse {
    url: String,
    #[serde(rename = "downloadUrl")]
    #[allow(dead_code)]
    download_url: String,
}

impl VercelBlobProvider {
    /// Create a new Vercel Blob provider
    ///
    /// # Arguments
    ///
    /// * `token` - Vercel Blob read/write token (BLOB_READ_WRITE_TOKEN environment variable)
    ///
    /// # Example
    ///
    /// ```
    /// use dais_vercel::VercelBlobProvider;
    ///
    /// let token = std::env::var("BLOB_READ_WRITE_TOKEN")
    ///     .expect("BLOB_READ_WRITE_TOKEN must be set");
    /// let provider = VercelBlobProvider::new(&token);
    /// ```
    pub fn new(token: &str) -> Self {
        Self {
            client: Client::new(),
            token: token.to_string(),
            base_url: "https://blob.vercel-storage.com".to_string(),
        }
    }
}

#[async_trait(?Send)]
impl StorageProvider for VercelBlobProvider {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> PlatformResult<String> {
        // Upload to Vercel Blob
        let response = self
            .client
            .put(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", content_type)
            .body(data)
            .send()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to upload to Vercel Blob: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PlatformError::Storage(format!(
                "Vercel Blob upload failed: {}",
                error_text
            )));
        }

        let blob_response: PutResponse = response
            .json()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to parse response: {}", e)))?;

        Ok(blob_response.url)
    }

    async fn put_with_metadata(
        &self,
        key: &str,
        data: Vec<u8>,
        content_type: &str,
        metadata: StorageMetadata,
    ) -> PlatformResult<String> {
        let mut request = self
            .client
            .put(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", content_type);

        if let Some(cache_control) = &metadata.cache_control {
            request = request.header("Cache-Control", cache_control);
        }
        if let Some(content_disposition) = &metadata.content_disposition {
            request = request.header("Content-Disposition", content_disposition);
        }

        let response = request
            .body(data)
            .send()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to upload: {}", e)))?;

        if !response.status().is_success() {
            return Err(PlatformError::Storage(format!("Upload failed: status {}", response.status())));
        }

        let blob_response: PutResponse = response.json().await
            .map_err(|e| PlatformError::Storage(format!("Failed to parse response: {}", e)))?;

        Ok(blob_response.url)
    }

    async fn get(&self, key: &str) -> PlatformResult<Vec<u8>> {
        let response = self
            .client
            .get(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to download from Vercel Blob: {}", e)))?;

        if !response.status().is_success() {
            return Err(PlatformError::Storage(format!(
                "Vercel Blob download failed: status {}",
                response.status()
            )));
        }

        let data = response
            .bytes()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to read response: {}", e)))?
            .to_vec();

        Ok(data)
    }

    async fn head(&self, key: &str) -> PlatformResult<ObjectInfo> {
        let response = self
            .client
            .head(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to get metadata: {}", e)))?;

        if !response.status().is_success() {
            return Err(PlatformError::Storage(format!("HEAD request failed: status {}", response.status())));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let size = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok(ObjectInfo {
            key: key.to_string(),
            content_type,
            size,
            last_modified,
            etag,
            metadata: StorageMetadata::new(),
        })
    }

    async fn delete(&self, key: &str) -> PlatformResult<()> {
        let response = self
            .client
            .delete(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| PlatformError::Storage(format!("Failed to delete from Vercel Blob: {}", e)))?;

        if !response.status().is_success() {
            return Err(PlatformError::Storage(format!(
                "Vercel Blob delete failed: status {}",
                response.status()
            )));
        }

        Ok(())
    }

    async fn list(&self, _prefix: &str) -> PlatformResult<Vec<String>> {
        // Vercel Blob doesn't have a direct list API yet, return empty list
        // In production, you'd use the Vercel Blob list API when available
        Ok(Vec::new())
    }

    async fn list_detailed(&self, _options: ListOptions) -> PlatformResult<ListResult> {
        // Vercel Blob doesn't have a direct list API yet
        Ok(ListResult {
            objects: Vec::new(),
            cursor: None,
            has_more: false,
        })
    }

    async fn copy(&self, from: &str, to: &str) -> PlatformResult<()> {
        // Vercel Blob doesn't have server-side copy, so download and re-upload
        let data = self.get(from).await?;
        let content_type = Self::guess_content_type(to);
        self.put(to, data, content_type).await?;
        Ok(())
    }

    fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.base_url, key)
    }

    async fn signed_url(&self, key: &str, _expires_in: u32) -> PlatformResult<String> {
        // Vercel Blob URLs are public by default with token authentication
        // For now, return the public URL
        Ok(self.public_url(key))
    }
}

impl VercelBlobProvider {
    /// Guess content type from file extension
    fn guess_content_type(key: &str) -> &'static str {
        if key.ends_with(".jpg") || key.ends_with(".jpeg") {
            "image/jpeg"
        } else if key.ends_with(".png") {
            "image/png"
        } else if key.ends_with(".gif") {
            "image/gif"
        } else if key.ends_with(".webp") {
            "image/webp"
        } else if key.ends_with(".mp4") {
            "video/mp4"
        } else if key.ends_with(".webm") {
            "video/webm"
        } else if key.ends_with(".mp3") {
            "audio/mpeg"
        } else if key.ends_with(".json") {
            "application/json"
        } else {
            "application/octet-stream"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_guessing() {
        assert_eq!(VercelBlobProvider::guess_content_type("image.jpg"), "image/jpeg");
        assert_eq!(VercelBlobProvider::guess_content_type("photo.png"), "image/png");
        assert_eq!(VercelBlobProvider::guess_content_type("video.mp4"), "video/mp4");
        assert_eq!(VercelBlobProvider::guess_content_type("unknown.xyz"), "application/octet-stream");
    }

    #[tokio::test]
    #[ignore] // Requires actual Vercel Blob token
    async fn test_blob_upload() {
        let token = std::env::var("BLOB_READ_WRITE_TOKEN")
            .expect("BLOB_READ_WRITE_TOKEN must be set for tests");

        let provider = VercelBlobProvider::new(&token);

        // Test upload
        let data = b"Hello, Vercel Blob!";
        let url = provider.put("test.txt", data).await.unwrap();
        assert!(url.starts_with("https://"));

        // Test download
        let downloaded = provider.get("test.txt").await.unwrap();
        assert_eq!(downloaded, data);

        // Test delete
        provider.delete("test.txt").await.unwrap();
    }
}
