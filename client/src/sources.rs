use anyhow::{anyhow, Context, Result};
use feed_rs::model::{Entry, Feed};
use feed_rs::parser;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
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
    if !is_refreshable_source_type(&source.source_type) {
        return Err(anyhow!("unsupported source type {}", source.source_type));
    }

    let client = reqwest::Client::builder()
        .user_agent("dais-source-refresh/1.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let policy: SourcePolicy = serde_json::from_str(&source.policy_json).unwrap_or(SourcePolicy {
        private_reader_only: true,
        excerpt_only: true,
        link_required: true,
        attribution_required: true,
        no_image: false,
        full_text_allowed: false,
    });

    if source.source_type == "watch_activitypub_actor" {
        let items = activitypub_actor_watch_items(&client, source, &policy).await?;
        return store_refresh_items(db, source, dry_run, items, None, None).await;
    }
    if source.source_type == "watch_activitypub_object" {
        let items = activitypub_object_watch_items(&client, source, &policy).await?;
        return store_refresh_items(db, source, dry_run, items, None, None).await;
    }
    if source.source_type == "watch_bluesky_actor" {
        let items = bluesky_actor_watch_items(&client, source, &policy).await?;
        return store_refresh_items(db, source, dry_run, items, None, None).await;
    }
    if source.source_type == "watch_bluesky_post" {
        let items = bluesky_post_watch_items(&client, source, &policy).await?;
        return store_refresh_items(db, source, dry_run, items, None, None).await;
    }

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
    if !is_watch_source_type(&source.source_type) {
        if let Some(secret_name) = source
            .api_secret_name
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            if let Ok(token) = std::env::var(secret_name) {
                request = request.bearer_auth(token);
            }
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
    let items = if source.source_type == "api" {
        api_items(source, &bytes, &policy)?
    } else {
        let feed = parser::parse(&bytes[..]).context("could not parse feed")?;
        feed_items(source, &feed, &policy)?
    };
    store_refresh_items(db, source, dry_run, items, etag, last_modified).await
}

async fn store_refresh_items(
    db: &D1Client,
    source: &D1SourceSubscription,
    dry_run: bool,
    items: Vec<NormalizedItem>,
    etag: Option<String>,
    last_modified: Option<String>,
) -> Result<RefreshReport> {
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

async fn activitypub_actor_watch_items(
    client: &reqwest::Client,
    source: &D1SourceSubscription,
    policy: &SourcePolicy,
) -> Result<Vec<NormalizedItem>> {
    let actor_url = activitypub_actor_url_for_target(client, &source.url).await?;
    let actor = fetch_json(client, &actor_url, "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json").await?;
    let outbox_url = actor
        .get("outbox")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("ActivityPub actor does not expose outbox"))?;
    let outbox_url = public_https_target(outbox_url, "actor outbox")?;
    let outbox = fetch_json(client, &outbox_url, "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json").await?;
    let page = match outbox.get("first").and_then(activitypub_link_value) {
        Some(first) => fetch_json(client, &public_https_target(&first, "outbox first page")?, "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json")
            .await
            .unwrap_or_else(|_| outbox.clone()),
        None => outbox,
    };
    Ok(page
        .get("orderedItems")
        .or_else(|| page.get("items"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(normalize_activitypub_public_post)
        .filter_map(|post| activitypub_watch_item(source, &post, policy))
        .collect())
}

async fn activitypub_object_watch_items(
    client: &reqwest::Client,
    source: &D1SourceSubscription,
    policy: &SourcePolicy,
) -> Result<Vec<NormalizedItem>> {
    let object_url = public_https_target(&source.url, "watch target")?;
    let object = fetch_json(client, &object_url, "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json").await?;
    Ok(normalize_activitypub_public_post(&object)
        .and_then(|post| activitypub_watch_item(source, &post, policy))
        .into_iter()
        .collect())
}

async fn bluesky_actor_watch_items(
    client: &reqwest::Client,
    source: &D1SourceSubscription,
    policy: &SourcePolicy,
) -> Result<Vec<NormalizedItem>> {
    let actor = bluesky_actor_target(&source.url)?;
    let mut url =
        reqwest::Url::parse("https://public.api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed")?;
    url.query_pairs_mut()
        .append_pair("actor", &actor)
        .append_pair("limit", "50")
        .append_pair("filter", "posts_no_replies");
    let body = fetch_json(client, url.as_str(), "application/json").await?;
    Ok(body
        .get("feed")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|row| row.get("post").or(Some(row)))
        .filter_map(|post| bluesky_watch_item(source, post, policy))
        .collect())
}

async fn bluesky_post_watch_items(
    client: &reqwest::Client,
    source: &D1SourceSubscription,
    policy: &SourcePolicy,
) -> Result<Vec<NormalizedItem>> {
    let uri = bluesky_post_uri(&source.url)?;
    let mut url =
        reqwest::Url::parse("https://public.api.bsky.app/xrpc/app.bsky.feed.getPostThread")?;
    url.query_pairs_mut()
        .append_pair("uri", &uri)
        .append_pair("depth", "1")
        .append_pair("parentHeight", "0");
    let body = fetch_json(client, url.as_str(), "application/json").await?;
    let mut posts = Vec::new();
    collect_bluesky_thread_posts(body.get("thread"), &mut posts);
    Ok(posts
        .iter()
        .filter_map(|post| bluesky_watch_item(source, post, policy))
        .collect())
}

async fn fetch_json(client: &reqwest::Client, url: &str, accept: &str) -> Result<Value> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, accept)
        .send()
        .await
        .with_context(|| format!("could not fetch {url}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("public fetch failed with HTTP {status}"));
    }
    response.json::<Value>().await.map_err(Into::into)
}

async fn activitypub_actor_url_for_target(
    client: &reqwest::Client,
    target: &str,
) -> Result<String> {
    let trimmed = target.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return public_https_target(trimmed, "ActivityPub actor");
    }
    let handle = trimmed.trim_start_matches('@');
    let domain = handle
        .rsplit('@')
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow!("ActivityPub actor watch target must be an actor URL or @user@domain handle")
        })?;
    public_https_target(&format!("https://{domain}/"), "ActivityPub domain")?;
    let mut url = reqwest::Url::parse(&format!("https://{domain}/.well-known/webfinger"))?;
    url.query_pairs_mut()
        .append_pair("resource", &format!("acct:{handle}"));
    let jrd = fetch_json(
        client,
        url.as_str(),
        "application/jrd+json, application/json",
    )
    .await?;
    let links = jrd
        .get("links")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("no ActivityPub self link found for {target}"))?;
    for link in links {
        let rel = link
            .get("rel")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let link_type = link
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let href = link
            .get("href")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if rel == "self" && link_type.contains("activity+json") && !href.is_empty() {
            return public_https_target(href, "ActivityPub actor link");
        }
    }
    Err(anyhow!("no ActivityPub self link found for {target}"))
}

fn normalize_activitypub_public_post(item: &Value) -> Option<Map<String, Value>> {
    let object = if item.get("type").and_then(serde_json::Value::as_str) == Some("Create") {
        item.get("object").unwrap_or(item)
    } else {
        item
    };
    let object_type = object.get("type").and_then(serde_json::Value::as_str)?;
    if !matches!(
        object_type,
        "Note" | "Article" | "Image" | "Video" | "Document" | "Event" | "Question"
    ) {
        return None;
    }
    let mut recipients = Vec::new();
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(item.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    collect_recipients(item.get("cc"), &mut recipients);
    if !recipients
        .iter()
        .any(|value| value == "https://www.w3.org/ns/activitystreams#Public")
    {
        return None;
    }
    let mut post = Map::new();
    post.insert(
        "id".to_string(),
        object
            .get("id")
            .or_else(|| item.get("id"))
            .and_then(value_to_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert("type".to_string(), Value::String(object_type.to_string()));
    post.insert(
        "actor_id".to_string(),
        public_post_actor_id(item, object)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "url".to_string(),
        object
            .get("url")
            .or_else(|| item.get("url"))
            .and_then(value_to_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "name".to_string(),
        object
            .get("name")
            .and_then(value_to_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "summary".to_string(),
        object
            .get("summary")
            .and_then(value_to_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "content".to_string(),
        object
            .get("content")
            .or_else(|| object.get("summary"))
            .or_else(|| object.get("name"))
            .and_then(value_to_string)
            .map(|value| Value::String(excerpt_text(&strip_html(&value), 800)))
            .unwrap_or_else(|| Value::String(String::new())),
    );
    post.insert(
        "published".to_string(),
        object
            .get("published")
            .or_else(|| item.get("published"))
            .and_then(value_to_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    Some(post)
}

fn activitypub_watch_item(
    source: &D1SourceSubscription,
    post: &Map<String, Value>,
    policy: &SourcePolicy,
) -> Option<NormalizedItem> {
    let id = map_string(post, "id")?;
    let canonical_url = map_string(post, "url").or_else(|| Some(id.clone()));
    let title = map_string(post, "name")
        .or_else(|| map_string(post, "summary"))
        .or_else(|| map_string(post, "content"))
        .map(|value| source_title(&strip_html(&value), "ActivityPub public post"))
        .unwrap_or_else(|| "ActivityPub public post".to_string());
    let excerpt = map_string(post, "content")
        .or_else(|| map_string(post, "summary"))
        .map(|value| excerpt_text(&value, excerpt_limit(policy)));
    normalized_watch_item(
        source,
        title,
        canonical_url,
        Some(id),
        map_string(post, "actor_id"),
        map_string(post, "published"),
        excerpt,
        None,
        "application/activity+json",
        policy,
    )
}

fn bluesky_watch_item(
    source: &D1SourceSubscription,
    post: &Value,
    policy: &SourcePolicy,
) -> Option<NormalizedItem> {
    let uri = post.get("uri").and_then(value_to_string)?;
    let author = post.get("author").and_then(serde_json::Value::as_object);
    let handle = author
        .and_then(|row| row.get("handle"))
        .and_then(value_to_string);
    let author_label = author
        .and_then(|row| row.get("displayName"))
        .and_then(value_to_string)
        .or_else(|| handle.clone())
        .or_else(|| {
            author
                .and_then(|row| row.get("did"))
                .and_then(value_to_string)
        });
    let record = post.get("record").and_then(serde_json::Value::as_object);
    let text = record
        .and_then(|row| row.get("text"))
        .and_then(value_to_string)
        .unwrap_or_default();
    let title = if text.trim().is_empty() {
        author_label
            .as_ref()
            .map(|author| format!("Bluesky public post by {author}"))
            .unwrap_or_else(|| "Bluesky public post".to_string())
    } else {
        source_title(&text, "Bluesky public post")
    };
    let canonical_url = bluesky_post_url(&uri, handle.as_deref()).or_else(|| Some(uri.clone()));
    let published_at = record
        .and_then(|row| row.get("createdAt"))
        .and_then(value_to_string)
        .or_else(|| post.get("indexedAt").and_then(value_to_string));
    let excerpt = (!text.trim().is_empty()).then(|| excerpt_text(&text, excerpt_limit(policy)));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        bluesky_post_thumbnail(post)
    };
    normalized_watch_item(
        source,
        title,
        canonical_url,
        Some(uri),
        author_label,
        published_at,
        excerpt,
        thumbnail_url,
        "application/json",
        policy,
    )
}

fn normalized_watch_item(
    source: &D1SourceSubscription,
    title: String,
    canonical_url: Option<String>,
    external_id: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    excerpt: Option<String>,
    thumbnail_url: Option<String>,
    content_type: &str,
    policy: &SourcePolicy,
) -> Option<NormalizedItem> {
    let seed = format!(
        "{}\n{}\n{}\n{}",
        source.id,
        external_id.as_deref().unwrap_or(""),
        canonical_url.as_deref().unwrap_or(""),
        title
    );
    let hash = hex_hash(&seed);
    Some(NormalizedItem {
        id: format!("src-{}", &hash[..24]),
        source_id: source.id.clone(),
        source_type: source.source_type.clone(),
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        content_type: Some(content_type.to_string()),
        hash,
        thumbnail_url,
        rights_policy_json: serde_json::to_string(policy).ok()?,
        raw_metadata_json: Some(
            serde_json::to_string(&serde_json::json!({
                "watch": true,
                "publicOnly": true,
                "noRemoteRelationship": true,
                "policy": policy
            }))
            .ok()?,
        ),
    })
}

fn collect_bluesky_thread_posts(value: Option<&Value>, posts: &mut Vec<Value>) {
    let Some(Value::Object(object)) = value else {
        return;
    };
    if let Some(post) = object.get("post") {
        posts.push(post.clone());
    }
    if let Some(replies) = object.get("replies").and_then(serde_json::Value::as_array) {
        for reply in replies {
            collect_bluesky_thread_posts(Some(reply), posts);
        }
    }
}

fn collect_recipients(value: Option<&Value>, recipients: &mut Vec<String>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = value_to_string(item) {
                    recipients.push(text);
                }
            }
        }
        Some(value) => {
            if let Some(text) = value_to_string(value) {
                recipients.push(text);
            }
        }
        None => {}
    }
}

fn public_post_actor_id(item: &Value, object: &Value) -> Option<String> {
    let actor = object
        .get("attributedTo")
        .or_else(|| object.get("actor"))
        .or_else(|| item.get("actor"))
        .or_else(|| item.get("attributedTo"))?;
    match actor {
        Value::String(text) => Some(text.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Array(items) => items.iter().find_map(value_to_string),
        _ => None,
    }
}

fn activitypub_link_value(value: &Value) -> Option<String> {
    value.as_str().map(ToOwned::to_owned).or_else(|| {
        value
            .as_object()
            .and_then(|object| object.get("id"))
            .and_then(value_to_string)
    })
}

fn bluesky_post_thumbnail(post: &Value) -> Option<String> {
    let embed = post.get("embed").and_then(serde_json::Value::as_object)?;
    embed
        .get("images")
        .and_then(serde_json::Value::as_array)
        .and_then(|images| images.first())
        .and_then(|image| {
            image
                .get("thumb")
                .or_else(|| image.get("fullsize"))
                .and_then(value_to_string)
        })
        .or_else(|| {
            embed
                .get("external")
                .and_then(serde_json::Value::as_object)
                .and_then(|external| external.get("thumb"))
                .and_then(value_to_string)
        })
}

fn bluesky_actor_target(value: &str) -> Result<String> {
    let trimmed = value.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        return Err(anyhow!("watch target is required"));
    }
    if trimmed.starts_with("did:") {
        return Ok(trimmed.to_string());
    }
    if trimmed.starts_with("at://") {
        return trimmed
            .trim_start_matches("at://")
            .split('/')
            .next()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Bluesky actor target is invalid"));
    }
    if let Ok(url) = reqwest::Url::parse(trimmed) {
        if url.host_str() != Some("bsky.app") {
            return Err(anyhow!("Bluesky actor URL must be on bsky.app"));
        }
        let mut parts = url.path().split('/').filter(|part| !part.is_empty());
        if parts.next() == Some("profile") {
            if let Some(actor) = parts.next().filter(|value| !value.trim().is_empty()) {
                return Ok(actor.to_string());
            }
        }
        return Err(anyhow!(
            "Bluesky actor URL must look like https://bsky.app/profile/<handle-or-did>"
        ));
    }
    if trimmed.contains('.') {
        return Ok(trimmed.to_string());
    }
    Err(anyhow!(
        "Bluesky actor target must be a handle, DID, or bsky.app profile URL"
    ))
}

fn bluesky_post_uri(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.starts_with("at://") && trimmed.contains("/app.bsky.feed.post/") {
        return Ok(trimmed.to_string());
    }
    let url = reqwest::Url::parse(trimmed)
        .map_err(|_| anyhow!("Bluesky post target must be an at:// URI or bsky.app post URL"))?;
    if url.host_str() != Some("bsky.app") {
        return Err(anyhow!("Bluesky post URL must be on bsky.app"));
    }
    let parts: Vec<&str> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 4 && parts[0] == "profile" && parts[2] == "post" {
        return Ok(format!("at://{}/app.bsky.feed.post/{}", parts[1], parts[3]));
    }
    Err(anyhow!(
        "Bluesky post URL must look like https://bsky.app/profile/<handle-or-did>/post/<rkey>"
    ))
}

fn bluesky_post_url(uri: &str, handle: Option<&str>) -> Option<String> {
    let handle = handle?.trim();
    let rkey = uri.rsplit('/').next()?.trim();
    if handle.is_empty() || rkey.is_empty() {
        return None;
    }
    Some(format!("https://bsky.app/profile/{handle}/post/{rkey}"))
}

fn public_https_target(value: &str, field: &str) -> Result<String> {
    let url =
        reqwest::Url::parse(value).map_err(|_| anyhow!("{field} must be an absolute https URL"))?;
    if url.scheme() != "https" {
        return Err(anyhow!("{field} must be an absolute https URL"));
    }
    Ok(value.to_string())
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn map_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(value_to_string)
}

fn source_title(value: &str, fallback: &str) -> String {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        fallback.to_string()
    } else {
        text.chars().take(120).collect()
    }
}

fn excerpt_limit(policy: &SourcePolicy) -> usize {
    if policy.full_text_allowed && !policy.excerpt_only {
        2000
    } else {
        800
    }
}

fn strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    let mut previous_space = false;
    for ch in value.chars() {
        match ch {
            '<' => {
                in_tag = true;
                if !previous_space && !output.is_empty() {
                    output.push(' ');
                    previous_space = true;
                }
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            _ if ch.is_whitespace() => {
                if !previous_space && !output.is_empty() {
                    output.push(' ');
                    previous_space = true;
                }
            }
            _ => {
                output.push(ch);
                previous_space = false;
            }
        }
    }
    output.trim().to_string()
}

fn is_refreshable_source_type(value: &str) -> bool {
    matches!(
        value,
        "rss"
            | "atom"
            | "api"
            | "watch_rss"
            | "watch_atom"
            | "watch_activitypub_actor"
            | "watch_activitypub_object"
            | "watch_bluesky_actor"
            | "watch_bluesky_post"
    )
}

fn is_watch_source_type(value: &str) -> bool {
    matches!(
        value,
        "watch_rss"
            | "watch_atom"
            | "watch_activitypub_actor"
            | "watch_activitypub_object"
            | "watch_bluesky_actor"
            | "watch_bluesky_post"
    )
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

    #[test]
    fn normalizes_bluesky_watch_targets() {
        assert_eq!(
            bluesky_actor_target("https://bsky.app/profile/nasa.gov").unwrap(),
            "nasa.gov"
        );
        assert_eq!(bluesky_actor_target("@nasa.gov").unwrap(), "nasa.gov");
        assert_eq!(
            bluesky_post_uri("https://bsky.app/profile/nasa.gov/post/3abc").unwrap(),
            "at://nasa.gov/app.bsky.feed.post/3abc"
        );
    }

    #[test]
    fn normalizes_activitypub_public_watch_item() {
        let activity = serde_json::json!({
            "type": "Create",
            "id": "https://example.com/create/1",
            "actor": "https://example.com/users/alice",
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "object": {
                "type": "Note",
                "id": "https://example.com/posts/1",
                "attributedTo": "https://example.com/users/alice",
                "to": ["https://www.w3.org/ns/activitystreams#Public"],
                "content": "<p>Hello public world</p>",
                "published": "2026-06-18T12:00:00Z",
                "url": "https://example.com/@alice/1"
            }
        });
        let post = normalize_activitypub_public_post(&activity).unwrap();
        let source = D1SourceSubscription {
            id: "source-watch".to_string(),
            source_type: "watch_activitypub_actor".to_string(),
            url: "https://example.com/users/alice".to_string(),
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
        let item = activitypub_watch_item(&source, &post, &policy).unwrap();
        assert_eq!(
            item.external_id.as_deref(),
            Some("https://example.com/posts/1")
        );
        assert_eq!(
            item.author.as_deref(),
            Some("https://example.com/users/alice")
        );
        assert_eq!(item.excerpt.as_deref(), Some("Hello public world"));
    }

    #[test]
    fn normalizes_bluesky_public_watch_item() {
        let post = serde_json::json!({
            "uri": "at://did:plc:alice/app.bsky.feed.post/3abc",
            "author": {
                "did": "did:plc:alice",
                "handle": "alice.example",
                "displayName": "Alice"
            },
            "record": {
                "$type": "app.bsky.feed.post",
                "text": "A public Bluesky update",
                "createdAt": "2026-06-18T12:00:00Z"
            }
        });
        let source = D1SourceSubscription {
            id: "source-bsky".to_string(),
            source_type: "watch_bluesky_actor".to_string(),
            url: "alice.example".to_string(),
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
        let item = bluesky_watch_item(&source, &post, &policy).unwrap();
        assert_eq!(
            item.canonical_url.as_deref(),
            Some("https://bsky.app/profile/alice.example/post/3abc")
        );
        assert_eq!(item.author.as_deref(), Some("Alice"));
        assert_eq!(item.excerpt.as_deref(), Some("A public Bluesky update"));
    }
}
