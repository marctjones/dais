use anyhow::{anyhow, Result};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Serialize;
use serde_json::Value;

use crate::cli::DoctorArgs;

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub target: String,
    pub actor: String,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorCheck {
    pub id: &'static str,
    pub status: DoctorStatus,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DoctorStatus {
    Pass,
    Fail,
    Info,
}

impl DoctorReport {
    pub fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == DoctorStatus::Fail)
    }
}

pub async fn run(args: &DoctorArgs) -> DoctorReport {
    let config = DoctorConfig::from_args(args);
    let client = reqwest::Client::new();
    let mut checks = Vec::new();

    checks.push(check_webfinger(&client, &config).await);
    checks.push(check_actor(&client, &config).await);
    checks.push(check_outbox(&client, &config).await);
    checks.push(check_public_post(&client, &config).await);
    checks.push(check_private_post_denied(&client, &config).await);
    checks.push(check_collections(&client, &config).await);
    checks.push(check_inbox_unsigned_rejected(&client, &config).await);
    checks.push(check_pds(&client, &config).await);
    checks.push(DoctorCheck {
        id: "SIGNED-FIXTURE",
        status: DoctorStatus::Info,
        detail:
            "valid signed inbox and authorized-fetch fixtures require remote actor key material"
                .to_string(),
    });

    DoctorReport {
        target: config.social_base_url,
        actor: config.actor_url,
        checks,
    }
}

pub fn print_report(report: &DoctorReport) {
    println!("dais doctor");
    println!("Target: {}", report.target);
    println!("Actor: {}", report.actor);
    println!();

    for check in &report.checks {
        let status = match check.status {
            DoctorStatus::Pass => "PASS",
            DoctorStatus::Fail => "FAIL",
            DoctorStatus::Info => "INFO",
        };
        println!("{status:<5} {:<24} {}", check.id, check.detail);
    }

    let pass = report
        .checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Pass)
        .count();
    let fail = report
        .checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Fail)
        .count();
    let info = report
        .checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Info)
        .count();
    println!();
    println!("Summary: PASS={pass} FAIL={fail} INFO={info}");
}

struct DoctorConfig {
    social_base_url: String,
    pds_base_url: String,
    username: String,
    acct_domain: String,
    actor_path: String,
    actor_url: String,
    public_post: String,
    private_post: String,
}

impl DoctorConfig {
    fn from_args(args: &DoctorArgs) -> Self {
        let social_base_url = trim_url(&args.social_base_url);
        let pds_base_url = trim_url(&args.pds_base_url);
        let actor_path = format!("/users/{}", args.username);
        let actor_url = format!("{social_base_url}{actor_path}");
        Self {
            social_base_url,
            pds_base_url,
            username: args.username.clone(),
            acct_domain: args.acct_domain.clone(),
            actor_path,
            actor_url,
            public_post: args.public_post.clone(),
            private_post: args.private_post.clone(),
        }
    }

    fn social_url(&self, path_or_url: &str) -> String {
        if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
            path_or_url.to_string()
        } else {
            format!("{}{}", self.social_base_url, path_or_url)
        }
    }
}

async fn check_webfinger(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    let resource = format!("acct:{}@{}", config.username, config.acct_domain);
    let url = format!(
        "{}/.well-known/webfinger?resource={}",
        config.social_base_url, resource
    );
    match get_json(client, &url, "application/jrd+json").await {
        Ok(json) => {
            let has_self = json
                .get("links")
                .and_then(Value::as_array)
                .map(|links| {
                    links.iter().any(|link| {
                        link.get("rel").and_then(Value::as_str) == Some("self")
                            && link.get("type").and_then(Value::as_str)
                                == Some("application/activity+json")
                            && link.get("href").and_then(Value::as_str)
                                == Some(config.actor_url.as_str())
                    })
                })
                .unwrap_or(false);
            if has_self {
                pass("WEBFINGER", resource)
            } else {
                fail("WEBFINGER", "missing ActivityPub self link")
            }
        }
        Err(error) => fail("WEBFINGER", error.to_string()),
    }
}

async fn check_actor(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    let url = config.social_url(&config.actor_path);
    match get_json(client, &url, "application/activity+json").await {
        Ok(json) => {
            let actor_ok = json.get("type").and_then(Value::as_str) == Some("Person")
                && json.get("id").and_then(Value::as_str) == Some(config.actor_url.as_str())
                && json.get("inbox").and_then(Value::as_str).is_some()
                && json.get("outbox").and_then(Value::as_str).is_some()
                && json
                    .get("publicKey")
                    .and_then(|key| key.get("publicKeyPem"))
                    .and_then(Value::as_str)
                    .map(|pem| pem.contains("BEGIN PUBLIC KEY"))
                    .unwrap_or(false);
            if actor_ok {
                pass("ACTOR", config.actor_url.clone())
            } else {
                fail("ACTOR", "actor document is missing required fields")
            }
        }
        Err(error) => fail("ACTOR", error.to_string()),
    }
}

async fn check_outbox(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    let url = config.social_url(&format!("{}/outbox", config.actor_path));
    match get_json(client, &url, "application/activity+json").await {
        Ok(json) => {
            let items = json.get("orderedItems").and_then(Value::as_array);
            let leaked = items
                .map(|items| {
                    items.iter().any(|item| {
                        item.pointer("/object/encryptedMessage").is_some()
                            || item
                                .pointer("/object/content")
                                .and_then(Value::as_str)
                                .map(|content| content.contains("End-to-end encrypted message"))
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if json.get("type").and_then(Value::as_str) != Some("OrderedCollection") {
                fail("OUTBOX", "outbox is not an OrderedCollection")
            } else if leaked {
                fail(
                    "OUTBOX",
                    "anonymous outbox leaks encrypted fallback content",
                )
            } else {
                let count = json
                    .get("totalItems")
                    .and_then(Value::as_u64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                pass("OUTBOX", format!("{count} public items"))
            }
        }
        Err(error) => fail("OUTBOX", error.to_string()),
    }
}

async fn check_public_post(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    let url = config.social_url(&config.public_post);
    match get_json(client, &url, "application/activity+json").await {
        Ok(json) => {
            if json.get("type").and_then(Value::as_str) == Some("Note") {
                pass("PUBLIC-POST", url)
            } else {
                fail("PUBLIC-POST", "known public object is not a Note")
            }
        }
        Err(error) => fail("PUBLIC-POST", error.to_string()),
    }
}

async fn check_private_post_denied(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    let url = config.social_url(&config.private_post);
    let html = client.get(&url).header(ACCEPT, "text/html").send().await;
    let json = client
        .get(&url)
        .header(ACCEPT, "application/activity+json")
        .send()
        .await;
    match (html, json) {
        (Ok(html), Ok(json)) if html.status().as_u16() == 404 && json.status().as_u16() == 404 => {
            pass("PRIVATE-DENIAL", "anonymous HTML/JSON denied")
        }
        (Ok(html), Ok(json)) => fail(
            "PRIVATE-DENIAL",
            format!("expected 404/404, got {}/{}", html.status(), json.status()),
        ),
        (Err(error), _) | (_, Err(error)) => fail("PRIVATE-DENIAL", error.to_string()),
    }
}

async fn check_collections(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    for name in ["followers", "following"] {
        let url = config.social_url(&format!("{}/{}", config.actor_path, name));
        match get_json(client, &url, "application/activity+json").await {
            Ok(json) => {
                let collection_ok =
                    json.get("type").and_then(Value::as_str) == Some("OrderedCollection");
                let exposes_items = json
                    .get("orderedItems")
                    .and_then(Value::as_array)
                    .map(|items| !items.is_empty())
                    .unwrap_or(false);
                if !collection_ok || exposes_items {
                    return fail(
                        "COLLECTIONS",
                        format!("{name} has invalid shape or exposes actor IDs"),
                    );
                }
            }
            Err(error) => return fail("COLLECTIONS", error.to_string()),
        }
    }
    pass("COLLECTIONS", "anonymous social graph pages are summaries")
}

async fn check_inbox_unsigned_rejected(
    client: &reqwest::Client,
    config: &DoctorConfig,
) -> DoctorCheck {
    let url = config.social_url(&format!("{}/inbox", config.actor_path));
    let preflight = client.request(reqwest::Method::OPTIONS, &url).send().await;
    let post = client
        .post(&url)
        .header(CONTENT_TYPE, "application/activity+json")
        .body(r#"{"type":"Create"}"#)
        .send()
        .await;
    match (preflight, post) {
        (Ok(preflight), Ok(post))
            if preflight.status().is_success() && post.status().as_u16() == 401 =>
        {
            pass("INBOX", "preflight ok; unsigned POST rejected")
        }
        (Ok(preflight), Ok(post)) => fail(
            "INBOX",
            format!(
                "expected preflight 2xx and POST 401, got {}/{}",
                preflight.status(),
                post.status()
            ),
        ),
        (Err(error), _) | (_, Err(error)) => fail("INBOX", error.to_string()),
    }
}

async fn check_pds(client: &reqwest::Client, config: &DoctorConfig) -> DoctorCheck {
    let url = format!(
        "{}/xrpc/com.atproto.server.describeServer",
        config.pds_base_url
    );
    match get_json(client, &url, "application/json").await {
        Ok(json) => {
            if json.get("availableUserDomains").is_some() {
                pass("PDS", "describeServer available")
            } else {
                fail(
                    "PDS",
                    "describeServer response missing availableUserDomains",
                )
            }
        }
        Err(error) => fail("PDS", error.to_string()),
    }
}

async fn get_json(client: &reqwest::Client, url: &str, accept: &str) -> Result<Value> {
    let response = client.get(url).header(ACCEPT, accept).send().await?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !status.is_success() {
        return Err(anyhow!("expected 2xx from {url}, got {status}"));
    }
    if !content_type
        .to_ascii_lowercase()
        .contains(&accept.to_ascii_lowercase())
    {
        return Err(anyhow!("expected {accept}, got {content_type}"));
    }
    Ok(response.json().await?)
}

fn pass(id: &'static str, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        id,
        status: DoctorStatus::Pass,
        detail: detail.into(),
    }
}

fn fail(id: &'static str, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        id,
        status: DoctorStatus::Fail,
        detail: detail.into(),
    }
}

fn trim_url(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}
