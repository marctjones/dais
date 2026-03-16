/// Cloudflare Queues provider implementation
///
/// Implements the QueueProvider trait using Cloudflare Queues

use dais_core::traits::{PlatformError, PlatformResult, QueueProvider};
use async_trait::async_trait;
use worker::Queue;

pub struct CloudflareQueueProvider {
    queue: Queue,
}

impl CloudflareQueueProvider {
    /// Create a new CloudflareQueueProvider from a Cloudflare Queue binding
    pub fn new(queue: Queue) -> Self {
        Self { queue }
    }
}

#[async_trait(?Send)]
impl QueueProvider for CloudflareQueueProvider {
    async fn send(&self, message: &str) -> PlatformResult<()> {
        self.queue
            .send(message)
            .await
            .map_err(|e| PlatformError::Queue(format!("Queue send failed: {:?}", e)))?;

        Ok(())
    }

    async fn send_batch(&self, messages: Vec<String>) -> PlatformResult<()> {
        // Cloudflare Queues batch API
        let batch: Vec<_> = messages.iter().map(|s| s.as_str()).collect();

        self.queue
            .send_batch(batch)
            .await
            .map_err(|e| PlatformError::Queue(format!("Queue batch send failed: {:?}", e)))?;

        Ok(())
    }

    async fn send_delayed(&self, message: &str, _delay_seconds: u32) -> PlatformResult<()> {
        // TODO: Add delay support when available in worker-rs
        // For now, just send immediately
        self.send(message).await
    }

    async fn depth(&self) -> PlatformResult<u64> {
        // Cloudflare Queues doesn't expose queue depth in the current API
        // Return 0 for now
        // TODO: Implement when API supports it
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_provider() {
        // Can't test with real Queue in unit tests
        // In integration tests with wrangler dev, we can test real queue operations
    }
}
