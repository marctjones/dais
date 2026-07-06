use crate::{
    escape_html, integer_field, mastodon_mentions, mastodon_status_content, mastodon_tags,
    media_type_for_filename, optional_body_string, parse_attachment_array, row_value_or_null,
    string_field, value_string, OwnerProfile, PUBLIC_COLLECTION,
};
use serde_json::{Map, Value};
use worker::{Request, Result};

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
