use serde_json::{Map, Value};

pub(crate) fn visibility(value: &str) -> &'static str {
    match value {
        "public" => "public",
        "unlisted" => "unlisted",
        "direct" => "direct",
        _ => "private",
    }
}

pub(crate) fn follow_request_action(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/follow_requests/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(id) = parts.next() else {
        return false;
    };
    !id.is_empty() && matches!(parts.next(), Some("authorize" | "reject")) && parts.next().is_none()
}

pub(crate) fn suggestion_dismiss(path: &str) -> bool {
    path.strip_prefix("/api/v1/suggestions/")
        .map(|rest| !rest.is_empty() && !rest.contains('/'))
        .unwrap_or(false)
}

pub(crate) fn account_statuses_path(path: &str) -> bool {
    account_collection_path(path, "statuses")
}

pub(crate) fn account_followers_path(path: &str) -> bool {
    account_collection_path(path, "followers")
}

pub(crate) fn account_following_path(path: &str) -> bool {
    account_collection_path(path, "following")
}

fn account_collection_path(path: &str, collection: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/accounts/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(id) = parts.next() else {
        return false;
    };
    !id.is_empty() && parts.next() == Some(collection) && parts.next().is_none()
}

pub(crate) fn account_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/accounts/") else {
        return false;
    };
    !rest.is_empty() && !rest.contains('/')
}

pub(crate) fn account_action_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/v1/accounts/")?;
    let mut parts = rest.split('/');
    let id = parts.next()?;
    let action = parts.next()?;
    if id.is_empty()
        || parts.next().is_some()
        || !matches!(
            action,
            "follow" | "unfollow" | "block" | "unblock" | "mute" | "unmute"
        )
    {
        return None;
    }
    Some((id.to_string(), action.to_string()))
}

pub(crate) fn status_context_path(path: &str) -> Option<String> {
    status_subpath(path, "context")
}

pub(crate) fn status_source_path(path: &str) -> Option<String> {
    status_subpath(path, "source")
}

pub(crate) fn status_action_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    for action in ["favourite", "unfavourite", "reblog", "unreblog"] {
        let suffix = format!("/{action}");
        if let Some(id) = rest.strip_suffix(&suffix).filter(|id| !id.is_empty()) {
            return Some((id.to_string(), action.to_string()));
        }
    }
    None
}

fn status_subpath(path: &str, suffix: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    let needle = format!("/{suffix}");
    let id = rest.strip_suffix(&needle)?;
    (!id.is_empty()).then(|| id.to_string())
}

pub(crate) fn status_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    (!rest.is_empty() && !rest.contains('/')).then(|| rest.to_string())
}

pub(crate) fn media_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/v1/media/")
        .or_else(|| path.strip_prefix("/api/v2/media/"))
        .filter(|rest| !rest.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn notification_dismiss_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/notifications/")?;
    let id = rest.strip_suffix("/dismiss")?;
    (!id.is_empty()).then(|| id.to_string())
}

pub(crate) fn status_json(row: &Map<String, Value>, account: &Value) -> Value {
    serde_json::json!({
        "id": crate::row_value_or_null(row, "id"),
        "uri": crate::row_value_or_null(row, "id"),
        "url": crate::row_value_or_null(row, "id"),
        "account": account,
        "in_reply_to_id": crate::row_value_or_null(row, "in_reply_to"),
        "in_reply_to_account_id": Value::Null,
        "reblog": Value::Null,
        "content": status_content(row),
        "plain_text": plain_text(row),
        "created_at": crate::row_value_or_null(row, "published_at"),
        "edited_at": Value::Null,
        "emojis": [],
        "replies_count": crate::integer_field(Some(row), "reply_count"),
        "reblogs_count": crate::integer_field(Some(row), "boost_count"),
        "favourites_count": crate::integer_field(Some(row), "like_count"),
        "reblogged": crate::bool_field(Some(row), "reblogged"),
        "favourited": crate::bool_field(Some(row), "favourited"),
        "muted": false,
        "sensitive": false,
        "spoiler_text": "",
        "visibility": visibility(&crate::string_field(Some(row), "visibility").unwrap_or_default()),
        "media_attachments": media_attachments(row),
        "mentions": mentions(row),
        "tags": tags(row),
        "card": Value::Null,
        "poll": poll_json(row),
    })
}

pub(crate) fn plain_text(row: &Map<String, Value>) -> String {
    ["name", "summary", "content"]
        .iter()
        .filter_map(|key| crate::string_field(Some(row), key))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn status_content(row: &Map<String, Value>) -> String {
    let mut parts = Vec::new();
    if let Some(name) = crate::string_field(Some(row), "name") {
        parts.push(format!(
            "<p><strong>{}</strong></p>",
            crate::escape_html(&name)
        ));
    }
    if let Some(summary) = crate::string_field(Some(row), "summary") {
        parts.push(format!("<p>{}</p>", crate::escape_html(&summary)));
    }
    parts.push(
        crate::string_field(Some(row), "content_html").unwrap_or_else(|| {
            crate::escape_html(&crate::string_field(Some(row), "content").unwrap_or_default())
        }),
    );
    parts.join("")
}

pub(crate) fn poll_json(row: &Map<String, Value>) -> Value {
    if crate::string_field(Some(row), "object_type").as_deref() != Some("Question") {
        return Value::Null;
    }
    let Some(raw) = row.get("poll_options") else {
        return Value::Null;
    };
    let parsed = match raw {
        Value::String(text) => serde_json::from_str::<Value>(text).ok(),
        value => Some(value.clone()),
    };
    let Some(parsed) = parsed else {
        return Value::Null;
    };
    let options = parsed
        .get("options")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if options.is_empty() {
        return Value::Null;
    }
    serde_json::json!({
        "id": format!("{}#poll", crate::string_field(Some(row), "id").unwrap_or_default()),
        "expires_at": Value::Null,
        "expired": false,
        "multiple": parsed.get("multiple").and_then(Value::as_bool).unwrap_or(false),
        "votes_count": 0,
        "voters_count": 0,
        "voted": false,
        "own_votes": [],
        "options": options.into_iter().map(|title| serde_json::json!({ "title": title, "votes_count": 0 })).collect::<Vec<_>>(),
        "emojis": [],
    })
}

pub(crate) fn media_attachments(row: &Map<String, Value>) -> Value {
    Value::Array(
        crate::parse_attachment_array(row.get("media_attachments"))
            .into_iter()
            .enumerate()
            .filter_map(|(index, attachment)| {
                let object = attachment.as_object()?;
                let url = crate::string_field(Some(object), "url").unwrap_or_default();
                if url.is_empty() {
                    return None;
                }
                let media_type = crate::string_field(Some(object), "mediaType").unwrap_or_default();
                let attachment_type = if media_type.starts_with("image/") {
                    "image"
                } else if media_type.starts_with("video/") {
                    "video"
                } else {
                    "unknown"
                };
                Some(serde_json::json!({
                    "id": format!("{}#media-{}", crate::string_field(Some(row), "id").unwrap_or_default(), index + 1),
                    "type": attachment_type,
                    "url": url,
                    "preview_url": url,
                    "remote_url": Value::Null,
                    "preview_remote_url": Value::Null,
                    "text_url": Value::Null,
                    "meta": {},
                    "description": crate::string_field(Some(object), "name").map(Value::String).unwrap_or(Value::Null),
                    "blurhash": Value::Null,
                }))
            })
            .collect(),
    )
}

pub(crate) fn mentions(row: &Map<String, Value>) -> Value {
    let mut seen = Vec::new();
    let mut mentions = Vec::new();
    for token in plain_text(row).split_whitespace() {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | ',' | '.' | ':' | ';' | '!' | '?'
            )
        });
        let Some(rest) = trimmed.strip_prefix('@') else {
            continue;
        };
        let Some((username, host)) = rest.split_once('@') else {
            continue;
        };
        if username.is_empty() || !host.contains('.') {
            continue;
        }
        let host = host.to_ascii_lowercase();
        let acct = format!("{username}@{host}");
        if seen.iter().any(|value: &String| value == &acct) {
            continue;
        }
        seen.push(acct.clone());
        mentions.push(serde_json::json!({
            "id": format!("https://{host}/@{username}"),
            "username": username,
            "acct": acct,
            "url": format!("https://{host}/@{username}"),
        }));
    }
    Value::Array(mentions)
}

pub(crate) fn tags(row: &Map<String, Value>) -> Value {
    let mut seen = Vec::new();
    let mut tags = Vec::new();
    for token in plain_text(row).split_whitespace() {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '(' | ')' | '[' | ']' | ',' | '.' | ':' | ';' | '!' | '?'
            )
        });
        let Some(name) = trimmed.strip_prefix('#') else {
            continue;
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
        {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if seen.iter().any(|value: &String| value == &key) {
            continue;
        }
        seen.push(key);
        tags.push(serde_json::json!({
            "name": name,
            "url": format!("https://social.dais.social/tags/{name}"),
        }));
    }
    Value::Array(tags)
}

pub(crate) fn notification_type(value: Option<&str>) -> String {
    match value {
        Some("like") => "favourite".to_string(),
        Some("boost") => "reblog".to_string(),
        Some(value) if !value.is_empty() => value.to_string(),
        _ => "mention".to_string(),
    }
}

pub(crate) fn remote_account_json(row: &Map<String, Value>) -> Value {
    let url = crate::string_field(Some(row), "url")
        .or_else(|| crate::string_field(Some(row), "actor_id"))
        .unwrap_or_default();
    let (username, acct) = parse_actor_acct(&url);
    serde_json::json!({
        "id": url,
        "username": username,
        "acct": acct,
        "display_name": username,
        "locked": false,
        "bot": false,
        "discoverable": false,
        "group": false,
        "created_at": crate::string_field(Some(row), "created_at").unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string()),
        "note": "",
        "url": url,
        "avatar": "",
        "avatar_static": "",
        "header": "",
        "header_static": "",
        "followers_count": 0,
        "following_count": 0,
        "statuses_count": 0,
        "fields": [],
        "emojis": [],
    })
}

pub(crate) fn parse_actor_acct(actor_url: &str) -> (String, String) {
    match worker::Url::parse(actor_url) {
        Ok(url) => {
            let username = url
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(ToOwned::to_owned))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| url.host_str().unwrap_or(actor_url).to_string());
            let host = url.host_str().unwrap_or_default();
            (username.clone(), format!("{username}@{host}"))
        }
        Err(_) => (actor_url.to_string(), actor_url.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_collections_without_accepting_extra_segments() {
        assert!(account_statuses_path("/api/v1/accounts/alice/statuses"));
        assert!(account_followers_path("/api/v1/accounts/alice/followers"));
        assert!(account_following_path("/api/v1/accounts/alice/following"));
        assert!(!account_statuses_path(
            "/api/v1/accounts/alice/statuses/more"
        ));
        assert!(!account_statuses_path("/api/v1/accounts//statuses"));
    }

    #[test]
    fn parses_account_actions_allowlist() {
        assert_eq!(
            account_action_path("/api/v1/accounts/bob/mute"),
            Some(("bob".to_string(), "mute".to_string()))
        );
        assert_eq!(
            account_action_path("/api/v1/accounts/bob/unfollow"),
            Some(("bob".to_string(), "unfollow".to_string()))
        );
        assert_eq!(account_action_path("/api/v1/accounts/bob/admin"), None);
        assert_eq!(account_action_path("/api/v1/accounts/bob/mute/extra"), None);
    }

    #[test]
    fn parses_status_paths_and_actions() {
        assert_eq!(status_path("/api/v1/statuses/123"), Some("123".to_string()));
        assert_eq!(
            status_context_path("/api/v1/statuses/123/context"),
            Some("123".to_string())
        );
        assert_eq!(
            status_source_path("/api/v1/statuses/123/source"),
            Some("123".to_string())
        );
        assert_eq!(
            status_action_path("/api/v1/statuses/123/favourite"),
            Some(("123".to_string(), "favourite".to_string()))
        );
        assert_eq!(status_path("/api/v1/statuses/123/context"), None);
        assert_eq!(status_action_path("/api/v1/statuses/123/delete"), None);
    }

    #[test]
    fn parses_misc_client_probe_paths() {
        assert!(follow_request_action(
            "/api/v1/follow_requests/abc/authorize"
        ));
        assert!(follow_request_action("/api/v1/follow_requests/abc/reject"));
        assert!(!follow_request_action("/api/v1/follow_requests/abc/ignore"));
        assert!(suggestion_dismiss("/api/v1/suggestions/example"));
        assert!(!suggestion_dismiss("/api/v1/suggestions/example/extra"));
        assert_eq!(
            media_path("/api/v2/media/media-1"),
            Some("media-1".to_string())
        );
        assert_eq!(
            notification_dismiss_path("/api/v1/notifications/n1/dismiss"),
            Some("n1".to_string())
        );
    }

    #[test]
    fn formats_status_content_mentions_and_tags() {
        let value = serde_json::json!({
            "name": "Launch <day>",
            "summary": "Public note",
            "content": "Hello @ada@example.com and @ada@example.com about #Space and #space.",
            "visibility": "public",
        });
        let row = value.as_object().unwrap();

        assert_eq!(
            status_content(row),
            "<p><strong>Launch &lt;day&gt;</strong></p><p>Public note</p>Hello @ada@example.com and @ada@example.com about #Space and #space."
        );
        assert_eq!(
            plain_text(row),
            "Launch <day>\n\nPublic note\n\nHello @ada@example.com and @ada@example.com about #Space and #space."
        );

        let mention_items = mentions(row).as_array().cloned().unwrap();
        assert_eq!(mention_items.len(), 1);
        assert_eq!(
            mention_items[0].get("acct").and_then(Value::as_str),
            Some("ada@example.com")
        );

        let tag_items = tags(row).as_array().cloned().unwrap();
        assert_eq!(tag_items.len(), 1);
        assert_eq!(
            tag_items[0].get("name").and_then(Value::as_str),
            Some("Space")
        );
    }

    #[test]
    fn formats_poll_and_media_attachments() {
        let value = serde_json::json!({
            "id": "post-1",
            "object_type": "Question",
            "poll_options": {
                "multiple": true,
                "options": ["Earth", "Mars", ""]
            },
            "media_attachments": [
                {
                    "url": "https://social.dais.social/media/uploads/image.jpg",
                    "mediaType": "image/jpeg",
                    "name": "Alt text"
                },
                {
                    "url": "https://social.dais.social/media/uploads/movie.mp4",
                    "mediaType": "video/mp4"
                }
            ]
        });
        let row = value.as_object().unwrap();

        let poll = poll_json(row);
        assert_eq!(poll.get("id").and_then(Value::as_str), Some("post-1#poll"));
        assert_eq!(poll.get("multiple").and_then(Value::as_bool), Some(true));
        assert_eq!(
            poll.get("options").and_then(Value::as_array).map(Vec::len),
            Some(2)
        );

        let attachments = media_attachments(row).as_array().cloned().unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(
            attachments[0].get("type").and_then(Value::as_str),
            Some("image")
        );
        assert_eq!(
            attachments[1].get("type").and_then(Value::as_str),
            Some("video")
        );
    }

    #[test]
    fn formats_remote_account_and_notification_type() {
        let (username, acct) = parse_actor_acct("https://example.com/users/ada");
        assert_eq!(username, "ada");
        assert_eq!(acct, "ada@example.com");
        assert_eq!(notification_type(Some("like")), "favourite");
        assert_eq!(notification_type(Some("boost")), "reblog");
        assert_eq!(notification_type(None), "mention");

        let value = serde_json::json!({
            "actor_id": "https://example.com/users/ada",
            "created_at": "2026-07-04T12:00:00Z"
        });
        let account = remote_account_json(value.as_object().unwrap());
        assert_eq!(account.get("username").and_then(Value::as_str), Some("ada"));
        assert_eq!(
            account.get("acct").and_then(Value::as_str),
            Some("ada@example.com")
        );
        assert_eq!(
            account.get("created_at").and_then(Value::as_str),
            Some("2026-07-04T12:00:00Z")
        );
    }
}
