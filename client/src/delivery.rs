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

#[derive(Clone, Debug, Deserialize)]
pub struct FollowerAcceptReport {
    pub follower_actor_id: String,
    pub accepted: bool,
    pub inbox: String,
}

#[derive(Serialize)]
struct DeliveryProcessRequest<'a> {
    delivery_id: &'a str,
}

#[derive(Serialize)]
struct DeliveryEnqueueRequest<'a> {
    delivery_id: &'a str,
}

#[derive(Serialize)]
struct FollowerAcceptRequest<'a> {
    actor_id: &'a str,
    follower_actor_id: &'a str,
}

pub async fn send_follower_accept(
    base_url: &str,
    actor_id: &str,
    follower_actor_id: &str,
) -> Result<FollowerAcceptReport> {
    let url = format!("{}/admin/followers/accept", base_url.trim_end_matches('/'));

    let response = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .json(&FollowerAcceptRequest {
            actor_id,
            follower_actor_id,
        })
        .send()
        .await
        .context("follower accept request failed")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("could not read follower accept response")?;

    if !status.is_success() {
        return Err(anyhow!("follower accept returned HTTP {status}: {body}"));
    }

    serde_json::from_str(&body).context("could not decode follower accept response")
}

pub async fn enqueue_delivery(base_url: &str, delivery_id: &str) -> Result<DeliveryEnqueueReport> {
    let client = reqwest::Client::new();
    let request = DeliveryEnqueueRequest { delivery_id };
    let mut last_error = None;
    for url in delivery_endpoint_candidates(base_url, "enqueue") {
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .with_context(|| format!("delivery enqueue request failed for {url}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("could not read delivery enqueue response from {url}"))?;

        if status.is_success() {
            return serde_json::from_str(&body)
                .context("could not decode delivery enqueue response");
        }
        let error = anyhow!("delivery enqueue returned HTTP {status} from {url}: {body}");
        if !should_try_next_delivery_endpoint(status) {
            return Err(error);
        }
        last_error = Some(error);
    }
    Err(last_error.unwrap_or_else(|| anyhow!("no delivery enqueue endpoint candidates")))
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
    let client = reqwest::Client::new();
    let request = DeliveryProcessRequest { delivery_id };
    let mut last_error = None;
    for url in delivery_endpoint_candidates(base_url, "process") {
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Dais-Admin-Token", admin_token)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("delivery worker request failed for {url}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("could not read delivery worker response from {url}"))?;

        if status.is_success() {
            return serde_json::from_str(&body)
                .context("could not decode delivery worker response");
        }
        let error = anyhow!("delivery worker returned HTTP {status} from {url}: {body}");
        if !should_try_next_delivery_endpoint(status) {
            return Err(error);
        }
        last_error = Some(error);
    }

    Err(last_error.unwrap_or_else(|| anyhow!("no delivery process endpoint candidates")))
}

fn delivery_endpoint_candidates(base_url: &str, action: &str) -> Vec<String> {
    let base_url = base_url.trim_end_matches('/');
    let worker_url = format!("{base_url}/deliveries/{action}");
    let router_url = format!("{base_url}/admin/deliveries/{action}");
    if base_url.contains("delivery-queue") {
        vec![worker_url, router_url]
    } else {
        vec![router_url, worker_url]
    }
}

fn should_try_next_delivery_endpoint(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::NOT_IMPLEMENTED
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_worker_base_prefers_worker_path() {
        assert_eq!(
            delivery_endpoint_candidates(
                "https://delivery-queue-production.marc-t-jones.workers.dev",
                "enqueue"
            ),
            vec![
                "https://delivery-queue-production.marc-t-jones.workers.dev/deliveries/enqueue",
                "https://delivery-queue-production.marc-t-jones.workers.dev/admin/deliveries/enqueue",
            ]
        );
    }

    #[test]
    fn social_base_prefers_router_admin_path() {
        assert_eq!(
            delivery_endpoint_candidates("https://social.dais.social/", "process"),
            vec![
                "https://social.dais.social/admin/deliveries/process",
                "https://social.dais.social/deliveries/process",
            ]
        );
    }
}
