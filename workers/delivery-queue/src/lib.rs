use worker::*;
use serde::{Deserialize, Serialize};

mod activitypub;
mod atproto;

#[derive(Debug, Deserialize, Serialize)]
struct DeliveryJob {
    delivery_id: String,
    post_id: String,
    protocol: String,  // "activitypub" or "atproto"
    target_type: String,  // "inbox" or "did"
    target_url: String,
    actor_url: String,
    activity_json: String,  // Serialized activity
    retry_count: u32,
}

#[event(queue)]
pub async fn main(message_batch: MessageBatch<DeliveryJob>, env: Env, _ctx: Context) -> Result<()> {
    console_log!("Processing batch of {} delivery jobs", message_batch.messages().len());

    for message in message_batch.messages() {
        let job = message.body();
        console_log!("Processing delivery: {} (protocol: {}, retry: {})",
            job.delivery_id, job.protocol, job.retry_count);

        let result = match job.protocol.as_str() {
            "activitypub" => {
                activitypub::deliver_activity(
                    &job.target_url,
                    &job.actor_url,
                    &job.activity_json,
                    &env
                ).await
            },
            "atproto" => {
                atproto::deliver_to_bluesky(
                    &job.activity_json,
                    &env
                ).await
            },
            _ => {
                console_log!("Unknown protocol: {}", job.protocol);
                message.ack();
                continue;
            }
        };

        let db = env.d1("DB")?;

        match result {
            Ok(_) => {
                console_log!("✓ Delivery successful: {}", job.delivery_id);

                // Update delivery status to delivered
                let now = chrono::Utc::now().to_rfc3339();
                let update_query = format!(
                    "UPDATE deliveries SET status = 'delivered', delivered_at = '{}', last_attempt_at = '{}' WHERE id = '{}'",
                    now, now, job.delivery_id
                );

                let statement = db.prepare(&update_query);
                if let Err(e) = statement.run().await {
                    console_log!("Warning: Failed to update delivery status: {}", e);
                }

                message.ack();
            },
            Err(e) => {
                console_log!("✗ Delivery failed: {} - {}", job.delivery_id, e);

                // Update delivery status
                let now = chrono::Utc::now().to_rfc3339();
                let error_msg = e.to_string().replace("'", "''");
                let new_status = if job.retry_count >= 3 { "failed" } else { "retry" };

                let update_query = format!(
                    "UPDATE deliveries SET status = '{}', retry_count = {}, last_attempt_at = '{}', error_message = '{}' WHERE id = '{}'",
                    new_status, job.retry_count + 1, now, error_msg, job.delivery_id
                );

                let statement = db.prepare(&update_query);
                if let Err(e) = statement.run().await {
                    console_log!("Warning: Failed to update delivery status: {}", e);
                }

                // Retry or ack based on retry count
                if job.retry_count >= 3 {
                    console_log!("Max retries reached for {}, moving to DLQ", job.delivery_id);
                    message.ack();  // This will move to dead letter queue
                } else {
                    message.retry();
                }
            }
        }
    }

    Ok(())
}
