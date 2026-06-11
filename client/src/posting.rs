use std::collections::BTreeMap;
use std::fs;

use anyhow::{anyhow, Result};

use crate::atproto::AtprotoClient;
use crate::cli::{CreatePostArgs, E2eeFallbackMode};
use crate::config::ConfigStore;
use crate::d1::D1Client;
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

pub struct PostDraft {
    pub text: String,
    pub visibility: Visibility,
    pub protocol: Protocol,
    pub encrypt: bool,
    pub recipients: BTreeMap<String, String>,
    pub reply_to: Option<String>,
    pub to: Vec<String>,
    pub e2ee_fallback: E2eeFallbackMode,
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
        db.create_encrypted_post(
            &post_id,
            actor_id,
            &fallback_content,
            &draft.visibility.to_string(),
            &published_at,
            &encrypted_json,
            draft.reply_to.as_deref(),
        )
        .await?;

        let activity_json = build_create_activity_json(
            actor_id,
            &post_id,
            &fallback_content,
            &draft.visibility.to_string(),
            &published_at,
            Some(serde_json::to_value(&payload.encrypted_message)?),
            draft.reply_to.as_deref(),
            &draft.to,
        )?;
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
            )
            .await?;

            let activity_json = build_create_activity_json(
                actor_id,
                &post_id,
                &draft.text,
                &draft.visibility.to_string(),
                &published_at,
                None,
                draft.reply_to.as_deref(),
                &draft.to,
            )?;
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
            )
            .await?;

            let activity_json = build_create_activity_json(
                actor_id,
                &post_id,
                &draft.text,
                &draft.visibility.to_string(),
                &published_at,
                None,
                draft.reply_to.as_deref(),
                &draft.to,
            )?;
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

fn build_create_activity_json(
    actor_id: &str,
    post_id: &str,
    content: &str,
    visibility: &str,
    published_at: &str,
    encrypted_message: Option<serde_json::Value>,
    in_reply_to: Option<&str>,
    recipients: &[String],
) -> Result<String> {
    let followers_collection = format!("{actor_id}/followers");
    let to = activity_to(visibility, &followers_collection, recipients);
    let cc = activity_cc(visibility, &followers_collection);

    let mut note = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Note",
        "id": post_id,
        "attributedTo": actor_id,
        "content": content,
        "published": published_at,
        "to": to
    });

    if !cc.is_empty() {
        note["cc"] = serde_json::json!(cc);
    }

    if let Some(in_reply_to) = in_reply_to {
        note["inReplyTo"] = serde_json::json!(in_reply_to);
    }

    if let Some(encrypted_message) = encrypted_message {
        note["encryptedMessage"] = encrypted_message;
    }

    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "id": format!("{post_id}#create"),
        "actor": actor_id,
        "published": published_at,
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
