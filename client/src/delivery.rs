use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct DeliveryProcessReport {
    pub delivery_id: String,
    pub success: bool,
    pub retryable: bool,
    pub retry_count: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeliveryEnqueueReport {
    pub delivery_id: String,
    pub enqueued: bool,
    pub status: Option<String>,
}

#[derive(Serialize)]
struct DeliveryProcessRequest<'a> {
    delivery_id: &'a str,
}

#[derive(Serialize)]
struct DeliveryEnqueueRequest<'a> {
    delivery_id: &'a str,
}

pub async fn enqueue_delivery(base_url: &str, delivery_id: &str) -> Result<DeliveryEnqueueReport> {
    let url = format!(
        "{}/admin/deliveries/enqueue",
        base_url.trim_end_matches('/')
    );

    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .json(&DeliveryEnqueueRequest { delivery_id })
        .send()
        .await
        .context("delivery enqueue request failed")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("could not read delivery enqueue response")?;

    if !status.is_success() {
        return Err(anyhow!("delivery enqueue returned HTTP {status}: {body}"));
    }

    serde_json::from_str(&body).context("could not decode delivery enqueue response")
}

pub async fn process_delivery(
    base_url: &str,
    admin_token: Option<&str>,
    delivery_id: &str,
) -> Result<DeliveryProcessReport> {
    let admin_token = admin_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("DELIVERY_ADMIN_TOKEN is required to process delivery jobs"))?;
    let url = format!(
        "{}/admin/deliveries/process",
        base_url.trim_end_matches('/')
    );

    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Dais-Admin-Token", admin_token)
        .json(&DeliveryProcessRequest { delivery_id })
        .send()
        .await
        .context("delivery worker request failed")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("could not read delivery worker response")?;

    if !status.is_success() {
        return Err(anyhow!("delivery worker returned HTTP {status}: {body}"));
    }

    serde_json::from_str(&body).context("could not decode delivery worker response")
}
