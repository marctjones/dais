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

    /// Cloudflare Queues support delayed delivery in the platform, but the
    /// worker-rs Queue binding used here does not expose that option. Keep this
    /// explicit so callers do not assume requested delays are honored.
    pub fn supports_delayed_send() -> bool {
        false
    }

    /// The Queues binding does not expose queue depth. Returning a guessed zero
    /// would hide operational risk, so depth remains an explicit unsupported
    /// operation.
    pub fn supports_depth() -> bool {
        false
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
        Err(unsupported_delayed_send_error(delay_seconds))
    }

    async fn depth(&self) -> PlatformResult<u64> {
        Err(unsupported_depth_error())
    }
}

fn unsupported_delayed_send_error(delay_seconds: u32) -> PlatformError {
    PlatformError::Queue(format!(
        "Cloudflare delayed queue send is unavailable in the worker-rs Queue binding; refusing to ignore requested {delay_seconds}s delay"
    ))
}

fn unsupported_depth_error() -> PlatformError {
    PlatformError::Queue(
        "Cloudflare queue depth is unavailable in the worker-rs Queue binding; refusing to report unknown depth as zero"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_capabilities_are_explicit() {
        assert!(!CloudflareQueueProvider::supports_delayed_send());
        assert!(!CloudflareQueueProvider::supports_depth());
    }

    #[test]
    fn unsupported_delayed_send_reports_requested_delay() {
        let error = unsupported_delayed_send_error(30).to_string();

        assert!(error.contains("delayed queue send is unavailable"));
        assert!(error.contains("30s delay"));
    }

    #[test]
    fn unsupported_depth_refuses_fake_zero() {
        let error = unsupported_depth_error().to_string();

        assert!(error.contains("queue depth is unavailable"));
        assert!(error.contains("unknown depth as zero"));
    }
}
