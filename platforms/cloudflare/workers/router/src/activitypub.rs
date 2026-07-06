use crate::fixtures::{fixture_public_key, fixture_url_with_public_key};
use crate::public_https_url;
use crate::{
    escape_html, integer_field, mastodon_mentions, mastodon_status_content, mastodon_tags,
    media_type_for_filename, optional_body_string, parse_attachment_array, row_value_or_null,
    string_field, strip_html, value_string, LocalActor, OwnerProfile, RemoteActor,
    PUBLIC_COLLECTION,
};
use dais_core::activitypub::sign_request;
use serde_json::{Map, Value};
use std::collections::HashMap;
use worker::{Fetch, Headers, Request, RequestInit, Result};

pub(crate) fn activitypub_actor_profile_html(
    profile: &OwnerProfile,
    posts: &[Map<String, Value>],
) -> String {
    let display_name = profile
        .display_name
        .clone()
        .unwrap_or_else(|| profile.username.clone());
    let summary = profile.summary.clone().unwrap_or_default();
    let posts_html = if posts.is_empty() {
        "<p>No public posts yet.</p>".to_string()
    } else {
        posts
            .iter()
            .map(activitypub_actor_post_html)
            .collect::<Vec<_>>()
            .join("")
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;max-width:760px;margin:40px auto;padding:0 20px;line-height:1.5;color:#111827}}a{{color:#0f766e}}article{{border-top:1px solid #d1d5db;padding:18px 0}}time{{color:#6b7280;font-size:.9rem}}.summary{{color:#374151}}</style></head><body><header><h1>{}</h1><p class=\"summary\">{}</p><p><a rel=\"alternate\" type=\"application/activity+json\" href=\"/users/{}/outbox\">ActivityPub outbox</a></p></header><main><h2>Public posts</h2>{}</main></body></html>",
        escape_html(&display_name),
        escape_html(&display_name),
        escape_html(&summary),
        escape_html(&profile.username),
        posts_html,
    )
}

fn activitypub_actor_post_html(row: &Map<String, Value>) -> String {
    let id = string_field(Some(row), "id").unwrap_or_default();
    let published = string_field(Some(row), "published_at").unwrap_or_default();
    let permalink = if id.starts_with("http://") || id.starts_with("https://") {
        id
    } else {
        format!("/users/social/posts/{}", escape_html(&id))
    };
    format!(
        "<article><time>{}</time><div>{}</div><p><a href=\"{}\">Permalink</a></p></article>",
        escape_html(&published),
        mastodon_status_content(row),
        escape_html(&permalink),
    )
}

pub(crate) fn activitypub_note_object(row: &Map<String, Value>, origin: &str) -> Value {
    let id = display_local_url(origin, &string_field(Some(row), "id").unwrap_or_default());
    let actor = format!("{origin}/users/social");
    let content = mastodon_status_content(row);
    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": string_field(Some(row), "object_type").unwrap_or_else(|| "Note".to_string()),
        "url": id,
        "attributedTo": actor,
        "content": content,
        "contentMap": { "en": content },
        "published": row_value_or_null(row, "published_at"),
        "to": [PUBLIC_COLLECTION],
        "cc": [format!("{actor}/followers")],
        "replies": {
            "type": "Collection",
            "totalItems": integer_field(Some(row), "reply_count"),
        },
        "likes": {
            "type": "Collection",
            "totalItems": integer_field(Some(row), "like_count"),
        },
        "shares": {
            "type": "Collection",
            "totalItems": integer_field(Some(row), "boost_count"),
        },
    });
    if let Value::Object(ref mut object) = note {
        insert_optional_activity_string(object, "name", string_field(Some(row), "name"));
        insert_optional_activity_string(object, "summary", string_field(Some(row), "summary"));
        if let Some(reply) = string_field(Some(row), "in_reply_to") {
            object.insert(
                "inReplyTo".to_string(),
                Value::String(display_local_url(origin, &reply)),
            );
        }
        let attachments = activitypub_attachments(row);
        if !attachments.is_empty() {
            object.insert("attachment".to_string(), Value::Array(attachments));
        }
        let tags = activitypub_tags(row);
        if !tags.is_empty() {
            object.insert("tag".to_string(), Value::Array(tags));
        }
        if let Some(poll) = activitypub_poll(row) {
            let multiple = poll
                .get("multiple")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let key = if multiple { "anyOf" } else { "oneOf" };
            let options = poll
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .map(|name| {
                    serde_json::json!({
                        "type": "Note",
                        "name": name,
                        "replies": { "type": "Collection", "totalItems": 0 },
                    })
                })
                .collect::<Vec<_>>();
            object.insert(key.to_string(), Value::Array(options));
            object.insert("votersCount".to_string(), Value::from(0));
        }
    }
    note
}

fn insert_optional_activity_string(
    object: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        object.insert(key.to_string(), Value::String(value));
    }
}

fn activitypub_attachments(row: &Map<String, Value>) -> Vec<Value> {
    parse_attachment_array(row.get("media_attachments"))
        .into_iter()
        .filter_map(|attachment| {
            let object = attachment.as_object()?;
            let url = string_field(Some(object), "url")?;
            let media_type = string_field(Some(object), "mediaType")
                .unwrap_or_else(|| media_type_for_filename(&url));
            let attachment_type = if media_type.starts_with("image/") {
                "Image"
            } else {
                "Document"
            };
            let mut item = Map::new();
            item.insert(
                "type".to_string(),
                Value::String(attachment_type.to_string()),
            );
            item.insert("url".to_string(), Value::String(url));
            item.insert("mediaType".to_string(), Value::String(media_type));
            insert_optional_activity_string(&mut item, "name", string_field(Some(object), "name"));
            Some(Value::Object(item))
        })
        .collect()
}

fn activitypub_poll(row: &Map<String, Value>) -> Option<Value> {
    if string_field(Some(row), "object_type").as_deref() != Some("Question") {
        return None;
    }
    match row.get("poll_options")? {
        Value::String(text) => serde_json::from_str::<Value>(text).ok(),
        value => Some(value.clone()),
    }
}

fn activitypub_tags(row: &Map<String, Value>) -> Vec<Value> {
    let mut tags = Vec::new();
    if let Value::Array(mentions) = mastodon_mentions(row) {
        for mention in mentions {
            let Some(mention) = mention.as_object() else {
                continue;
            };
            let Some(acct) = string_field(Some(mention), "acct") else {
                continue;
            };
            tags.push(serde_json::json!({
                "type": "Mention",
                "name": format!("@{acct}"),
                "href": string_field(Some(mention), "url").unwrap_or_default(),
            }));
        }
    }
    if let Value::Array(hashtags) = mastodon_tags(row) {
        for hashtag in hashtags {
            let Some(hashtag) = hashtag.as_object() else {
                continue;
            };
            let Some(name) = string_field(Some(hashtag), "name") else {
                continue;
            };
            tags.push(serde_json::json!({
                "type": "Hashtag",
                "name": format!("#{name}"),
                "href": string_field(Some(hashtag), "url").unwrap_or_default(),
            }));
        }
    }
    tags
}

pub(crate) fn display_local_url(origin: &str, value: &str) -> String {
    let origin_host = worker::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned));
    worker::Url::parse(value)
        .ok()
        .and_then(|url| {
            let path = url.path();
            (url.host_str() == origin_host.as_deref() && path.starts_with("/users/social/"))
                .then(|| format!("{origin}{path}"))
        })
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn accepts_activity_json(req: &Request) -> bool {
    req.headers()
        .get("Accept")
        .ok()
        .flatten()
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("activity+json")
                || value.contains("application/ld+json")
                || value.contains("application/json")
        })
        .unwrap_or(false)
}

pub(crate) fn signature_actor_id(req: &Request) -> Result<Option<String>> {
    let Some(header) = req.headers().get("Signature")? else {
        return Ok(None);
    };
    let Some(key_id) = signature_header_value(&header, "keyId") else {
        return Ok(None);
    };
    let actor_id = key_id
        .split('#')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok((!actor_id.is_empty()).then_some(actor_id))
}

fn signature_header_value(header: &str, key: &str) -> Option<String> {
    for part in header.split(',') {
        let mut pieces = part.splitn(2, '=');
        let name = pieces.next()?.trim();
        let value = pieces.next()?.trim().trim_matches('"');
        if name.eq_ignore_ascii_case(key) && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

pub(crate) fn actor_domain(actor_id: &str) -> String {
    worker::Url::parse(actor_id)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default()
}

pub(crate) fn activitypub_public_recipients(activity: &Value, object: &Value) -> bool {
    let mut recipients = Vec::new();
    collect_recipients(activity.get("to"), &mut recipients);
    collect_recipients(activity.get("cc"), &mut recipients);
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    recipients.iter().any(|value| value == PUBLIC_COLLECTION)
}

pub(crate) fn activitypub_direct_to_actor(object: &Value, actor_url: &str) -> bool {
    let mut recipients = Vec::new();
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    recipients.iter().any(|value| value == actor_url)
        && !recipients.iter().any(|value| value == PUBLIC_COLLECTION)
}

pub(crate) fn supported_timeline_object_type(object_type: &str) -> bool {
    matches!(
        object_type,
        "Note"
            | "Question"
            | "Article"
            | "Page"
            | "Image"
            | "Video"
            | "Audio"
            | "Event"
            | "Document"
            | "Review"
    )
}

pub(crate) fn activitypub_object_content_html(object: &Map<String, Value>) -> String {
    if let Some(content) = object
        .get("content")
        .and_then(|value| value_string(Some(value)))
    {
        return content;
    }
    if let Some(content_map) = object
        .get("contentMap")
        .and_then(Value::as_object)
        .and_then(|map| map.get("en").or_else(|| map.values().next()))
        .and_then(|value| value_string(Some(value)))
    {
        return content_map;
    }

    let object_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("Object");
    let mut parts = Vec::new();
    if let Some(name) = object.get("name").and_then(optional_body_string) {
        parts.push(format!("<p><strong>{}</strong></p>", escape_html(&name)));
    }
    if let Some(summary) = object.get("summary").and_then(optional_body_string) {
        parts.push(format!("<p>{}</p>", escape_html(&summary)));
    }
    if object_type == "Event" {
        if let Some(start) = object.get("startTime").and_then(optional_body_string) {
            parts.push(format!("<p>Starts: {}</p>", escape_html(&start)));
        }
        if let Some(end) = object.get("endTime").and_then(optional_body_string) {
            parts.push(format!("<p>Ends: {}</p>", escape_html(&end)));
        }
        if let Some(location) = object.get("location").and_then(activitypub_location_label) {
            parts.push(format!("<p>Location: {}</p>", escape_html(&location)));
        }
    }
    if parts.is_empty() {
        if let Some(url) = object
            .get("url")
            .or_else(|| object.get("id"))
            .and_then(optional_body_string)
        {
            parts.push(format!(
                "<p>{} from {}</p>",
                escape_html(object_type),
                escape_html(&url)
            ));
        }
    }
    parts.join("")
}

fn activitypub_location_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()).filter(|text| !text.is_empty()),
        Value::Object(object) => object
            .get("name")
            .or_else(|| object.get("address"))
            .and_then(optional_body_string),
        _ => None,
    }
}

pub(crate) fn collect_recipients(value: Option<&Value>, recipients: &mut Vec<String>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = optional_body_string(item) {
                    recipients.push(text);
                }
            }
        }
        Some(value) => {
            if let Some(text) = optional_body_string(value) {
                recipients.push(text);
            }
        }
        None => {}
    }
}

pub(crate) async fn resolve_activitypub_object_inbox(
    object_id: &str,
) -> std::result::Result<String, String> {
    let object_url = public_https_url(object_id, "object_id")?;
    if let Some(inbox) = local_object_inbox(&object_url) {
        return Ok(inbox);
    }
    let object = fetch_activitypub_json(&object_url, "object").await?;
    let actor_id = object
        .get("attributedTo")
        .or_else(|| object.get("actor"))
        .and_then(optional_body_string)
        .ok_or_else(|| "object does not expose attributedTo or actor".to_string())?;
    let actor_url = public_https_url(&actor_id, "target")?;
    let actor = fetch_activitypub_json(&actor_url, "actor").await?;
    let inbox = actor
        .get("inbox")
        .and_then(optional_body_string)
        .unwrap_or_default();
    if inbox.is_empty() {
        return Err("object actor does not expose inbox".to_string());
    }
    let shared_inbox = actor
        .get("endpoints")
        .and_then(Value::as_object)
        .and_then(|endpoints| endpoints.get("sharedInbox"))
        .and_then(optional_body_string);
    Ok(shared_inbox.unwrap_or(inbox))
}

pub(crate) async fn resolve_activitypub_actor(
    target: &str,
) -> std::result::Result<RemoteActor, String> {
    let actor_url = activitypub_actor_url_for_target(target).await?;
    let actor = fetch_activitypub_json(&actor_url, "actor").await?;
    remote_actor_from_json(actor_url, actor)
}

pub(crate) async fn resolve_activitypub_actor_for_local(
    target: &str,
    local_actor: &LocalActor,
) -> std::result::Result<RemoteActor, String> {
    let actor_url = activitypub_actor_url_for_target(target).await?;
    let actor = match fetch_activitypub_json(&actor_url, "actor").await {
        Ok(actor) => actor,
        Err(unsigned_error)
            if should_retry_signed_fetch(&unsigned_error) && local_actor.can_sign() =>
        {
            fetch_activitypub_json_signed(&actor_url, "actor", local_actor)
                .await
                .map_err(|signed_error| {
                    format!("{unsigned_error}; signed retry failed: {signed_error}")
                })?
        }
        Err(unsigned_error) if should_retry_signed_fetch(&unsigned_error) => {
            return Err(format!(
                "{unsigned_error}; signed retry skipped: local actor signing key is not configured"
            ));
        }
        Err(error) => return Err(error),
    };
    remote_actor_from_json(actor_url, actor)
}

pub(crate) async fn activitypub_actor_url_for_target(
    target: &str,
) -> std::result::Result<String, String> {
    if target.starts_with("http://") || target.starts_with("https://") {
        public_https_url(target, "target")
    } else {
        resolve_webfinger_actor(target).await
    }
}

fn remote_actor_from_json(
    actor_url: String,
    actor: Value,
) -> std::result::Result<RemoteActor, String> {
    let endpoints = actor.get("endpoints").and_then(Value::as_object);
    Ok(RemoteActor {
        id: actor
            .get("id")
            .and_then(optional_body_string)
            .unwrap_or_else(|| actor_url.clone()),
        actor_type: actor.get("type").and_then(optional_body_string),
        inbox: actor
            .get("inbox")
            .and_then(optional_body_string)
            .unwrap_or_default(),
        shared_inbox: endpoints
            .and_then(|value| value.get("sharedInbox"))
            .and_then(optional_body_string),
        preferred_username: actor
            .get("preferredUsername")
            .and_then(optional_body_string),
        name: actor
            .get("name")
            .and_then(optional_body_string)
            .or_else(|| {
                actor
                    .get("preferredUsername")
                    .and_then(optional_body_string)
            }),
        summary: actor.get("summary").and_then(optional_body_string),
        icon_url: actor
            .get("icon")
            .and_then(Value::as_object)
            .and_then(|icon| icon.get("url"))
            .and_then(optional_body_string),
        url: actor
            .get("url")
            .and_then(optional_body_string)
            .or(Some(actor_url)),
        outbox: actor.get("outbox").and_then(optional_body_string),
    })
}

pub(crate) fn should_retry_signed_fetch(error: &str) -> bool {
    error.contains("HTTP 401") || error.contains("HTTP 403")
}

async fn resolve_webfinger_actor(target: &str) -> std::result::Result<String, String> {
    let handle = target.trim().trim_start_matches('@');
    if !handle.contains('@') {
        return Err("target must be an actor URL or @user@domain handle".to_string());
    }
    let domain = handle.rsplit('@').next().unwrap_or_default().trim();
    public_https_url(&format!("https://{domain}/"), "target domain")?;
    let resource = format!("acct:{handle}");
    let url = format!(
        "https://{}/.well-known/webfinger?resource={}",
        domain,
        urlencoding::encode(&resource)
    );
    let jrd =
        fetch_json_with_accept(&url, "application/jrd+json, application/json", "webfinger").await?;
    let links = jrd
        .get("links")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("no ActivityPub self link found for {target}"))?;
    for link in links {
        let Some(object) = link.as_object() else {
            continue;
        };
        let rel = object
            .get("rel")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let link_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let href = object
            .get("href")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if rel == "self" && link_type.contains("activity+json") && !href.is_empty() {
            return public_https_url(href, "actor link");
        }
    }
    Err(format!("no ActivityPub self link found for {target}"))
}

pub(crate) async fn discover_public_post_target(target: &str) -> Option<Map<String, Value>> {
    if !target.starts_with("http://") && !target.starts_with("https://") {
        return None;
    }
    let object_url = public_https_url(target, "target public post").ok()?;
    let item = fetch_activitypub_json(&object_url, "object").await.ok()?;
    normalize_discovered_public_post(&item)
}

pub(crate) async fn fetch_actor_recent_public_posts(
    actor: &RemoteActor,
) -> Vec<Map<String, Value>> {
    let Some(outbox) = actor.outbox.as_deref() else {
        return Vec::new();
    };
    let Ok(outbox_url) = public_https_url(outbox, "actor outbox") else {
        return Vec::new();
    };
    let Ok(outbox) = fetch_activitypub_json(&outbox_url, "object").await else {
        return Vec::new();
    };
    let page = match outbox.get("first").and_then(|value| {
        value.as_str().map(ToOwned::to_owned).or_else(|| {
            value
                .as_object()
                .and_then(|object| object.get("id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
    }) {
        Some(page_url) => match public_https_url(&page_url, "actor outbox first page") {
            Ok(url) => fetch_activitypub_json(&url, "object")
                .await
                .unwrap_or_else(|_| outbox.clone()),
            Err(_) => outbox.clone(),
        },
        None => outbox.clone(),
    };
    let items = page
        .get("orderedItems")
        .or_else(|| page.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items
        .iter()
        .filter_map(normalize_discovered_public_post)
        .take(3)
        .collect()
}

pub(crate) async fn fetch_activitypub_json(
    url: &str,
    label: &str,
) -> std::result::Result<Value, String> {
    if let Some(value) = local_activitypub_fixture_value(url) {
        return Ok(value);
    }
    fetch_json_with_accept_and_headers(
        url,
        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json",
        label,
        &[],
    )
    .await
}

pub(crate) async fn fetch_activitypub_json_signed(
    url: &str,
    label: &str,
    local_actor: &LocalActor,
) -> std::result::Result<Value, String> {
    if let Some(value) = local_activitypub_fixture_value(url) {
        return Ok(value);
    }
    let signed_headers = signed_activitypub_get_headers(url, local_actor)?;
    fetch_json_with_accept_and_headers(
        url,
        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\", application/json",
        label,
        &signed_headers,
    )
    .await
}

fn local_activitypub_fixture_value(url: &str) -> Option<Value> {
    let parsed = worker::Url::parse(url).ok()?;
    match parsed.path() {
        "/__dais-fixtures/activitypub/actor" => {
            let public_key = fixture_public_key(&parsed)?;
            let actor_url = parsed.to_string();
            let name = parsed
                .query_pairs()
                .find(|(key, _)| key == "name")
                .map(|(_, value)| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "dais-s2s-fixture".to_string());
            Some(serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": actor_url,
                "type": "Application",
                "preferredUsername": name,
                "name": name,
                "inbox": format!("{}://{}/__dais-fixtures/activitypub/inbox", parsed.scheme(), parsed.host_str().unwrap_or_default()),
                "outbox": fixture_url_with_public_key(&parsed, "/__dais-fixtures/activitypub/outbox"),
                "publicKey": {
                    "id": format!("{actor_url}#main-key"),
                    "owner": actor_url,
                    "publicKeyPem": public_key,
                },
            }))
        }
        "/__dais-fixtures/activitypub/outbox" => {
            let post = local_activitypub_fixture_post_value(&parsed)?;
            let post_id = post.get("id").and_then(Value::as_str).unwrap_or_default();
            Some(serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": parsed.to_string(),
                "type": "OrderedCollection",
                "totalItems": 1,
                "orderedItems": [
                    {
                        "id": format!("{post_id}#create"),
                        "type": "Create",
                        "actor": post.get("attributedTo").cloned().unwrap_or(Value::Null),
                        "to": post.get("to").cloned().unwrap_or(Value::Array(Vec::new())),
                        "object": post,
                    }
                ],
            }))
        }
        "/__dais-fixtures/activitypub/posts/public-preview" => {
            local_activitypub_fixture_post_value(&parsed)
        }
        _ => None,
    }
}

fn local_activitypub_fixture_post_value(url: &worker::Url) -> Option<Value> {
    let post_id =
        fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/posts/public-preview");
    let object_type = url
        .query_pairs()
        .find(|(key, _)| key == "kind")
        .map(|(_, value)| value.to_string())
        .filter(|value| supported_timeline_object_type(value))
        .unwrap_or_else(|| "Note".to_string());
    let actor = fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/actor");
    let mut object = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": post_id,
        "type": object_type.clone(),
        "attributedTo": actor,
        "to": [PUBLIC_COLLECTION],
        "published": "2026-06-16T00:00:00Z",
        "url": post_id,
    });
    match object_type.as_str() {
        "Image" => {
            object["name"] = Value::String("Dais fixture public image".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from an image server.".to_string());
            object["url"] = serde_json::json!([{
                "type": "Link",
                "mediaType": "image/png",
                "href": post_id,
            }]);
        }
        "Video" => {
            object["name"] = Value::String("Dais fixture public video".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from a video server.".to_string());
        }
        "Audio" => {
            object["name"] = Value::String("Dais fixture public audio".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from an audio server.".to_string());
        }
        "Event" => {
            object["name"] = Value::String("Dais fixture public event".to_string());
            object["summary"] =
                Value::String("Dais fixture public preview post from an event server.".to_string());
            object["startTime"] = Value::String("2026-06-17T18:00:00Z".to_string());
            object["endTime"] = Value::String("2026-06-17T19:00:00Z".to_string());
            object["location"] = serde_json::json!({
                "type": "Place",
                "name": "Example venue",
            });
        }
        "Article" | "Page" | "Review" => {
            object["name"] = Value::String(format!("Dais fixture public {object_type}"));
            object["content"] = Value::String(format!(
                "<p>Dais fixture public preview post from a {} server.</p>",
                object_type.to_ascii_lowercase()
            ));
        }
        _ => {
            object["content"] =
                Value::String("<p>Dais fixture public preview post</p>".to_string());
        }
    }
    Some(object)
}

pub(crate) async fn fetch_json_with_accept(
    url: &str,
    accept: &str,
    label: &str,
) -> std::result::Result<Value, String> {
    fetch_json_with_accept_and_headers(url, accept, label, &[]).await
}

pub(crate) async fn fetch_lenient_json_with_accept(
    url: &str,
    accept: &str,
    label: &str,
) -> std::result::Result<Value, String> {
    let headers = Headers::new();
    headers
        .set("Accept", accept)
        .map_err(|error| error.to_string())?;
    headers
        .set("User-Agent", "dais-owner-api/1.0")
        .map_err(|error| error.to_string())?;
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init).map_err(|error| error.to_string())?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status_code();
    if !(200..=299).contains(&status) {
        return Err(format!("could not fetch {label} {url}: HTTP {status}"));
    }
    let body = response.text().await.map_err(|error| error.to_string())?;
    parse_lenient_json_body(&body).map_err(|error| format!("could not parse {label} JSON: {error}"))
}

pub(crate) fn parse_lenient_json_body(body: &str) -> std::result::Result<Value, serde_json::Error> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return serde_json::from_str(trimmed);
    }
    let json_start = trimmed
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '{' | '[').then_some(index))
        .unwrap_or(0);
    serde_json::from_str(&trimmed[json_start..])
}

pub(crate) async fn fetch_json_with_accept_and_headers(
    url: &str,
    accept: &str,
    label: &str,
    extra_headers: &[(String, String)],
) -> std::result::Result<Value, String> {
    let headers = Headers::new();
    headers
        .set("Accept", accept)
        .map_err(|error| error.to_string())?;
    headers
        .set("User-Agent", "dais-owner-api/1.0")
        .map_err(|error| error.to_string())?;
    for (name, value) in extra_headers {
        headers
            .set(name, value)
            .map_err(|error| error.to_string())?;
    }
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Get).with_headers(headers);
    let request = Request::new_with_init(url, &init).map_err(|error| error.to_string())?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status_code();
    if !(200..=299).contains(&status) {
        return Err(format!("could not fetch {label} {url}: HTTP {status}"));
    }
    response
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())
}

fn signed_activitypub_get_headers(
    url: &str,
    local_actor: &LocalActor,
) -> std::result::Result<Vec<(String, String)>, String> {
    let parsed = worker::Url::parse(url).map_err(|error| error.to_string())?;
    let host = activitypub_request_host(&parsed)?;
    let request_target = activitypub_request_target(&parsed, &host);
    let date = js_sys::Date::new_0()
        .to_utc_string()
        .as_string()
        .unwrap_or_default();
    if date.is_empty() {
        return Err("could not generate Date header".to_string());
    }

    let mut sign_headers = HashMap::new();
    sign_headers.insert("host".to_string(), host.clone());
    sign_headers.insert("date".to_string(), date.clone());
    let headers_to_sign = vec![
        "(request-target)".to_string(),
        "host".to_string(),
        "date".to_string(),
    ];
    let key_id = format!("{}#main-key", local_actor.id);
    let signature = sign_request(
        &local_actor.private_key,
        &key_id,
        "GET",
        &request_target,
        &sign_headers,
        &headers_to_sign,
    )?;
    Ok(vec![
        ("Host".to_string(), host),
        ("Date".to_string(), date),
        ("Signature".to_string(), signature.to_header()),
    ])
}

fn activitypub_request_host(url: &worker::Url) -> std::result::Result<String, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "target URL is missing a host".to_string())?;
    match url.port() {
        Some(port) => Ok(format!("{host}:{port}")),
        None => Ok(host.to_string()),
    }
}

fn activitypub_request_target(url: &worker::Url, host: &str) -> String {
    let origin = format!("{}://{}", url.scheme(), host);
    url.to_string()
        .strip_prefix(&origin)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| url.path().to_string())
}

fn local_object_inbox(object_id: &str) -> Option<String> {
    let url = worker::Url::parse(object_id).ok()?;
    let mut parts = url.path().split('/').filter(|part| !part.is_empty());
    if parts.next()? != "users" {
        return None;
    }
    let username = parts.next()?;
    if parts.next()? != "posts" || parts.next().is_none() {
        return None;
    }
    Some(format!(
        "{}://{}/users/{}/inbox",
        url.scheme(),
        url.host_str()?,
        username
    ))
}

pub(crate) fn normalize_discovered_public_post(item: &Value) -> Option<Map<String, Value>> {
    let object = if item.get("type").and_then(Value::as_str) == Some("Create") {
        item.get("object").unwrap_or(item)
    } else {
        item
    };
    let object_type = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !supported_timeline_object_type(object_type) {
        return None;
    }
    let object_map = object.as_object()?;
    let mut recipients = Vec::new();
    collect_recipients(object.get("to"), &mut recipients);
    collect_recipients(item.get("to"), &mut recipients);
    collect_recipients(object.get("cc"), &mut recipients);
    collect_recipients(item.get("cc"), &mut recipients);
    if !recipients.iter().any(|value| value == PUBLIC_COLLECTION) {
        return None;
    }
    let mut post = Map::new();
    post.insert(
        "id".to_string(),
        Value::String(
            object
                .get("id")
                .or_else(|| item.get("id"))
                .and_then(optional_body_string)
                .unwrap_or_default(),
        ),
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
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "name".to_string(),
        object
            .get("name")
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    post.insert(
        "summary".to_string(),
        object
            .get("summary")
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    let content = activitypub_object_content_html(object_map);
    post.insert(
        "content".to_string(),
        Value::String(strip_html(&content).chars().take(280).collect()),
    );
    post.insert(
        "published".to_string(),
        object
            .get("published")
            .or_else(|| item.get("published"))
            .and_then(optional_body_string)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    Some(post)
}

fn public_post_actor_id(item: &Value, object: &Value) -> Option<String> {
    let actor = object
        .get("attributedTo")
        .or_else(|| object.get("actor"))
        .or_else(|| item.get("actor"))
        .or_else(|| item.get("attributedTo"))?;
    match actor {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => items.iter().find_map(optional_body_string),
        _ => None,
    }
}

pub(crate) fn actor_handle(actor: &RemoteActor) -> Option<String> {
    let preferred_username = actor.preferred_username.as_deref()?;
    let url = worker::Url::parse(actor.url.as_deref().unwrap_or(&actor.id)).ok()?;
    Some(format!(
        "@{}@{}",
        preferred_username,
        url.host_str().unwrap_or_default()
    ))
}
