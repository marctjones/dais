use anyhow::{anyhow, Context, Result};
use feed_rs::model::{Entry, Feed};
use feed_rs::parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli::SourceAddArgs;
use crate::d1::{D1Client, D1SourceSubscription, SourceItemInsert, SourceSubscriptionInsert};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourcePolicy {
    pub private_reader_only: bool,
    pub excerpt_only: bool,
    pub link_required: bool,
    pub attribution_required: bool,
    pub no_image: bool,
    pub full_text_allowed: bool,
}

#[derive(Clone, Debug)]
pub struct RefreshReport {
    pub source_id: String,
    pub url: String,
    pub fetched: bool,
    pub parsed_items: usize,
    pub stored_items: usize,
}

impl SourcePolicy {
    pub fn from_args(args: &SourceAddArgs) -> Self {
        Self {
            private_reader_only: args.private_reader_only,
            excerpt_only: args.excerpt_only,
            link_required: args.link_required,
            attribution_required: args.attribution_required,
            no_image: args.no_image,
            full_text_allowed: args.full_text_allowed,
        }
    }
}

pub async fn add_source(db: &D1Client, source_type: &str, args: SourceAddArgs) -> Result<String> {
    let id = source_id(source_type, &args.url);
    let policy_json = serde_json::to_string(&SourcePolicy::from_args(&args))?;
    db.add_source_subscription(SourceSubscriptionInsert {
        id: &id,
        source_type,
        url: &args.url,
        title: args.title.as_deref(),
        cadence_minutes: args.cadence_minutes.unwrap_or(60),
        policy_json: &policy_json,
        api_secret_name: args.api_secret_name.as_deref(),
    })
    .await?;
    Ok(id)
}

pub async fn refresh_source(
    db: &D1Client,
    source: &D1SourceSubscription,
    dry_run: bool,
) -> Result<RefreshReport> {
    if source.source_type != "rss" && source.source_type != "atom" && source.source_type != "api" {
        return Err(anyhow!("unsupported source type {}", source.source_type));
    }

    let client = reqwest::Client::builder()
        .user_agent("dais-source-refresh/1.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut request = client.get(&source.url);
    if let Some(etag) = source.etag.as_deref().filter(|value| !value.is_empty()) {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = source
        .last_modified
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
    }
    if let Some(secret_name) = source
        .api_secret_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if let Ok(token) = std::env::var(secret_name) {
            request = request.bearer_auth(token);
        }
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("could not fetch source {}", source.url))?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        if !dry_run {
            db.mark_source_refreshed(
                &source.id,
                source.refresh_cadence_minutes,
                source.etag.as_deref(),
                source.last_modified.as_deref(),
            )
            .await?;
        }
        return Ok(RefreshReport {
            source_id: source.id.clone(),
            url: source.url.clone(),
            fetched: false,
            parsed_items: 0,
            stored_items: 0,
        });
    }
    let status = response.status();
    if !status.is_success() {
        let message = format!("source fetch failed with HTTP {status}");
        if !dry_run {
            db.mark_source_error(&source.id, &message).await?;
        }
        return Err(anyhow!(message));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response.bytes().await?;
    let policy: SourcePolicy = serde_json::from_str(&source.policy_json).unwrap_or(SourcePolicy {
        private_reader_only: true,
        excerpt_only: true,
        link_required: true,
        attribution_required: true,
        no_image: false,
        full_text_allowed: false,
    });
    let items = if source.source_type == "api" {
        api_items(source, &bytes, &policy)?
    } else {
        let feed = parser::parse(&bytes[..]).context("could not parse feed")?;
        feed_items(source, &feed, &policy)?
    };
    let parsed_items = items.len();
    let mut stored_items = 0;
    if !dry_run {
        for item in &items {
            db.upsert_source_item(SourceItemInsert {
                id: &item.id,
                source_id: &item.source_id,
                source_type: &item.source_type,
                title: &item.title,
                canonical_url: item.canonical_url.as_deref(),
                external_id: item.external_id.as_deref(),
                author: item.author.as_deref(),
                published_at: item.published_at.as_deref(),
                excerpt: item.excerpt.as_deref(),
                content_type: item.content_type.as_deref(),
                hash: &item.hash,
                thumbnail_url: item.thumbnail_url.as_deref(),
                rights_policy_json: &item.rights_policy_json,
                raw_metadata_json: item.raw_metadata_json.as_deref(),
            })
            .await?;
            stored_items += 1;
        }
        db.mark_source_refreshed(
            &source.id,
            source.refresh_cadence_minutes,
            etag.as_deref(),
            last_modified.as_deref(),
        )
        .await?;
    }

    Ok(RefreshReport {
        source_id: source.id.clone(),
        url: source.url.clone(),
        fetched: true,
        parsed_items,
        stored_items,
    })
}

struct NormalizedItem {
    id: String,
    source_id: String,
    source_type: String,
    title: String,
    canonical_url: Option<String>,
    external_id: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    excerpt: Option<String>,
    content_type: Option<String>,
    hash: String,
    thumbnail_url: Option<String>,
    rights_policy_json: String,
    raw_metadata_json: Option<String>,
}

fn feed_items(
    source: &D1SourceSubscription,
    feed: &Feed,
    policy: &SourcePolicy,
) -> Result<Vec<NormalizedItem>> {
    feed.entries
        .iter()
        .map(|entry| normalize_entry(source, feed, entry, policy))
        .collect()
}

fn normalize_entry(
    source: &D1SourceSubscription,
    feed: &Feed,
    entry: &Entry,
    policy: &SourcePolicy,
) -> Result<NormalizedItem> {
    let title = entry
        .title
        .as_ref()
        .map(|text| text.content.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "(untitled source item)".to_string());
    let canonical_url = entry.links.first().map(|link| link.href.clone());
    let external_id = if entry.id.trim().is_empty() {
        canonical_url.clone()
    } else {
        Some(entry.id.clone())
    };
    let author = entry
        .authors
        .first()
        .map(|person| person.name.clone())
        .or_else(|| feed.authors.first().map(|person| person.name.clone()));
    let published_at = entry
        .published
        .or(entry.updated)
        .map(|date| date.to_rfc3339());
    let excerpt = entry
        .summary
        .as_ref()
        .map(|text| text.content.clone())
        .or_else(|| {
            if policy.full_text_allowed && !policy.excerpt_only {
                entry
                    .content
                    .as_ref()
                    .and_then(|content| content.body.clone())
            } else {
                None
            }
        })
        .map(|value| excerpt_text(&value, 800));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        entry
            .media
            .iter()
            .flat_map(|media| media.thumbnails.iter())
            .find_map(|thumbnail| Some(thumbnail.image.uri.clone()))
    };
    let seed = format!(
        "{}\n{}\n{}\n{}",
        source.id,
        external_id.as_deref().unwrap_or(""),
        canonical_url.as_deref().unwrap_or(""),
        title
    );
    let hash = hex_hash(&seed);
    let id = format!("src-{}", &hash[..24]);
    let raw_metadata_json = serde_json::to_string(&serde_json::json!({
        "feedTitle": feed.title.as_ref().map(|text| text.content.clone()),
        "policy": policy,
    }))?;

    Ok(NormalizedItem {
        id,
        source_id: source.id.clone(),
        source_type: source.source_type.clone(),
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        content_type: Some("text/html".to_string()),
        hash,
        thumbnail_url,
        rights_policy_json: serde_json::to_string(policy)?,
        raw_metadata_json: Some(raw_metadata_json),
    })
}

fn api_items(
    source: &D1SourceSubscription,
    bytes: &[u8],
    policy: &SourcePolicy,
) -> Result<Vec<NormalizedItem>> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).context("could not parse API JSON")?;
    let rows = value
        .get("articles")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.get("items").and_then(serde_json::Value::as_array))
        .ok_or_else(|| anyhow!("API response must contain articles[] or items[]"))?;
    rows.iter()
        .map(|row| normalize_api_item(source, row, policy))
        .collect()
}

fn normalize_api_item(
    source: &D1SourceSubscription,
    row: &serde_json::Value,
    policy: &SourcePolicy,
) -> Result<NormalizedItem> {
    let title = row
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("(untitled source item)")
        .to_string();
    let canonical_url = row
        .get("url")
        .or_else(|| row.get("external_url"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let external_id = row
        .get("id")
        .or_else(|| row.get("guid"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| canonical_url.clone());
    let author = row
        .get("author")
        .or_else(|| row.get("byline"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            row.get("source")
                .and_then(|source| source.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        });
    let published_at = row
        .get("publishedAt")
        .or_else(|| row.get("date_published"))
        .or_else(|| row.get("published_at"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let excerpt = row
        .get("description")
        .or_else(|| row.get("summary"))
        .or_else(|| row.get("excerpt"))
        .and_then(serde_json::Value::as_str)
        .map(|value| {
            excerpt_text(
                value,
                if policy.full_text_allowed && !policy.excerpt_only {
                    2000
                } else {
                    800
                },
            )
        });
    let thumbnail_url = if policy.no_image {
        None
    } else {
        row.get("urlToImage")
            .or_else(|| row.get("image"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
    };
    let seed = format!(
        "{}\n{}\n{}\n{}",
        source.id,
        external_id.as_deref().unwrap_or(""),
        canonical_url.as_deref().unwrap_or(""),
        title
    );
    let hash = hex_hash(&seed);
    let id = format!("src-{}", &hash[..24]);
    Ok(NormalizedItem {
        id,
        source_id: source.id.clone(),
        source_type: source.source_type.clone(),
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        content_type: Some("application/json".to_string()),
        hash,
        thumbnail_url,
        rights_policy_json: serde_json::to_string(policy)?,
        raw_metadata_json: Some(serde_json::to_string(&serde_json::json!({
            "api": true,
            "policy": policy
        }))?),
    })
}

fn source_id(source_type: &str, url: &str) -> String {
    format!(
        "source-{}",
        &hex_hash(&format!("{source_type}\n{url}"))[..24]
    )
}

fn hex_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn excerpt_text(value: &str, max_chars: usize) -> String {
    let stripped = value
        .replace('\n', " ")
        .replace('\r', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    stripped.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rss_fixture_into_normalized_items() {
        let bytes = br#"<?xml version="1.0"?>
        <rss version="2.0">
          <channel>
            <title>Example News</title>
            <link>https://example.com</link>
            <item>
              <title>First item</title>
              <link>https://example.com/first</link>
              <guid>first-guid</guid>
              <description>Short allowed excerpt.</description>
              <pubDate>Thu, 11 Jun 2026 12:00:00 GMT</pubDate>
              <author>newsroom@example.com</author>
            </item>
          </channel>
        </rss>"#;
        let feed = parser::parse(&bytes[..]).unwrap();
        let source = D1SourceSubscription {
            id: "source-test".to_string(),
            source_type: "rss".to_string(),
            url: "https://example.com/rss".to_string(),
            title: None,
            homepage_url: None,
            status: "active".to_string(),
            refresh_cadence_minutes: 60,
            last_fetched_at: None,
            next_fetch_at: None,
            etag: None,
            last_modified: None,
            last_error: None,
            error_count: 0,
            policy_json: "{}".to_string(),
            api_secret_name: None,
            created_at: None,
            updated_at: None,
        };
        let policy = SourcePolicy {
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            no_image: false,
            full_text_allowed: false,
        };
        let items = feed_items(&source, &feed, &policy).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "First item");
        assert_eq!(
            items[0].canonical_url.as_deref(),
            Some("https://example.com/first")
        );
        assert_eq!(items[0].excerpt.as_deref(), Some("Short allowed excerpt."));
    }

    #[test]
    fn parses_news_api_style_fixture_into_normalized_items() {
        let bytes = br#"{
          "articles": [{
            "title": "Official API item",
            "url": "https://example.com/api-item",
            "author": "Example Newsroom",
            "publishedAt": "2026-06-11T12:00:00Z",
            "description": "Terms-allowed API excerpt.",
            "urlToImage": "https://example.com/image.jpg"
          }]
        }"#;
        let source = D1SourceSubscription {
            id: "source-api".to_string(),
            source_type: "api".to_string(),
            url: "https://api.example.com/articles".to_string(),
            title: None,
            homepage_url: None,
            status: "active".to_string(),
            refresh_cadence_minutes: 60,
            last_fetched_at: None,
            next_fetch_at: None,
            etag: None,
            last_modified: None,
            last_error: None,
            error_count: 0,
            policy_json: "{}".to_string(),
            api_secret_name: Some("EXAMPLE_API_TOKEN".to_string()),
            created_at: None,
            updated_at: None,
        };
        let policy = SourcePolicy {
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            no_image: false,
            full_text_allowed: false,
        };
        let items = api_items(&source, bytes, &policy).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Official API item");
        assert_eq!(
            items[0].canonical_url.as_deref(),
            Some("https://example.com/api-item")
        );
        assert_eq!(items[0].author.as_deref(), Some("Example Newsroom"));
    }
}
