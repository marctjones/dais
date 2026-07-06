use crate::request::string_like_any;
use crate::{
    bluesky_appview_xrpc_url, bluesky_post_url, collapse_whitespace, fetch_activitypub_json,
    fetch_actor_recent_public_posts, fetch_json_with_accept, normalize_discovered_public_post,
    optional_body_string, public_https_url, resolve_activitypub_actor, stable_id, string_field,
    strip_html, value_string,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

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
