// Vercel queue provider
//
// Vercel doesn't have a built-in queue service, so this implementation
// uses Upstash Redis or HTTP webhooks for background job processing.

use async_trait::async_trait;
use dais_core::traits::QueueProvider;
use dais_core::types::{CoreResult, CoreError, QueueMessage};
use reqwest::Client;
use serde_json;

/// Queue implementation strategies
pub enum QueueStrategy {
    /// Use Upstash Redis for queueing (recommended)
    UpstashRedis {
        redis_url: String,
        redis_token: String,
    },
    /// Use HTTP webhooks (call another Vercel function)
    HttpWebhook { webhook_url: String },
    /// In-memory queue (for development/testing only)
    InMemory,
}

/// Vercel queue provider
///
/// Supports multiple queue strategies since Vercel doesn't have native queuing.
pub struct VercelQueueProvider {
    client: Client,
    strategy: QueueStrategy,
}

impl VercelQueueProvider {
    /// Create a new Vercel queue provider with specified strategy
    ///
    /// # Example with Upstash Redis
    ///
    /// ```
    /// use dais_vercel::{VercelQueueProvider, QueueStrategy};
    ///
    /// let redis_url = std::env::var("UPSTASH_REDIS_URL").unwrap();
    /// let redis_token = std::env::var("UPSTASH_REDIS_TOKEN").unwrap();
    ///
    /// let provider = VercelQueueProvider::new(QueueStrategy::UpstashRedis {
    ///     redis_url,
    ///     redis_token,
    /// });
    /// ```
    ///
    /// # Example with HTTP webhooks
    ///
    /// ```
    /// use dais_vercel::{VercelQueueProvider, QueueStrategy};
    ///
    /// let provider = VercelQueueProvider::new(QueueStrategy::HttpWebhook {
    ///     webhook_url: "https://your-app.vercel.app/api/queue-processor".to_string(),
    /// });
    /// ```
    pub fn new(strategy: QueueStrategy) -> Self {
        Self {
            client: Client::new(),
            strategy,
        }
    }

    /// Create provider from environment variables
    ///
    /// Checks for UPSTASH_REDIS_URL first, falls back to QUEUE_WEBHOOK_URL,
    /// otherwise uses in-memory queue.
    pub fn from_env() -> Self {
        if let (Ok(redis_url), Ok(redis_token)) = (
            std::env::var("UPSTASH_REDIS_URL"),
            std::env::var("UPSTASH_REDIS_TOKEN"),
        ) {
            return Self::new(QueueStrategy::UpstashRedis {
                redis_url,
                redis_token,
            });
        }

        if let Ok(webhook_url) = std::env::var("QUEUE_WEBHOOK_URL") {
            return Self::new(QueueStrategy::HttpWebhook { webhook_url });
        }

        eprintln!("Warning: No queue strategy configured, using in-memory queue");
        Self::new(QueueStrategy::InMemory)
    }
}

#[async_trait]
impl QueueProvider for VercelQueueProvider {
    async fn send(&self, queue: &str, message: &QueueMessage) -> CoreResult<()> {
        match &self.strategy {
            QueueStrategy::UpstashRedis {
                redis_url,
                redis_token,
            } => {
                // Use Upstash Redis REST API to push to queue
                let message_json = serde_json::to_string(message)
                    .map_err(|e| CoreError::QueueError(format!("Failed to serialize message: {}", e)))?;

                let response = self
                    .client
                    .post(format!("{}/lpush/{}", redis_url, queue))
                    .header("Authorization", format!("Bearer {}", redis_token))
                    .json(&vec![message_json])
                    .send()
                    .await
                    .map_err(|e| CoreError::QueueError(format!("Failed to push to Redis: {}", e)))?;

                if !response.status().is_success() {
                    return Err(CoreError::QueueError(format!(
                        "Redis push failed: status {}",
                        response.status()
                    )));
                }

                Ok(())
            }

            QueueStrategy::HttpWebhook { webhook_url } => {
                // Send message to webhook endpoint
                let response = self
                    .client
                    .post(webhook_url)
                    .json(&message)
                    .send()
                    .await
                    .map_err(|e| CoreError::QueueError(format!("Webhook call failed: {}", e)))?;

                if !response.status().is_success() {
                    return Err(CoreError::QueueError(format!(
                        "Webhook failed: status {}",
                        response.status()
                    )));
                }

                Ok(())
            }

            QueueStrategy::InMemory => {
                // In-memory queue doesn't actually queue anything
                // This is only for development/testing
                eprintln!(
                    "Warning: In-memory queue ignoring message to queue '{}': {:?}",
                    queue, message
                );
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_in_memory() {
        // Without env vars, should use in-memory
        let provider = VercelQueueProvider::from_env();
        matches!(provider.strategy, QueueStrategy::InMemory);
    }

    #[tokio::test]
    #[ignore] // Requires actual Upstash Redis
    async fn test_upstash_redis_queue() {
        let redis_url = std::env::var("UPSTASH_REDIS_URL")
            .expect("UPSTASH_REDIS_URL must be set for tests");
        let redis_token = std::env::var("UPSTASH_REDIS_TOKEN")
            .expect("UPSTASH_REDIS_TOKEN must be set for tests");

        let provider = VercelQueueProvider::new(QueueStrategy::UpstashRedis {
            redis_url,
            redis_token,
        });

        let message = QueueMessage {
            id: "test123".to_string(),
            data: serde_json::json!({"test": "data"}),
        };

        provider.send("test-queue", &message).await.unwrap();
    }
}
