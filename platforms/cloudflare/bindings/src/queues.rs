use async_trait::async_trait;
/// Cloudflare Queues provider implementation
///
/// Implements the QueueProvider trait using Cloudflare Queues
use dais_core::traits::{PlatformError, PlatformResult, QueueProvider};
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

    async fn send_delayed(&self, _message: &str, delay_seconds: u32) -> PlatformResult<()> {
        Err(PlatformError::Queue(format!(
            "Cloudflare delayed queue send is not implemented in this binding; refusing to ignore requested {delay_seconds}s delay"
        )))
    }

    async fn depth(&self) -> PlatformResult<u64> {
        Err(PlatformError::Queue(
            "Cloudflare queue depth is unavailable in this binding; refusing to report unknown depth as zero"
                .to_string(),
        ))
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
