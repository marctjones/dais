// Vercel Blob storage provider
//
// This provider uses Vercel Blob API for object storage (images, videos, etc.)
// Vercel Blob is S3-compatible and provides global CDN delivery.

use async_trait::async_trait;
use dais_core::traits::StorageProvider;
use dais_core::types::{CoreResult, CoreError};
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
struct PutRequest {
    pathname: String,
    #[serde(rename = "contentType")]
    content_type: Option<String>,
}

#[derive(Deserialize)]
struct PutResponse {
    url: String,
    #[serde(rename = "downloadUrl")]
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

#[async_trait]
impl StorageProvider for VercelBlobProvider {
    async fn put(&self, key: &str, data: &[u8]) -> CoreResult<String> {
        // Determine content type from file extension
        let content_type = Self::guess_content_type(key);

        // Upload to Vercel Blob
        let response = self
            .client
            .put(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| CoreError::StorageError(format!("Failed to upload to Vercel Blob: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(CoreError::StorageError(format!(
                "Vercel Blob upload failed: {}",
                error_text
            )));
        }

        let blob_response: PutResponse = response
            .json()
            .await
            .map_err(|e| CoreError::StorageError(format!("Failed to parse response: {}", e)))?;

        Ok(blob_response.url)
    }

    async fn get(&self, key: &str) -> CoreResult<Vec<u8>> {
        // Download from Vercel Blob
        let response = self
            .client
            .get(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CoreError::StorageError(format!("Failed to download from Vercel Blob: {}", e)))?;

        if !response.status().is_success() {
            return Err(CoreError::StorageError(format!(
                "Vercel Blob download failed: status {}",
                response.status()
            )));
        }

        let data = response
            .bytes()
            .await
            .map_err(|e| CoreError::StorageError(format!("Failed to read response: {}", e)))?
            .to_vec();

        Ok(data)
    }

    async fn delete(&self, key: &str) -> CoreResult<()> {
        // Delete from Vercel Blob
        let response = self
            .client
            .delete(format!("{}/{}", self.base_url, key))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CoreError::StorageError(format!("Failed to delete from Vercel Blob: {}", e)))?;

        if !response.status().is_success() {
            return Err(CoreError::StorageError(format!(
                "Vercel Blob delete failed: status {}",
                response.status()
            )));
        }

        Ok(())
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
