use std::collections::BTreeMap;
use std::fs;

use anyhow::{anyhow, Result};

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
    let interaction_activity_id = format!(
        "{actor_id}#{}s/{}",
        interaction,
        activity_suffix(object_id)
    );

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
        })
    }
}

pub async fn publish_post(
    draft: PostDraft,
    store: &ConfigStore,
    db: &D1Client,
) -> Result<PostOutcome> {
    let effective = effective_protocol(draft.protocol, draft.visibility);
    if draft.visibility == Visibility::Direct && draft.to.is_empty() {
        anyhow::bail!("direct posts require at least one --to actor URL");
    }
    if draft.object_type != ActivityObjectType::Note && effective != Protocol::ActivityPub {
        anyhow::bail!("Article and Document posts can only be sent to ActivityPub");
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

fn activity_suffix(value: &str) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(value.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
