use crate::request::string_like_any;
use crate::{
    activitypub_domain, bluesky_appview_xrpc_url, bluesky_post_url, bool_field,
    clamp_cadence_minutes, collapse_whitespace, fetch_activitypub_json,
    fetch_actor_recent_public_posts, fetch_json_with_accept, fixture_rss_response, non_empty_value,
    normalize_discovered_public_post, optional_body_string, public_https_url,
    resolve_activitypub_actor, row_int, row_value_or_fallback_null, row_value_or_null, stable_id,
    string_field, strip_html, truncate_chars, value_string,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use wasm_bindgen::JsValue;
use worker::{D1Type, Env, Fetch, Headers, Request, RequestInit, Result};

const SOURCE_TYPES: &[&str] = &[
    "rss",
    "atom",
    "activitypub",
    "api",
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const REFRESHABLE_SOURCE_TYPES: &[&str] = &[
    "rss",
    "atom",
    "api",
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];
const WATCH_SOURCE_TYPES: &[&str] = &[
    "watch_rss",
    "watch_atom",
    "watch_activitypub_actor",
    "watch_activitypub_object",
    "watch_bluesky_actor",
    "watch_bluesky_post",
];

pub(crate) fn source_policy_json_for_type(body: &Value, source_type: &str) -> String {
    let is_watch = is_watch_source_type(source_type);
    format!(
        "{{\"private_reader_only\":{},\"excerpt_only\":{},\"link_required\":{},\"attribution_required\":{},\"image_allowed\":{},\"full_text_allowed\":{},\"watch\":{},\"public_only\":{},\"no_remote_relationship\":{}}}",
        source_policy_default_true(body, "private_reader_only", "privateReaderOnly") || is_watch,
        source_policy_default_true(body, "excerpt_only", "excerptOnly"),
        source_policy_default_true(body, "link_required", "linkRequired"),
        source_policy_default_true(body, "attribution_required", "attributionRequired"),
        source_policy_bool(body, "image_allowed", "imageAllowed"),
        source_policy_bool(body, "full_text_allowed", "fullTextAllowed"),
        is_watch,
        is_watch,
        is_watch,
    )
}

fn source_policy_default_true(body: &Value, snake: &str, camel: &str) -> bool {
    !matches!(
        body.get(snake).or_else(|| body.get(camel)),
        Some(Value::Bool(false))
    )
}

fn source_policy_bool(body: &Value, snake: &str, camel: &str) -> bool {
    matches!(
        body.get(snake).or_else(|| body.get(camel)),
        Some(Value::Bool(true))
    )
}

pub(crate) fn normalize_source_type(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(':', "_")
}

pub(crate) fn addable_source_types() -> Vec<&'static str> {
    SOURCE_TYPES
        .iter()
        .copied()
        .filter(|value| *value != "activitypub")
        .collect()
}

pub(crate) fn is_addable_source_type(value: &str) -> bool {
    addable_source_types().iter().any(|item| *item == value)
}

pub(crate) fn is_watch_source_type(value: &str) -> bool {
    WATCH_SOURCE_TYPES.iter().any(|item| *item == value)
}

pub(crate) fn is_refreshable_source_type(value: &str) -> bool {
    REFRESHABLE_SOURCE_TYPES.iter().any(|item| *item == value)
}

pub(crate) fn source_type_for_watch_kind(value: &str) -> Option<&'static str> {
    match normalize_source_type(value).as_str() {
        "rss" | "feed" | "watch_rss" => Some("watch_rss"),
        "atom" | "watch_atom" => Some("watch_atom"),
        "activitypub" | "activitypub_actor" | "ap" | "actor" | "watch_activitypub_actor" => {
            Some("watch_activitypub_actor")
        }
        "activitypub_object"
        | "activitypub_post"
        | "ap_object"
        | "ap_post"
        | "watch_activitypub_object" => Some("watch_activitypub_object"),
        "bluesky"
        | "bsky"
        | "atproto"
        | "bluesky_actor"
        | "atproto_actor"
        | "watch_bluesky_actor" => Some("watch_bluesky_actor"),
        "bluesky_post" | "bsky_post" | "atproto_post" | "watch_bluesky_post" => {
            Some("watch_bluesky_post")
        }
        _ => None,
    }
}

pub(crate) fn normalized_source_target(
    source_type: &str,
    body: &Value,
) -> std::result::Result<String, String> {
    let raw = string_like_any(
        body,
        &["url", "target", "uri", "actor", "feed_url", "feedUrl"],
    )
    .unwrap_or_default();
    match source_type {
        "watch_activitypub_actor" => normalized_activitypub_actor_target(&raw),
        "watch_bluesky_actor" => bluesky_actor_target(&raw),
        "watch_bluesky_post" => bluesky_post_uri(&raw),
        "watch_rss" | "watch_atom" | "watch_activitypub_object" => {
            public_https_url(&raw, "watch target")
        }
        _ => public_https_url(&raw, "source url"),
    }
}

fn normalized_activitypub_actor_target(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with('@') && trimmed.trim_start_matches('@').contains('@') {
        return Ok(trimmed.to_string());
    }
    public_https_url(trimmed, "watch target")
}

pub(crate) fn bluesky_actor_target(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        return Err("watch target is required".to_string());
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
            .ok_or_else(|| "Bluesky actor target is invalid".to_string());
    }
    if let Ok(url) = worker::Url::parse(trimmed) {
        if url.host_str() != Some("bsky.app") {
            return Err("Bluesky actor URL must be on bsky.app".to_string());
        }
        let mut parts = url.path().split('/').filter(|part| !part.is_empty());
        if parts.next() == Some("profile") {
            if let Some(actor) = parts.next().filter(|value| !value.trim().is_empty()) {
                return Ok(actor.to_string());
            }
        }
        return Err(
            "Bluesky actor URL must look like https://bsky.app/profile/<handle-or-did>".to_string(),
        );
    }
    if trimmed.contains('.') || trimmed.starts_with("did:") {
        return Ok(trimmed.to_string());
    }
    Err("Bluesky actor target must be a handle, DID, or bsky.app profile URL".to_string())
}

pub(crate) fn bluesky_post_uri(value: &str) -> std::result::Result<String, String> {
    let trimmed = value.trim();
    if trimmed.starts_with("at://") && trimmed.contains("/app.bsky.feed.post/") {
        return Ok(trimmed.to_string());
    }
    let url = worker::Url::parse(trimmed)
        .map_err(|_| "Bluesky post target must be an at:// URI or bsky.app post URL".to_string())?;
    if url.host_str() != Some("bsky.app") {
        return Err("Bluesky post URL must be on bsky.app".to_string());
    }
    let parts: Vec<&str> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 4 && parts[0] == "profile" && parts[2] == "post" {
        return Ok(format!("at://{}/app.bsky.feed.post/{}", parts[1], parts[3]));
    }
    Err(
        "Bluesky post URL must look like https://bsky.app/profile/<handle-or-did>/post/<rkey>"
            .to_string(),
    )
}

pub(crate) fn source_id(source_type: &str, source_url: &str) -> String {
    let digest = Sha256::digest(format!("{source_type}\n{source_url}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("source-{}", &hex[..24])
}

#[derive(Clone)]
pub(crate) struct SourcePolicy {
    private_reader_only: bool,
    excerpt_only: bool,
    link_required: bool,
    attribution_required: bool,
    no_image: bool,
    full_text_allowed: bool,
}

impl SourcePolicy {
    pub(crate) fn default() -> Self {
        Self {
            private_reader_only: true,
            excerpt_only: true,
            link_required: true,
            attribution_required: true,
            no_image: false,
            full_text_allowed: false,
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        serde_json::json!({
            "private_reader_only": self.private_reader_only,
            "excerpt_only": self.excerpt_only,
            "link_required": self.link_required,
            "attribution_required": self.attribution_required,
            "no_image": self.no_image,
            "full_text_allowed": self.full_text_allowed,
        })
    }
}

pub(crate) struct SourceRefreshItem {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) canonical_url: Option<String>,
    pub(crate) external_id: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) published_at: Option<String>,
    pub(crate) excerpt: Option<String>,
    pub(crate) thumbnail_url: Option<String>,
    pub(crate) hash: String,
}

pub(crate) fn source_policy_from_row(row: &Map<String, Value>) -> SourcePolicy {
    let mut policy = SourcePolicy::default();
    let Some(value) = string_field(Some(row), "policy_json") else {
        return policy;
    };
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&value) else {
        return policy;
    };
    if let Some(value) = object.get("private_reader_only").and_then(Value::as_bool) {
        policy.private_reader_only = value;
    }
    if let Some(value) = object.get("excerpt_only").and_then(Value::as_bool) {
        policy.excerpt_only = value;
    }
    if let Some(value) = object.get("link_required").and_then(Value::as_bool) {
        policy.link_required = value;
    }
    if let Some(value) = object.get("attribution_required").and_then(Value::as_bool) {
        policy.attribution_required = value;
    }
    if let Some(value) = object.get("no_image").and_then(Value::as_bool) {
        policy.no_image = value;
    }
    if let Some(value) = object.get("full_text_allowed").and_then(Value::as_bool) {
        policy.full_text_allowed = value;
    }
    policy
}

pub(crate) fn parse_feed_items(
    xml: &str,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> Vec<SourceRefreshItem> {
    let rss_items = xml_blocks(xml, "item");
    if !rss_items.is_empty() {
        return rss_items
            .into_iter()
            .map(|block| normalize_feed_block(&block, source, policy, "rss"))
            .collect();
    }
    xml_blocks(xml, "entry")
        .into_iter()
        .map(|block| normalize_feed_block(&block, source, policy, "atom"))
        .collect()
}

pub(crate) fn parse_api_items(
    body: &str,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let value = serde_json::from_str::<Value>(body).map_err(|error| error.to_string())?;
    let rows = value
        .get("articles")
        .or_else(|| value.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(rows
        .iter()
        .map(|row| normalize_api_item(row, source, policy))
        .collect())
}

pub(crate) async fn watch_activitypub_actor_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let remote = resolve_activitypub_actor(&target).await?;
    let posts = fetch_actor_recent_public_posts(&remote).await;
    Ok(posts
        .iter()
        .filter_map(|post| activitypub_watch_item(source, post, policy))
        .collect())
}

pub(crate) async fn watch_activitypub_object_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let object_url = public_https_url(&target, "watch target")?;
    let object = fetch_activitypub_json(&object_url, "watch object").await?;
    let Some(post) = normalize_discovered_public_post(&object) else {
        return Ok(Vec::new());
    };
    Ok(activitypub_watch_item(source, &post, policy)
        .into_iter()
        .collect())
}

pub(crate) async fn watch_bluesky_actor_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let actor = bluesky_actor_target(&target)?;
    let url = bluesky_appview_xrpc_url(
        "app.bsky.feed.getAuthorFeed",
        &format!(
            "actor={}&limit=50&filter=posts_no_replies",
            urlencoding::encode(&actor)
        ),
    );
    let body = fetch_json_with_accept(&url, "application/json", "bluesky author feed").await?;
    let feed = body
        .get("feed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(feed
        .iter()
        .filter_map(|row| row.get("post").or(Some(row)))
        .filter_map(|post| bluesky_watch_item(source, post, policy))
        .collect())
}

pub(crate) async fn watch_bluesky_post_items(
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> std::result::Result<Vec<SourceRefreshItem>, String> {
    let target =
        string_field(Some(source), "url").ok_or_else(|| "watch target is missing".to_string())?;
    let uri = bluesky_post_uri(&target)?;
    let url = bluesky_appview_xrpc_url(
        "app.bsky.feed.getPostThread",
        &format!("uri={}&depth=1&parentHeight=0", urlencoding::encode(&uri)),
    );
    let body = fetch_json_with_accept(&url, "application/json", "bluesky post thread").await?;
    let mut posts = Vec::new();
    collect_bluesky_thread_posts(body.get("thread"), &mut posts);
    Ok(posts
        .iter()
        .filter_map(|post| bluesky_watch_item(source, post, policy))
        .collect())
}

pub(crate) fn activitypub_watch_item(
    source: &Map<String, Value>,
    post: &Map<String, Value>,
    policy: &SourcePolicy,
) -> Option<SourceRefreshItem> {
    let id = string_field(Some(post), "id")?;
    let canonical_url = string_field(Some(post), "url").or_else(|| Some(id.clone()));
    let title = string_field(Some(post), "name")
        .or_else(|| string_field(Some(post), "summary"))
        .or_else(|| string_field(Some(post), "content"))
        .map(|value| source_title(&strip_html(&value), "ActivityPub public post"))
        .unwrap_or_else(|| "ActivityPub public post".to_string());
    let excerpt = string_field(Some(post), "content")
        .or_else(|| string_field(Some(post), "summary"))
        .and_then(|value| source_excerpt(&value, excerpt_limit(policy)));
    let published_at = normalize_source_date(string_field(Some(post), "published"));
    Some(source_refresh_item(
        source,
        title,
        canonical_url,
        Some(id),
        string_field(Some(post), "actor_id"),
        published_at,
        excerpt,
        None,
    ))
}

pub(crate) fn bluesky_watch_item(
    source: &Map<String, Value>,
    post: &Value,
    policy: &SourcePolicy,
) -> Option<SourceRefreshItem> {
    let object = post.as_object()?;
    let uri = object.get("uri").and_then(optional_body_string)?;
    let author = object.get("author").and_then(Value::as_object);
    let handle = author
        .and_then(|row| row.get("handle"))
        .and_then(optional_body_string);
    let display_name = author
        .and_then(|row| row.get("displayName"))
        .and_then(optional_body_string);
    let author_label = display_name.or_else(|| handle.clone()).or_else(|| {
        author
            .and_then(|row| row.get("did"))
            .and_then(optional_body_string)
    });
    let record = object.get("record").and_then(Value::as_object);
    let text = record
        .and_then(|row| row.get("text"))
        .and_then(optional_body_string)
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
    let published_at = normalize_source_date(
        record
            .and_then(|row| row.get("createdAt"))
            .and_then(optional_body_string)
            .or_else(|| object.get("indexedAt").and_then(optional_body_string)),
    );
    let excerpt = source_excerpt(&text, excerpt_limit(policy));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        bluesky_post_thumbnail(post)
    };
    Some(source_refresh_item(
        source,
        title,
        canonical_url,
        Some(uri),
        author_label,
        published_at,
        excerpt,
        thumbnail_url,
    ))
}

fn collect_bluesky_thread_posts(value: Option<&Value>, posts: &mut Vec<Value>) {
    let Some(Value::Object(object)) = value else {
        return;
    };
    if let Some(post) = object.get("post") {
        posts.push(post.clone());
    }
    if let Some(replies) = object.get("replies").and_then(Value::as_array) {
        for reply in replies {
            collect_bluesky_thread_posts(Some(reply), posts);
        }
    }
}

fn bluesky_post_thumbnail(post: &Value) -> Option<String> {
    let embed = post.get("embed").and_then(Value::as_object)?;
    embed
        .get("images")
        .and_then(Value::as_array)
        .and_then(|images| images.first())
        .and_then(|image| {
            image
                .get("thumb")
                .or_else(|| image.get("fullsize"))
                .and_then(optional_body_string)
        })
        .or_else(|| {
            embed
                .get("external")
                .and_then(Value::as_object)
                .and_then(|external| external.get("thumb"))
                .and_then(optional_body_string)
        })
}

fn source_title(value: &str, fallback: &str) -> String {
    let text = collapse_whitespace(value);
    if text.is_empty() {
        fallback.to_string()
    } else {
        text.chars().take(120).collect()
    }
}

fn normalize_api_item(
    row: &Value,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
) -> SourceRefreshItem {
    let title = value_string(row.get("title"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "(untitled source item)".to_string());
    let canonical_url = value_string(row.get("url").or_else(|| row.get("external_url")));
    let external_id = value_string(row.get("id").or_else(|| row.get("guid")))
        .or_else(|| canonical_url.clone())
        .or_else(|| Some(title.clone()));
    let author = value_string(row.get("author").or_else(|| row.get("byline"))).or_else(|| {
        row.get("source")
            .and_then(|source| source.get("name"))
            .and_then(|value| value_string(Some(value)))
    });
    let published_at = normalize_source_date(value_string(
        row.get("publishedAt")
            .or_else(|| row.get("date_published"))
            .or_else(|| row.get("published_at")),
    ));
    let excerpt = value_string(
        row.get("description")
            .or_else(|| row.get("summary"))
            .or_else(|| row.get("excerpt")),
    )
    .and_then(|value| source_excerpt(&value, excerpt_limit(policy)));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        value_string(row.get("urlToImage").or_else(|| row.get("image")))
    };
    source_refresh_item(
        source,
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        thumbnail_url,
    )
}

fn normalize_feed_block(
    block: &str,
    source: &Map<String, Value>,
    policy: &SourcePolicy,
    kind: &str,
) -> SourceRefreshItem {
    let title =
        xml_text_tag(block, "title").unwrap_or_else(|| "(untitled source item)".to_string());
    let canonical_url = if kind == "atom" {
        xml_attr_tag(block, "link", "href").or_else(|| xml_text_tag(block, "link"))
    } else {
        xml_text_tag(block, "link")
    };
    let external_id = xml_text_tag(block, "guid")
        .or_else(|| xml_text_tag(block, "id"))
        .or_else(|| canonical_url.clone())
        .or_else(|| Some(title.clone()));
    let author = xml_text_tag(block, "author")
        .or_else(|| xml_text_tag(block, "dc:creator"))
        .or_else(|| xml_text_tag(block, "name"));
    let published_at = normalize_source_date(
        xml_text_tag(block, "pubDate")
            .or_else(|| xml_text_tag(block, "published"))
            .or_else(|| xml_text_tag(block, "updated")),
    );
    let excerpt = xml_text_tag(block, "description")
        .or_else(|| xml_text_tag(block, "summary"))
        .and_then(|value| source_excerpt(&value, excerpt_limit(policy)));
    let thumbnail_url = if policy.no_image {
        None
    } else {
        xml_attr_tag(block, "media:thumbnail", "url")
    };
    source_refresh_item(
        source,
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        thumbnail_url,
    )
}

fn source_refresh_item(
    source: &Map<String, Value>,
    title: String,
    canonical_url: Option<String>,
    external_id: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    excerpt: Option<String>,
    thumbnail_url: Option<String>,
) -> SourceRefreshItem {
    let source_id = string_field(Some(source), "id").unwrap_or_default();
    let external_seed = external_id.clone().unwrap_or_default();
    let canonical_seed = canonical_url.clone().unwrap_or_default();
    let seed = format!("{source_id}\n{external_seed}\n{canonical_seed}\n{title}");
    let hash = stable_id(&seed);
    SourceRefreshItem {
        id: format!("src-{}", hash.chars().take(24).collect::<String>()),
        title,
        canonical_url,
        external_id,
        author,
        published_at,
        excerpt,
        thumbnail_url,
        hash,
    }
}

fn xml_blocks(xml: &str, tag: &str) -> Vec<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let close_tag = format!("</{}>", tag.to_ascii_lowercase());
    let mut blocks = Vec::new();
    let mut offset = 0;
    while let Some(open_rel) = lower_xml[offset..].find(&open_prefix) {
        let open = offset + open_rel;
        let Some(open_end_rel) = lower_xml[open..].find('>') else {
            break;
        };
        let content_start = open + open_end_rel + 1;
        let Some(close_rel) = lower_xml[content_start..].find(&close_tag) else {
            break;
        };
        let close = content_start + close_rel;
        blocks.push(xml[content_start..close].to_string());
        offset = close + close_tag.len();
    }
    blocks
}

fn xml_text_tag(xml: &str, tag: &str) -> Option<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let open = lower_xml.find(&open_prefix)?;
    let open_end = open + lower_xml[open..].find('>')?;
    let content_start = open_end + 1;
    let close_tag = format!("</{}>", tag.to_ascii_lowercase());
    let close = content_start + lower_xml[content_start..].find(&close_tag)?;
    let value = strip_xml_tags(&strip_cdata(&xml[content_start..close]));
    let decoded = decode_xml(value.trim());
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

fn xml_attr_tag(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let lower_xml = xml.to_ascii_lowercase();
    let open_prefix = format!("<{}", tag.to_ascii_lowercase());
    let open = lower_xml.find(&open_prefix)?;
    let end = open + lower_xml[open..].find('>')?;
    let raw_attrs = &xml[open..end];
    let lower_attrs = raw_attrs.to_ascii_lowercase();
    let attr_prefix = format!("{}=", attr.to_ascii_lowercase());
    let attr_start = lower_attrs.find(&attr_prefix)? + attr_prefix.len();
    let quote = raw_attrs[attr_start..].chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_start = attr_start + quote.len_utf8();
    let value_end = value_start + raw_attrs[value_start..].find(quote)?;
    Some(decode_xml(&raw_attrs[value_start..value_end])).filter(|value| !value.trim().is_empty())
}

fn strip_cdata(value: &str) -> String {
    value
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
        .unwrap_or(value)
        .to_string()
}

fn strip_xml_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => {
                in_tag = true;
                output.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn source_excerpt(value: &str, max_chars: usize) -> Option<String> {
    let text = collapse_whitespace(&strip_xml_tags(&decode_xml(value)));
    let excerpt: String = text.chars().take(max_chars).collect();
    if excerpt.trim().is_empty() {
        None
    } else {
        Some(excerpt)
    }
}

fn excerpt_limit(policy: &SourcePolicy) -> usize {
    if policy.full_text_allowed && !policy.excerpt_only {
        2000
    } else {
        800
    }
}

fn normalize_source_date(value: Option<String>) -> Option<String> {
    let value = value?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        return Some(value);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&JsValue::from_str(&value));
        let millis = date.get_time();
        if millis.is_nan() {
            None
        } else {
            date.to_iso_string().as_string()
        }
    }
}

pub(crate) async fn owner_source_subscriptions(
    env: &Env,
    limit: i32,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
        FROM source_subscriptions
        WHERE source_type NOT IN (
          'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
          'watch_bluesky_actor', 'watch_bluesky_post'
        )
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

pub(crate) async fn owner_watch_subscriptions(
    env: &Env,
    limit: i32,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
        FROM source_subscriptions
        WHERE source_type IN (
          'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
          'watch_bluesky_actor', 'watch_bluesky_post'
        )
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

pub(crate) async fn owner_source_items(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, external_id, author,
                   published_at, fetched_at, excerpt, content_type, thumbnail_url,
                   rights_policy_json, read, summary, created_at, updated_at
            FROM source_items
            WHERE source_type NOT IN (
              'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
              'watch_bluesky_actor', 'watch_bluesky_post'
            )
            ORDER BY COALESCE(published_at, fetched_at) DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(normalize_source_item).collect())
}

pub(crate) async fn owner_watch_items(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, external_id, author,
                   published_at, fetched_at, excerpt, content_type, thumbnail_url,
                   rights_policy_json, read, summary, created_at, updated_at
            FROM source_items
            WHERE source_type IN (
              'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
              'watch_bluesky_actor', 'watch_bluesky_post'
            )
            ORDER BY COALESCE(published_at, fetched_at) DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(normalize_source_item).collect())
}

pub(crate) async fn owner_add_source(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let source_type = normalize_source_type(
        &string_like_any(body, &["source_type", "sourceType"]).unwrap_or_default(),
    );
    if !is_addable_source_type(&source_type) {
        return Err(format!(
            "source_type must be one of: {}",
            addable_source_types().join(", ")
        ));
    }
    let source_url = normalized_source_target(&source_type, body)?;
    let title = body.get("title").and_then(optional_body_string);
    let cadence_minutes = clamp_cadence_minutes(string_like_any(
        body,
        &["cadence_minutes", "cadenceMinutes"],
    ));
    let api_secret_name = if is_watch_source_type(&source_type) {
        None
    } else {
        string_like_any(body, &["api_secret_name", "apiSecretName"])
            .and_then(|value| optional_body_string(&Value::String(value)))
    };
    let policy_json = source_policy_json_for_type(body, &source_type);

    owner_upsert_source(
        env,
        &source_type,
        &source_url,
        title.as_deref(),
        cadence_minutes,
        api_secret_name.as_deref(),
        &policy_json,
    )
    .await
}

pub(crate) async fn owner_add_watch(
    env: &Env,
    body: &Value,
) -> std::result::Result<Map<String, Value>, String> {
    let watch_kind = string_like_any(
        body,
        &[
            "watch_type",
            "watchType",
            "source_type",
            "sourceType",
            "protocol",
            "kind",
        ],
    )
    .unwrap_or_else(|| "rss".to_string());
    let source_type = source_type_for_watch_kind(&watch_kind)
        .ok_or_else(|| "watch_type must be rss, atom, activitypub_actor, activitypub_object, bluesky_actor, or bluesky_post".to_string())?;
    let source_url = normalized_source_target(source_type, body)?;
    let id = source_id(source_type, &source_url);
    let title = body.get("title").and_then(optional_body_string);
    let cadence_minutes = clamp_cadence_minutes(string_like_any(
        body,
        &["cadence_minutes", "cadenceMinutes"],
    ));
    let policy_json = source_policy_json_for_type(body, source_type);

    owner_upsert_source(
        env,
        source_type,
        &source_url,
        title.as_deref(),
        cadence_minutes,
        None,
        &policy_json,
    )
    .await
    .map(|mut row| {
        row.insert("watch".to_string(), Value::Bool(true));
        row.insert(
            "watch_type".to_string(),
            Value::String(source_type.to_string()),
        );
        row.insert("id".to_string(), Value::String(id));
        row
    })
}

async fn owner_upsert_source(
    env: &Env,
    source_type: &str,
    source_url: &str,
    title: Option<&str>,
    cadence_minutes: i32,
    api_secret_name: Option<&str>,
    policy_json: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let id = source_id(source_type, source_url);
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let id_arg = D1Type::Text(&id);
    let type_arg = D1Type::Text(source_type);
    let url_arg = D1Type::Text(source_url);
    let title_arg = title.map(D1Type::Text).unwrap_or(D1Type::Null);
    let cadence_arg = D1Type::Integer(cadence_minutes);
    let policy_arg = D1Type::Text(policy_json);
    let secret_arg = api_secret_name.map(D1Type::Text).unwrap_or(D1Type::Null);
    db.prepare(
        r#"
        INSERT INTO source_subscriptions (
          id, source_type, url, title, refresh_cadence_minutes, policy_json,
          api_secret_name, status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
          source_type = excluded.source_type,
          url = excluded.url,
          title = excluded.title,
          refresh_cadence_minutes = excluded.refresh_cadence_minutes,
          policy_json = excluded.policy_json,
          api_secret_name = excluded.api_secret_name,
          status = 'active',
          last_error = NULL,
          updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind_refs([
        &id_arg,
        &type_arg,
        &url_arg,
        &title_arg,
        &cadence_arg,
        &policy_arg,
        &secret_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    owner_source_by_id(env, &id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "source add failed".to_string())
}

async fn owner_source_by_id(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE id = ?1
        "#,
    )
    .bind_refs(&id_arg)?
    .first::<Map<String, Value>>(None)
    .await
}

pub(crate) async fn owner_refresh_sources(
    env: &Env,
    id: Option<&str>,
) -> std::result::Result<Value, String> {
    let rows = if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
        match owner_source_by_id(env, id)
            .await
            .map_err(|error| error.to_string())?
        {
            Some(source) => vec![source],
            None => return Err(format!("source not found: {id}")),
        }
    } else {
        owner_active_sources(env)
            .await
            .map_err(|error| error.to_string())?
    };
    refresh_source_rows(env, rows).await
}

pub(crate) async fn owner_refresh_watches(
    env: &Env,
    id: Option<&str>,
) -> std::result::Result<Value, String> {
    let rows = if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
        match owner_source_by_id(env, id)
            .await
            .map_err(|error| error.to_string())?
        {
            Some(source)
                if string_field(Some(&source), "source_type")
                    .map(|source_type| is_watch_source_type(&source_type))
                    .unwrap_or(false) =>
            {
                vec![source]
            }
            Some(_) => return Err(format!("source is not a watch: {id}")),
            None => return Err(format!("watch not found: {id}")),
        }
    } else {
        owner_active_watches(env)
            .await
            .map_err(|error| error.to_string())?
    };
    refresh_source_rows(env, rows).await
}

async fn refresh_source_rows(
    env: &Env,
    rows: Vec<Map<String, Value>>,
) -> std::result::Result<Value, String> {
    let mut items = Vec::new();
    for source in rows {
        let source_id = string_field(Some(&source), "id").unwrap_or_default();
        match refresh_feed_source(env, &source).await {
            Ok(()) => {
                let status = owner_source_by_id(env, &source_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|row| string_field(Some(&row), "status"))
                    .unwrap_or_else(|| "active".to_string());
                items.push(serde_json::json!({ "id": source_id, "ok": true, "status": status }));
            }
            Err(message) => {
                let message = truncate_chars(&message, 500);
                mark_source_error(env, &source_id, &message).await?;
                items.push(serde_json::json!({ "id": source_id, "ok": false, "error": message }));
            }
        }
    }
    let ok = items
        .iter()
        .all(|item| item.get("ok").and_then(Value::as_bool).unwrap_or(false));
    Ok(serde_json::json!({ "ok": ok, "items": items }))
}

pub(crate) async fn refresh_due_sources(env: &Env) -> std::result::Result<(), String> {
    let rows = due_active_sources(env)
        .await
        .map_err(|error| error.to_string())?;
    for source in rows {
        if let Err(message) = refresh_feed_source(env, &source).await {
            if let Some(source_id) = string_field(Some(&source), "id") {
                let message = truncate_chars(&message, 500);
                mark_source_error(env, &source_id, &message).await?;
            }
        }
    }
    Ok(())
}

async fn owner_active_sources(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE status = 'active'
          AND source_type IN (
            'rss', 'atom', 'api', 'watch_rss', 'watch_atom',
            'watch_activitypub_actor', 'watch_activitypub_object',
            'watch_bluesky_actor', 'watch_bluesky_post'
          )
        ORDER BY COALESCE(next_fetch_at, created_at) ASC
        LIMIT 20
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_active_watches(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE status = 'active'
          AND source_type IN (
            'watch_rss', 'watch_atom', 'watch_activitypub_actor', 'watch_activitypub_object',
            'watch_bluesky_actor', 'watch_bluesky_post'
          )
        ORDER BY COALESCE(next_fetch_at, created_at) ASC
        LIMIT 20
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn due_active_sources(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let now = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let now_arg = D1Type::Text(&now);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               etag, last_modified, last_fetched_at, next_fetch_at, last_error, error_count,
               policy_json, api_secret_name, created_at, updated_at
        FROM source_subscriptions
        WHERE status = 'active'
          AND source_type IN (
            'rss', 'atom', 'api', 'watch_rss', 'watch_atom',
            'watch_activitypub_actor', 'watch_activitypub_object',
            'watch_bluesky_actor', 'watch_bluesky_post'
          )
          AND (next_fetch_at IS NULL OR next_fetch_at <= ?1)
        ORDER BY COALESCE(next_fetch_at, created_at) ASC
        LIMIT 20
        "#,
    )
    .bind_refs(&now_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn refresh_feed_source(
    env: &Env,
    source: &Map<String, Value>,
) -> std::result::Result<(), String> {
    let source_id =
        string_field(Some(source), "id").ok_or_else(|| "source id is missing".to_string())?;
    let source_type =
        string_field(Some(source), "source_type").unwrap_or_else(|| "rss".to_string());
    if !is_refreshable_source_type(&source_type) {
        return Err(format!("unsupported source type {source_type}"));
    }
    let url =
        string_field(Some(source), "url").ok_or_else(|| "source url is missing".to_string())?;
    let cadence = row_int(source, "refresh_cadence_minutes")
        .unwrap_or(60)
        .max(5);
    let next_fetch_at = js_sys::Date::new(&JsValue::from_f64(
        js_sys::Date::now() + (cadence as f64) * 60.0 * 1000.0,
    ))
    .to_iso_string()
    .as_string()
    .unwrap_or_default();
    let policy = source_policy_from_row(source);

    if source_type == "watch_activitypub_actor" {
        let items = watch_activitypub_actor_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }
    if source_type == "watch_activitypub_object" {
        let items = watch_activitypub_object_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }
    if source_type == "watch_bluesky_actor" {
        let items = watch_bluesky_actor_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }
    if source_type == "watch_bluesky_post" {
        let items = watch_bluesky_post_items(source, &policy).await?;
        store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
        mark_source_refreshed(env, &source_id, &next_fetch_at, None, None).await?;
        return Ok(());
    }

    let mut response = fetch_source(env, source, &url).await?;
    let status = response.status_code();
    if status == 304 {
        mark_source_refreshed(
            env,
            &source_id,
            &next_fetch_at,
            string_field(Some(source), "etag").as_deref(),
            string_field(Some(source), "last_modified").as_deref(),
        )
        .await?;
        return Ok(());
    }
    if !(200..=299).contains(&status) {
        return Err(format!("source fetch failed with HTTP {status}"));
    }

    let etag = response
        .headers()
        .get("ETag")
        .map_err(|error| error.to_string())?
        .or_else(|| string_field(Some(source), "etag"));
    let last_modified = response
        .headers()
        .get("Last-Modified")
        .map_err(|error| error.to_string())?
        .or_else(|| string_field(Some(source), "last_modified"));
    let body = response.text().await.map_err(|error| error.to_string())?;
    let mut items = if source_type == "api" {
        parse_api_items(&body, source, &policy)?
    } else {
        parse_feed_items(&body, source, &policy)
    };
    items.truncate(50);
    store_source_refresh_items(env, &source_id, &source_type, &policy, items).await?;
    mark_source_refreshed(
        env,
        &source_id,
        &next_fetch_at,
        etag.as_deref(),
        last_modified.as_deref(),
    )
    .await?;
    Ok(())
}

async fn store_source_refresh_items(
    env: &Env,
    source_id: &str,
    source_type: &str,
    policy: &SourcePolicy,
    mut items: Vec<SourceRefreshItem>,
) -> std::result::Result<(), String> {
    items.truncate(50);
    for item in items {
        insert_source_item(env, source_id, source_type, policy, &item).await?;
    }
    Ok(())
}

async fn fetch_source(
    env: &Env,
    env_source: &Map<String, Value>,
    url: &str,
) -> std::result::Result<worker::Response, String> {
    if let Ok(parsed) = worker::Url::parse(url) {
        if parsed.host_str() == Some(activitypub_domain(env).as_str())
            && parsed.path() == "/__dais-fixtures/sources/rss"
        {
            return fixture_rss_response(&parsed).map_err(|error| error.to_string());
        }
    }

    let headers = Headers::new();
    headers
        .set("User-Agent", "dais-source-refresh/1.0")
        .map_err(|error| error.to_string())?;
    if let Some(etag) = string_field(Some(env_source), "etag") {
        headers
            .set("If-None-Match", &etag)
            .map_err(|error| error.to_string())?;
    }
    if let Some(last_modified) = string_field(Some(env_source), "last_modified") {
        headers
            .set("If-Modified-Since", &last_modified)
            .map_err(|error| error.to_string())?;
    }
    let source_type = string_field(Some(env_source), "source_type").unwrap_or_default();
    if !is_watch_source_type(&source_type) {
        if let Some(secret_name) = string_field(Some(env_source), "api_secret_name") {
            if let Ok(secret) = env.var(&secret_name) {
                headers
                    .set("Authorization", &format!("Bearer {}", secret.to_string()))
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init).map_err(|error| error.to_string())?;
    Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())
}

async fn insert_source_item(
    env: &Env,
    source_id: &str,
    source_type: &str,
    policy: &SourcePolicy,
    item: &SourceRefreshItem,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let policy_json =
        serde_json::to_string(&policy.to_value()).map_err(|error| error.to_string())?;
    let metadata_json = serde_json::json!({ "scheduled": true }).to_string();
    let item_id_arg = D1Type::Text(&item.id);
    let source_id_arg = D1Type::Text(source_id);
    let source_type_arg = D1Type::Text(source_type);
    let title_arg = D1Type::Text(&item.title);
    let canonical_arg = item
        .canonical_url
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let external_arg = item
        .external_id
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let author_arg = item
        .author
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let published_arg = item
        .published_at
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let excerpt_arg = item
        .excerpt
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let content_type_arg = D1Type::Text("text/html");
    let hash_arg = D1Type::Text(&item.hash);
    let thumbnail_arg = item
        .thumbnail_url
        .as_deref()
        .map(D1Type::Text)
        .unwrap_or(D1Type::Null);
    let policy_arg = D1Type::Text(&policy_json);
    let metadata_arg = D1Type::Text(&metadata_json);
    db.prepare(
        r#"
        INSERT OR IGNORE INTO source_items (
          id, source_id, source_type, title, canonical_url, external_id, author,
          published_at, excerpt, content_type, hash, thumbnail_url, rights_policy_json,
          raw_metadata_json, fetched_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
    )
    .bind_refs([
        &item_id_arg,
        &source_id_arg,
        &source_type_arg,
        &title_arg,
        &canonical_arg,
        &external_arg,
        &author_arg,
        &published_arg,
        &excerpt_arg,
        &content_type_arg,
        &hash_arg,
        &thumbnail_arg,
        &policy_arg,
        &metadata_arg,
    ])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

async fn mark_source_refreshed(
    env: &Env,
    source_id: &str,
    next_fetch_at: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let next_arg = D1Type::Text(next_fetch_at);
    let etag_arg = etag.map(D1Type::Text).unwrap_or(D1Type::Null);
    let modified_arg = last_modified.map(D1Type::Text).unwrap_or(D1Type::Null);
    let id_arg = D1Type::Text(source_id);
    db.prepare(
        r#"
        UPDATE source_subscriptions
        SET status = 'active',
            last_fetched_at = CURRENT_TIMESTAMP,
            next_fetch_at = ?1,
            etag = COALESCE(?2, etag),
            last_modified = COALESCE(?3, last_modified),
            last_error = NULL,
            error_count = 0,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?4
        "#,
    )
    .bind_refs([&next_arg, &etag_arg, &modified_arg, &id_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn mark_source_error(
    env: &Env,
    source_id: &str,
    message: &str,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let message_arg = D1Type::Text(message);
    let id_arg = D1Type::Text(source_id);
    db.prepare(
        r#"
        UPDATE source_subscriptions
        SET status = 'error',
            last_error = ?1,
            error_count = error_count + 1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?2
        "#,
    )
    .bind_refs([&message_arg, &id_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn owner_delete_source(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("DELETE FROM source_subscriptions WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

pub(crate) async fn owner_delete_watch(env: &Env, id: &str) -> std::result::Result<(), String> {
    let Some(source) = owner_source_by_id(env, id)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Err(format!("watch not found: {id}"));
    };
    let source_type = string_field(Some(&source), "source_type").unwrap_or_default();
    if !is_watch_source_type(&source_type) {
        return Err(format!("source is not a watch: {id}"));
    }
    owner_delete_source(env, id)
        .await
        .map_err(|error| error.to_string())
}

fn normalize_source_item(row: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&row, "id"));
    item.insert("title".to_string(), row_value_or_null(&row, "title"));
    item.insert(
        "source_type".to_string(),
        row_value_or_null(&row, "source_type"),
    );
    item.insert(
        "canonical_url".to_string(),
        row_value_or_null(&row, "canonical_url"),
    );
    item.insert(
        "excerpt".to_string(),
        row_value_or_fallback_null(&row, "excerpt", "summary"),
    );
    item.insert(
        "rights_policy_json".to_string(),
        non_empty_value(&row, "rights_policy_json")
            .unwrap_or_else(|| Value::String("{}".to_string())),
    );
    item.insert(
        "read".to_string(),
        Value::Bool(bool_field(Some(&row), "read")),
    );
    item.insert(
        "source_id".to_string(),
        row_value_or_null(&row, "source_id"),
    );
    item.insert("author".to_string(), row_value_or_null(&row, "author"));
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&row, "published_at"),
    );
    item.insert(
        "fetched_at".to_string(),
        row_value_or_null(&row, "fetched_at"),
    );
    item.insert(
        "thumbnail_url".to_string(),
        row_value_or_null(&row, "thumbnail_url"),
    );
    item
}
