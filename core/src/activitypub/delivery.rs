/// ActivityPub delivery (HTTP POST to inboxes)
///
/// TODO: Migrate delivery logic from workers/delivery-queue/src/lib.rs

use crate::{CoreResult, CoreError};
use crate::traits::HttpProvider;
use super::types::Activity;

pub async fn deliver_activity(
    http: &dyn HttpProvider,
    inbox_url: &str,
    activity: &Activity,
    actor: &str,
    private_key: &str,
) -> CoreResult<()> {
    // TODO: Implement delivery logic
    // - Sign HTTP request
    // - POST to inbox
    // - Handle retries
    Err(CoreError::Internal("Not implemented".to_string()))
}
