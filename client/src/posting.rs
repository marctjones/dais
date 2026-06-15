use std::collections::BTreeMap;
use std::fs;

use anyhow::{anyhow, Result};
use reqwest::Url;

use crate::atproto::AtprotoClient;
use crate::cli::{ActivityObjectType, CreatePostArgs, E2eeFallbackMode};
use crate::config::ConfigStore;
use crate::d1::{ActivityDeliveryInsert, D1Client, EncryptedPostInsert};
use crate::e2ee;
use crate::new_local_post_id;
use crate::routing::{effective_protocol, Protocol, Visibility};

pub enum PostOutcome {
    ActivityPub {
        post_id: String,
        read_url: Option<String>,
        split_key_url: Option<String>,
        delivery_ids: Vec<String>,
    },
    Bluesky {
        uri: String,
    },
    Both {
        post_id: String,
        uri: String,
        read_url: Option<String>,
        delivery_ids: Vec<String>,
    },
}

pub struct ActivityOutcome {
    pub activity_id: String,
    pub delivery_ids: Vec<String>,
}

pub struct PostDraft {
    pub text: String,
    pub visibility: Visibility,
    pub protocol: Protocol,
    pub encrypt: bool,
    pub recipients: BTreeMap<String, String>,
    pub reply_to: Option<String>,
    pub to: Vec<String>,
    pub e2ee_fallback: E2eeFallbackMode,
    pub object_type: ActivityObjectType,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub location: Option<String>,
    pub attachments: Vec<String>,
}

pub async fn update_activitypub_post(
    db: &D1Client,
    actor_id: &str,
    post_id: &str,
    content: &str,
) -> Result<ActivityOutcome> {
    db.update_post_content(post_id, content).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let activity_id = format!("{post_id}#updates/{}", activity_suffix(&now));
    let followers = format!("{actor_id}/followers");
    let activity_json = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Update",
        "actor": actor_id,
        "published": now,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [followers],
        "object": {
            "id": post_id,
            "type": "Note",
            "attributedTo": actor_id,
            "content": content,
            "updated": chrono::Utc::now().to_rfc3339(),
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "cc": [format!("{actor_id}/followers")]
        }
    })
    .to_string();

    let delivery_ids = db
        .create_activity_deliveries(ActivityDeliveryInsert {
            post_id,
            actor_id,
            activity_type: "Update",
            activity_json: &activity_json,
            target_inboxes: &[],
        })
        .await?;

    Ok(ActivityOutcome {
        activity_id,
        delivery_ids,
    })
}

pub async fn delete_activitypub_post(
    db: &D1Client,
    actor_id: &str,
    post_id: &str,
) -> Result<ActivityOutcome> {
    let now = chrono::Utc::now().to_rfc3339();
    let activity_id = format!("{post_id}#delete/{}", activity_suffix(&now));
    let activity_json = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Delete",
        "actor": actor_id,
        "published": now,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{actor_id}/followers")],
        "object": post_id
    })
    .to_string();

    let delivery_ids = db
        .create_activity_deliveries(ActivityDeliveryInsert {
            post_id,
            actor_id,
            activity_type: "Delete",
            activity_json: &activity_json,
            target_inboxes: &[],
        })
        .await?;
    db.delete_post(post_id).await?;

    Ok(ActivityOutcome {
        activity_id,
        delivery_ids,
    })
}

pub async fn publish_interaction(
    db: &D1Client,
    actor_id: &str,
    object_id: &str,
    interaction: &str,
    undo: bool,
    target_inbox: Option<String>,
) -> Result<ActivityOutcome> {
    let target_inbox = match target_inbox {
        Some(inbox) => inbox,
        None => resolve_object_inbox(object_id).await?,
    };
    let now = chrono::Utc::now().to_rfc3339();
    let activity_type = match interaction {
        "like" => "Like",
        "boost" => "Announce",
        other => anyhow::bail!("unsupported interaction type {other}"),
    };
    let interaction_activity_id =
        format!("{actor_id}#{}s/{}", interaction, activity_suffix(object_id));

    let (activity_id, delivery_type, activity_json) = if undo {
        let undo_id = format!(
            "{actor_id}#undos/{}/{}",
            interaction,
            activity_suffix(object_id)
        );
        (
            undo_id.clone(),
            "Undo",
            serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": undo_id,
                "type": "Undo",
                "actor": actor_id,
                "published": now,
                "to": ["https://www.w3.org/ns/activitystreams#Public"],
                "cc": [format!("{actor_id}/followers")],
                "object": {
                    "id": interaction_activity_id,
                    "type": activity_type,
                    "actor": actor_id,
                    "object": object_id
                }
            })
            .to_string(),
        )
    } else {
        (
            interaction_activity_id.clone(),
            activity_type,
            serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": interaction_activity_id,
                "type": activity_type,
                "actor": actor_id,
                "published": now,
                "to": ["https://www.w3.org/ns/activitystreams#Public"],
                "cc": [format!("{actor_id}/followers")],
                "object": object_id
            })
            .to_string(),
        )
    };

    let delivery_ids = db
        .create_activity_deliveries(ActivityDeliveryInsert {
            post_id: object_id,
            actor_id,
            activity_type: delivery_type,
            activity_json: &activity_json,
            target_inboxes: &[target_inbox],
        })
        .await?;

    if undo {
        db.remove_interaction(&interaction_activity_id).await?;
    } else {
        db.record_interaction(&interaction_activity_id, interaction, actor_id, object_id)
            .await?;
    }

    Ok(ActivityOutcome {
        activity_id,
        delivery_ids,
    })
}

async fn resolve_object_inbox(object_id: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let object: serde_json::Value = client
        .get(object_id)
        .header("Accept", "application/activity+json, application/ld+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let actor = object
        .get("attributedTo")
        .or_else(|| object.get("actor"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("could not resolve actor from remote object"))?;
    let actor_doc: serde_json::Value = client
        .get(actor)
        .header("Accept", "application/activity+json, application/ld+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    actor_doc
        .get("endpoints")
        .and_then(|value| value.get("sharedInbox"))
        .and_then(|value| value.as_str())
        .or_else(|| actor_doc.get("inbox").and_then(|value| value.as_str()))
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("could not resolve inbox for remote actor {actor}"))
}

struct CreateActivityInput<'a> {
    actor_id: &'a str,
    post_id: &'a str,
    content: &'a str,
    visibility: &'a str,
    published_at: &'a str,
    encrypted_message: Option<serde_json::Value>,
    in_reply_to: Option<&'a str>,
    recipients: &'a [String],
    object_type: ActivityObjectType,
    title: Option<&'a str>,
    summary: Option<&'a str>,
    starts_at: Option<&'a str>,
    ends_at: Option<&'a str>,
    location: Option<&'a str>,
    attachments: &'a [String],
}

impl PostDraft {
    pub fn from_create_args(args: CreatePostArgs) -> Result<Self> {
        let mut recipients = BTreeMap::new();
        for recipient in args.recipients {
            let (key_id, path) = recipient
                .split_once('=')
                .ok_or_else(|| anyhow!("recipient must be in key_id=public_key_pem_file form"))?;
            recipients.insert(key_id.to_string(), fs::read_to_string(path)?);
        }

        Ok(Self {
            text: args.text,
            visibility: if args.public {
                Visibility::Public
            } else {
                args.visibility
            },
            protocol: args.protocol,
            encrypt: args.encrypt,
            recipients,
            reply_to: args.reply_to,
            to: args.to,
            e2ee_fallback: args.e2ee_fallback,
            object_type: args.object_type,
            title: args.title,
            summary: args.summary,
            starts_at: args.starts_at,
            ends_at: args.ends_at,
            location: args.location,
            attachments: args.attachments,
        })
    }
}

pub async fn publish_post(
    draft: PostDraft,
    store: &ConfigStore,
    db: &D1Client,
) -> Result<PostOutcome> {
    let effective = effective_protocol(draft.protocol, draft.visibility);
    validate_media_attachments(&draft.attachments, draft.visibility)?;
    if !draft.attachments.is_empty() && effective != Protocol::ActivityPub {
        anyhow::bail!(
            "media attachments currently require ActivityPub routing; AT Protocol media upload is not implemented yet"
        );
    }
    if draft.visibility == Visibility::Direct && draft.to.is_empty() {
        anyhow::bail!("direct posts require at least one --to actor URL");
    }
    if draft.object_type != ActivityObjectType::Note && effective != Protocol::ActivityPub {
        anyhow::bail!("rich ActivityPub objects can only be sent to ActivityPub");
    }
    if draft.encrypt && draft.object_type != ActivityObjectType::Note {
        anyhow::bail!("encrypted posts currently use Note fallback objects");
    }

    if draft.encrypt {
        if effective != Protocol::ActivityPub {
            anyhow::bail!("encrypted posts can only be sent to ActivityPub");
        }
        let local_post_id = new_local_post_id();
        let post_id = format!("https://social.dais.social/users/social/posts/{local_post_id}");
        let actor_id = "https://social.dais.social/users/social";
        let read_url = format!("https://social.dais.social/messages/{local_post_id}");
        let (payload, content_key) = e2ee::encrypted_note_payload_with_content_key(
            &draft.text,
            &draft.recipients,
            Some(&read_url),
        )?;
        let split_key_url = format!("{read_url}#cek={content_key}");
        let fallback_content = match draft.e2ee_fallback {
            E2eeFallbackMode::Strict | E2eeFallbackMode::SplitChannel => payload.content.clone(),
            E2eeFallbackMode::TrustedServer => e2ee::fallback_content(Some(&split_key_url)),
        };
        let encrypted_json = serde_json::to_string(&payload.encrypted_message)?;
        let published_at = chrono::Utc::now().to_rfc3339();
        let visibility = draft.visibility.to_string();
        db.create_encrypted_post(EncryptedPostInsert {
            id: &post_id,
            actor_id,
            fallback_content: &fallback_content,
            visibility: &visibility,
            published_at: &published_at,
            encrypted_message_json: &encrypted_json,
            in_reply_to: draft.reply_to.as_deref(),
        })
        .await?;

        let activity_json = build_create_activity_json(CreateActivityInput {
            actor_id,
            post_id: &post_id,
            content: &fallback_content,
            visibility: &visibility,
            published_at: &published_at,
            encrypted_message: Some(serde_json::to_value(&payload.encrypted_message)?),
            in_reply_to: draft.reply_to.as_deref(),
            recipients: &draft.to,
            object_type: ActivityObjectType::Note,
            title: None,
            summary: None,
            starts_at: None,
            ends_at: None,
            location: None,
            attachments: &[],
        })?;
        let delivery_ids =
            create_deliveries(db, &post_id, actor_id, &activity_json, &draft).await?;

        return Ok(PostOutcome::ActivityPub {
            post_id,
            read_url: Some(read_url),
            split_key_url: (draft.e2ee_fallback == E2eeFallbackMode::SplitChannel)
                .then_some(split_key_url),
            delivery_ids,
        });
    }

    match effective {
        Protocol::Atproto => {
            let mut client = AtprotoClient::from_config(&store.load_bluesky()?)?;
            let created = client.create_post(&draft.text).await?;
            Ok(PostOutcome::Bluesky { uri: created.uri })
        }
        Protocol::Both => {
            let local_post_id = new_local_post_id();
            let post_id = format!("https://social.dais.social/users/social/posts/{local_post_id}");
            let actor_id = "https://social.dais.social/users/social";
            let published_at = chrono::Utc::now().to_rfc3339();
            db.create_post(
                &post_id,
                actor_id,
                &draft.text,
                &draft.visibility.to_string(),
                &published_at,
                draft.reply_to.as_deref(),
                draft.object_type,
                draft.title.as_deref(),
                draft.summary.as_deref(),
                draft.starts_at.as_deref(),
                draft.ends_at.as_deref(),
                draft.location.as_deref(),
                attachment_json(&draft.attachments)?.as_deref(),
            )
            .await?;

            let activity_json = build_create_activity_json(CreateActivityInput {
                actor_id,
                post_id: &post_id,
                content: &draft.text,
                visibility: &draft.visibility.to_string(),
                published_at: &published_at,
                encrypted_message: None,
                in_reply_to: draft.reply_to.as_deref(),
                recipients: &draft.to,
                object_type: draft.object_type,
                title: draft.title.as_deref(),
                summary: draft.summary.as_deref(),
                starts_at: draft.starts_at.as_deref(),
                ends_at: draft.ends_at.as_deref(),
                location: draft.location.as_deref(),
                attachments: &draft.attachments,
            })?;
            let delivery_ids =
                create_deliveries(db, &post_id, actor_id, &activity_json, &draft).await?;

            let mut client = AtprotoClient::from_config(&store.load_bluesky()?)?;
            let created = client.create_post(&draft.text).await?;
            Ok(PostOutcome::Both {
                post_id,
                uri: created.uri,
                read_url: None,
                delivery_ids,
            })
        }
        Protocol::ActivityPub => {
            let local_post_id = new_local_post_id();
            let post_id = format!("https://social.dais.social/users/social/posts/{local_post_id}");
            let actor_id = "https://social.dais.social/users/social";
            let published_at = chrono::Utc::now().to_rfc3339();
            db.create_post(
                &post_id,
                actor_id,
                &draft.text,
                &draft.visibility.to_string(),
                &published_at,
                draft.reply_to.as_deref(),
                draft.object_type,
                draft.title.as_deref(),
                draft.summary.as_deref(),
                draft.starts_at.as_deref(),
                draft.ends_at.as_deref(),
                draft.location.as_deref(),
                attachment_json(&draft.attachments)?.as_deref(),
            )
            .await?;

            let activity_json = build_create_activity_json(CreateActivityInput {
                actor_id,
                post_id: &post_id,
                content: &draft.text,
                visibility: &draft.visibility.to_string(),
                published_at: &published_at,
                encrypted_message: None,
                in_reply_to: draft.reply_to.as_deref(),
                recipients: &draft.to,
                object_type: draft.object_type,
                title: draft.title.as_deref(),
                summary: draft.summary.as_deref(),
                starts_at: draft.starts_at.as_deref(),
                ends_at: draft.ends_at.as_deref(),
                location: draft.location.as_deref(),
                attachments: &draft.attachments,
            })?;
            let delivery_ids =
                create_deliveries(db, &post_id, actor_id, &activity_json, &draft).await?;

            Ok(PostOutcome::ActivityPub {
                post_id,
                read_url: None,
                split_key_url: None,
                delivery_ids,
            })
        }
    }
}

async fn create_deliveries(
    db: &D1Client,
    post_id: &str,
    actor_id: &str,
    activity_json: &str,
    draft: &PostDraft,
) -> Result<Vec<String>> {
    if draft.visibility == Visibility::Direct {
        return db
            .create_direct_deliveries(post_id, actor_id, activity_json, &draft.to)
            .await;
    }

    db.create_follower_deliveries(post_id, actor_id, activity_json)
        .await
}

fn build_create_activity_json(input: CreateActivityInput<'_>) -> Result<String> {
    let followers_collection = format!("{}/followers", input.actor_id);
    let to = activity_to(input.visibility, &followers_collection, input.recipients);
    let cc = activity_cc(input.visibility, &followers_collection);

    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": input.object_type.to_string(),
        "id": input.post_id,
        "attributedTo": input.actor_id,
        "content": input.content,
        "published": input.published_at,
        "to": to
    });

    if !cc.is_empty() {
        note["cc"] = serde_json::json!(cc);
    }

    if let Some(title) = input.title {
        note["name"] = serde_json::json!(title);
    }

    if let Some(summary) = input.summary {
        note["summary"] = serde_json::json!(summary);
    }

    if let Some(starts_at) = input.starts_at {
        note["startTime"] = serde_json::json!(starts_at);
    }

    if let Some(ends_at) = input.ends_at {
        note["endTime"] = serde_json::json!(ends_at);
    }

    if let Some(location) = input.location {
        note["location"] = serde_json::json!({
            "type": "Place",
            "name": location
        });
    }

    if !input.attachments.is_empty() {
        note["attachment"] = serde_json::json!(attachment_values(input.attachments)?);
    }

    let tags = activity_tags(input.content);
    if !tags.is_empty() {
        note["tag"] = serde_json::json!(tags);
    }

    if let Some(in_reply_to) = input.in_reply_to {
        note["inReplyTo"] = serde_json::json!(in_reply_to);
    }

    if let Some(encrypted_message) = input.encrypted_message {
        note["encryptedMessage"] = encrypted_message;
    }

    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "id": format!("{}#create", input.post_id),
        "actor": input.actor_id,
        "published": input.published_at,
        "to": note["to"].clone(),
        "cc": note.get("cc").cloned().unwrap_or_else(|| serde_json::json!([])),
        "object": note
    });

    Ok(serde_json::to_string(&activity)?)
}

fn activity_to(visibility: &str, followers_collection: &str, recipients: &[String]) -> Vec<String> {
    match visibility {
        "public" | "unlisted" => vec!["https://www.w3.org/ns/activitystreams#Public".to_string()],
        "direct" => recipients.to_vec(),
        _ => vec![followers_collection.to_string()],
    }
}

fn activity_cc(visibility: &str, followers_collection: &str) -> Vec<String> {
    match visibility {
        "public" | "unlisted" => vec![followers_collection.to_string()],
        _ => Vec::new(),
    }
}

fn attachment_json(attachments: &[String]) -> Result<Option<String>> {
    if attachments.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string(&attachment_values(
        attachments,
    )?)?))
}

fn attachment_values(attachments: &[String]) -> Result<Vec<serde_json::Value>> {
    attachments
        .iter()
        .map(|attachment| {
            if attachment.trim_start().starts_with('{') {
                serde_json::from_str(attachment)
                    .map_err(|error| anyhow!("invalid attachment JSON: {error}"))
            } else {
                Ok(serde_json::json!({
                    "type": "Document",
                    "url": attachment
                }))
            }
        })
        .collect()
}

fn validate_media_attachments(attachments: &[String], visibility: Visibility) -> Result<()> {
    if attachments.is_empty() {
        return Ok(());
    }

    let values = attachment_values(attachments)?;
    if matches!(visibility, Visibility::Followers | Visibility::Direct)
        && !values.iter().all(is_private_media_attachment)
    {
        anyhow::bail!(
            "followers-only and direct media attachments must use private media upload URLs"
        );
    }

    if visibility == Visibility::Public || visibility == Visibility::Unlisted {
        return Ok(());
    }

    Ok(())
}

fn is_private_media_attachment(attachment: &serde_json::Value) -> bool {
    let Some(url) = attachment.get("url").and_then(serde_json::Value::as_str) else {
        return false;
    };
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some("social.dais.social")
        && parsed.path().starts_with("/media/_private/")
}

fn activity_tags(content: &str) -> Vec<serde_json::Value> {
    let mut tags = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in content.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| {
            matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\''
            )
        });
        if let Some(tag) = hashtag_tag(trimmed) {
            if seen.insert(format!("hashtag:{trimmed}")) {
                tags.push(tag);
            }
            continue;
        }
        if let Some(tag) = mention_tag(trimmed) {
            if seen.insert(format!("mention:{trimmed}")) {
                tags.push(tag);
            }
        }
    }
    tags
}

fn hashtag_tag(token: &str) -> Option<serde_json::Value> {
    let name = token.strip_prefix('#')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(serde_json::json!({
        "type": "Hashtag",
        "name": format!("#{name}"),
        "href": format!("https://social.dais.social/tags/{name}")
    }))
}

fn mention_tag(token: &str) -> Option<serde_json::Value> {
    let without_prefix = token.strip_prefix('@')?;
    let mut parts = without_prefix.split('@');
    let username = parts.next()?.trim();
    let host = parts.next()?.trim();
    if parts.next().is_some()
        || username.is_empty()
        || host.is_empty()
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
    {
        return None;
    }
    let href = format!("https://{host}/users/{username}");
    Some(serde_json::json!({
        "type": "Mention",
        "name": format!("@{username}@{host}"),
        "href": href
    }))
}

fn activity_suffix(value: &str) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(value.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        activity_tags, build_create_activity_json, validate_media_attachments, CreateActivityInput,
    };
    use crate::cli::ActivityObjectType;
    use crate::routing::Visibility;

    #[test]
    fn followers_media_requires_private_capability_url() {
        let attachments = vec!["https://social.dais.social/media/uploads/public.png".to_string()];
        let error = validate_media_attachments(&attachments, Visibility::Followers)
            .expect_err("public media must not be valid for followers posts");
        assert!(error
            .to_string()
            .contains("must use private media upload URLs"));
    }

    #[test]
    fn followers_media_allows_private_capability_url() {
        let attachments =
            vec!["https://social.dais.social/media/_private/token/image.png".to_string()];
        validate_media_attachments(&attachments, Visibility::Followers).unwrap();
    }

    #[test]
    fn activity_tags_extract_mentions_and_hashtags() {
        let tags = activity_tags("hello @alice@example.social #Dais");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0]["type"], "Mention");
        assert_eq!(tags[0]["name"], "@alice@example.social");
        assert_eq!(tags[1]["type"], "Hashtag");
        assert_eq!(tags[1]["name"], "#Dais");
    }

    #[test]
    fn create_activity_includes_mastodon_tag_shapes() {
        let json = build_create_activity_json(CreateActivityInput {
            actor_id: "https://social.dais.social/users/social",
            post_id: "https://social.dais.social/users/social/posts/1",
            content: "hello @alice@example.social #Dais",
            visibility: "followers",
            published_at: "2026-06-15T00:00:00Z",
            encrypted_message: None,
            in_reply_to: None,
            recipients: &[],
            object_type: ActivityObjectType::Note,
            title: None,
            summary: None,
            starts_at: None,
            ends_at: None,
            location: None,
            attachments: &[],
        })
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["object"]["tag"][0]["type"], "Mention");
        assert_eq!(value["object"]["tag"][1]["type"], "Hashtag");
    }
}
