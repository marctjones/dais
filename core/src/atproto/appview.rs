//! AT Protocol AppView response helpers.
//!
//! The Cloudflare PDS worker still owns D1/R2 access, but Bluesky/AppView post
//! and thread shapes are protocol-level behavior. Keeping them in core prevents
//! Desk/router/PDS surfaces from each inventing slightly different feed JSON.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::records::{repo_path_from_at_uri, stable_cid};
use super::repo::{repo_record_block, AtprotoIdentity};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaAttachment {
    #[serde(default = "default_image_attachment_type", rename = "type")]
    pub attachment_type: String,
    pub url: String,
    #[serde(default, rename = "mediaType")]
    pub media_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cid: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppViewPost {
    pub id: String,
    pub content: String,
    pub published_at: String,
    pub summary: String,
    pub atproto_uri: Option<String>,
    pub atproto_reply_json: Option<String>,
    pub in_reply_to: Option<String>,
    pub media_attachments: Vec<MediaAttachment>,
    pub reply_count: u64,
    pub repost_count: u64,
    pub like_count: u64,
}

impl AppViewPost {
    pub fn from_row(row: Map<String, Value>) -> Self {
        Self {
            id: string_field(&row, "id"),
            content: string_field(&row, "content"),
            published_at: string_field(&row, "published_at"),
            summary: string_field(&row, "summary"),
            atproto_uri: optional_string_field(&row, "atproto_uri"),
            atproto_reply_json: optional_string_field(&row, "atproto_reply_json"),
            in_reply_to: optional_string_field(&row, "in_reply_to"),
            media_attachments: media_attachments_from_row(&row),
            reply_count: u64_field(&row, "reply_count"),
            repost_count: u64_field(&row, "repost_count"),
            like_count: u64_field(&row, "like_count"),
        }
    }
}

pub fn media_attachments_from_row(row: &Map<String, Value>) -> Vec<MediaAttachment> {
    let raw = row
        .get("media_attachments")
        .and_then(Value::as_str)
        .unwrap_or("");
    parse_media_attachments(raw)
}

pub fn parse_media_attachments(raw: &str) -> Vec<MediaAttachment> {
    serde_json::from_str::<Vec<MediaAttachment>>(raw).unwrap_or_default()
}

pub fn media_attachment_cid(attachment: &MediaAttachment) -> String {
    if attachment.cid.is_empty() {
        stable_cid(&attachment.url)
    } else {
        attachment.cid.clone()
    }
}

pub fn post_at_uri(identity: &AtprotoIdentity, post: &AppViewPost) -> String {
    post.atproto_uri.clone().unwrap_or_else(|| {
        let rkey = post.id.rsplit('/').next().unwrap_or(post.id.as_str());
        format!("at://{}/app.bsky.feed.post/{rkey}", identity.did)
    })
}

pub fn post_record_value(post: &AppViewPost) -> Value {
    let (facets, tags) = feed_post_facets(&post.content);
    let mut record = serde_json::json!({
        "$type": "app.bsky.feed.post",
        "text": post.content,
        "createdAt": post.published_at
    });
    if let Some(object) = record.as_object_mut() {
        if !post.content.trim().is_empty() {
            object.insert("langs".to_string(), serde_json::json!(["en"]));
        }
        if !facets.is_empty() {
            object.insert("facets".to_string(), Value::Array(facets));
        }
        if !tags.is_empty() {
            object.insert(
                "tags".to_string(),
                Value::Array(tags.into_iter().map(Value::String).collect()),
            );
        }
        if !post.summary.trim().is_empty() {
            object.insert(
                "labels".to_string(),
                serde_json::json!({
                    "$type": "com.atproto.label.defs#selfLabels",
                    "values": [{ "val": "!warn" }]
                }),
            );
        }
    }

    if let Some(reply) = post
        .atproto_reply_json
        .as_deref()
        .filter(|value| !value.is_empty())
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
    {
        if let Some(object) = record.as_object_mut() {
            object.insert("reply".to_string(), reply);
        }
    } else if let Some(in_reply_to) = post
        .in_reply_to
        .as_deref()
        .filter(|value| value.starts_with("at://"))
    {
        let cid = stable_cid(in_reply_to);
        if let Some(object) = record.as_object_mut() {
            object.insert(
                "reply".to_string(),
                serde_json::json!({
                    "root": {
                        "uri": in_reply_to,
                        "cid": cid
                    },
                    "parent": {
                        "uri": in_reply_to,
                        "cid": cid
                    }
                }),
            );
        }
    }

    let images: Vec<Value> = post
        .media_attachments
        .iter()
        .filter(|attachment| attachment.media_type.starts_with("image/"))
        .map(|attachment| {
            let cid = media_attachment_cid(attachment);
            serde_json::json!({
                "alt": attachment.name,
                "image": {
                    "$type": "blob",
                    "ref": { "$link": cid },
                    "mimeType": attachment.media_type,
                    "size": attachment.size
                }
            })
        })
        .collect();
    if !images.is_empty() {
        if let Some(object) = record.as_object_mut() {
            object.insert(
                "embed".to_string(),
                serde_json::json!({
                    "$type": "app.bsky.embed.images",
                    "images": images
                }),
            );
        }
    }

    record
}

pub fn post_view(identity: &AtprotoIdentity, post: AppViewPost) -> Value {
    let uri = post_at_uri(identity, &post);
    let record = post_record_value(&post);
    let cid = repo_record_block(
        repo_path_from_at_uri(&uri).unwrap_or_default(),
        record.clone(),
    )
    .map(|block| block.cid.to_string())
    .unwrap_or_else(|_| stable_cid(&uri));
    serde_json::json!({
        "uri": uri,
        "cid": cid,
        "author": {
            "did": identity.did,
            "handle": identity.handle,
            "displayName": "dais"
        },
        "record": record,
        "replyCount": post.reply_count,
        "repostCount": post.repost_count,
        "likeCount": post.like_count,
        "indexedAt": post.published_at
    })
}

pub fn thread_view_post(
    identity: &AtprotoIdentity,
    post: AppViewPost,
    replies: Vec<Value>,
) -> Value {
    serde_json::json!({
        "$type": "app.bsky.feed.defs#threadViewPost",
        "post": post_view(identity, post),
        "replies": replies
    })
}

fn default_image_attachment_type() -> String {
    "Image".to_string()
}

fn feed_post_facets(text: &str) -> (Vec<Value>, Vec<String>) {
    let mut facets = Vec::new();
    let mut tags = Vec::new();
    let mut link_ranges = Vec::new();

    for (start, _) in text
        .match_indices("http://")
        .chain(text.match_indices("https://"))
    {
        let end = start + trimmed_url_len(&text[start..]);
        if end <= start {
            continue;
        }
        let uri = &text[start..end];
        link_ranges.push((start, end));
        facets.push(facet(
            start,
            end,
            "app.bsky.richtext.facet#link",
            "uri",
            uri,
        ));
    }

    for (start, _) in text.match_indices('#') {
        if link_ranges
            .iter()
            .any(|(link_start, link_end)| start >= *link_start && start < *link_end)
        {
            continue;
        }
        let end = scan_tag_end(text, start + 1);
        if end <= start + 1 {
            continue;
        }
        let tag = &text[start + 1..end];
        if tag.len() > 640 {
            continue;
        }
        facets.push(facet(start, end, "app.bsky.richtext.facet#tag", "tag", tag));
        push_unique_tag(&mut tags, tag);
    }

    facets.sort_by_key(|value| {
        value
            .get("index")
            .and_then(|index| index.get("byteStart"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    });
    (facets, tags)
}

fn facet(start: usize, end: usize, feature_type: &str, field: &str, value: &str) -> Value {
    serde_json::json!({
        "index": {
            "byteStart": start,
            "byteEnd": end
        },
        "features": [{
            "$type": feature_type,
            field: value
        }]
    })
}

fn trimmed_url_len(value: &str) -> usize {
    let mut end = value.len();
    for (index, ch) in value.char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'') {
            end = index;
            break;
        }
    }
    while end > 0 {
        let Some((index, ch)) = value[..end].char_indices().next_back() else {
            break;
        };
        if matches!(ch, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']') {
            end = index;
        } else {
            break;
        }
    }
    end
}

fn scan_tag_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = start + offset + ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn push_unique_tag(tags: &mut Vec<String>, tag: &str) {
    if tags.len() >= 8 {
        return;
    }
    if tags
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(tag))
    {
        return;
    }
    tags.push(tag.to_string());
}

fn string_field(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn optional_string_field(row: &Map<String, Value>, key: &str) -> Option<String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn u64_field(row: &Map<String, Value>, key: &str) -> u64 {
    row.get(key).and_then(Value::as_u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn identity() -> AtprotoIdentity {
        AtprotoIdentity {
            did: "did:web:social.dais.social".into(),
            handle: "social.dais.social".into(),
            pds_hostname: "pds.dais.social".into(),
        }
    }

    #[test]
    fn post_view_exposes_counts_record_facets_labels_reply_and_images() {
        let mut row = Map::new();
        row.insert(
            "id".into(),
            json!("https://social.dais.social/users/social/posts/local1"),
        );
        row.insert(
            "content".into(),
            json!("Hello @ada.example #space https://example.com/news."),
        );
        row.insert("summary".into(), json!("science"));
        row.insert("published_at".into(), json!("2026-07-04T12:00:00.000Z"));
        row.insert("reply_count".into(), json!(2));
        row.insert("repost_count".into(), json!(3));
        row.insert("like_count".into(), json!(5));
        row.insert(
            "in_reply_to".into(),
            json!("at://did:web:social.dais.social/app.bsky.feed.post/root1"),
        );
        row.insert(
            "media_attachments".into(),
            json!(serde_json::to_string(&vec![MediaAttachment {
                attachment_type: "Image".to_string(),
                url: "https://social.dais.social/media/uploads/atproto/bafyimg.png".to_string(),
                media_type: "image/png".to_string(),
                name: "alt text".to_string(),
                cid: "bafyimg".to_string(),
                size: 123,
            }])
            .unwrap()),
        );

        let post = AppViewPost::from_row(row.clone());
        let view = post_view(&identity(), post);
        assert_eq!(
            view.get("uri").and_then(Value::as_str),
            Some("at://did:web:social.dais.social/app.bsky.feed.post/local1")
        );
        assert_eq!(view.get("replyCount").and_then(Value::as_u64), Some(2));
        assert_eq!(view.get("repostCount").and_then(Value::as_u64), Some(3));
        assert_eq!(view.get("likeCount").and_then(Value::as_u64), Some(5));
        assert!(view.get("cid").and_then(Value::as_str).is_some());

        let record = view.get("record").expect("record");
        assert_eq!(
            record.get("$type").and_then(Value::as_str),
            Some("app.bsky.feed.post")
        );
        assert_eq!(
            record.get("text").and_then(Value::as_str),
            Some("Hello @ada.example #space https://example.com/news.")
        );
        assert!(record.get("facets").and_then(Value::as_array).is_some());
        assert!(record.get("tags").and_then(Value::as_array).is_some());
        assert!(record.get("labels").is_some());
        assert!(record.get("embed").is_some());
        assert_eq!(
            record
                .get("reply")
                .and_then(|reply| reply.get("parent"))
                .and_then(|parent| parent.get("uri"))
                .and_then(Value::as_str),
            Some("at://did:web:social.dais.social/app.bsky.feed.post/root1")
        );

        assert_eq!(
            post_at_uri(&identity(), &AppViewPost::from_row(row.clone())),
            "at://did:web:social.dais.social/app.bsky.feed.post/local1"
        );
        assert_eq!(
            post_record_value(&AppViewPost::from_row(row))
                .get("langs")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn thread_view_post_preserves_nested_reply_shape() {
        let parent = AppViewPost {
            id: "https://social.dais.social/users/social/posts/root".to_string(),
            content: "root".to_string(),
            published_at: "2026-07-04T12:00:00.000Z".to_string(),
            ..Default::default()
        };
        let child = AppViewPost {
            id: "https://social.dais.social/users/social/posts/reply".to_string(),
            content: "reply".to_string(),
            published_at: "2026-07-04T12:01:00.000Z".to_string(),
            ..Default::default()
        };

        let child_thread = thread_view_post(&identity(), child, Vec::new());
        let parent_thread = thread_view_post(&identity(), parent, vec![child_thread]);
        assert_eq!(
            parent_thread.get("$type").and_then(Value::as_str),
            Some("app.bsky.feed.defs#threadViewPost")
        );
        let replies = parent_thread
            .get("replies")
            .and_then(Value::as_array)
            .expect("replies");
        assert_eq!(replies.len(), 1);
        assert_eq!(
            replies[0]
                .get("post")
                .and_then(|post| post.get("record"))
                .and_then(|record| record.get("text"))
                .and_then(Value::as_str),
            Some("reply")
        );
    }

    #[test]
    fn media_attachment_cid_falls_back_to_stable_url_cid() {
        let with_cid = MediaAttachment {
            attachment_type: "Image".to_string(),
            url: "https://social.dais.social/media/uploads/atproto/one.png".to_string(),
            media_type: "image/png".to_string(),
            name: String::new(),
            cid: "bafyexplicit".to_string(),
            size: 0,
        };
        assert_eq!(media_attachment_cid(&with_cid), "bafyexplicit");

        let without_cid = MediaAttachment {
            cid: String::new(),
            ..with_cid
        };
        assert!(media_attachment_cid(&without_cid).starts_with("baf"));
    }
}
