//! Wire actions — the operations that talk to prod D1 and federate over the network.
//! These are what bring the client to parity with the Python CLI so it can replace it.
//!
//! Every D1 write here is **parameterized** (no string-interpolated SQL — closing the
//! injection class the Python CLI's f-string SQL had), and all delivery goes through
//! core's audited `deliver_to_inbox` via [`crate::federation`].

use chrono::Utc;
use dais_core::traits::DatabaseProvider;
use serde_json::Value;

use crate::api::Client;
use crate::error::{Error, Result};
use crate::federation;
use crate::model::{Feed, Post, Visibility};
use crate::platform::{D1Db, ReqwestHttp};

/// Result of publishing a post.
pub struct PublishOutcome {
    pub post_id: String,
    pub followers_targeted: usize,
    pub delivered: usize,
    pub failed: usize,
}

/// A pending follow request read from prod D1.
pub struct RemoteRequest {
    pub follower_actor_id: String,
    pub follower_inbox: String,
    pub created_at: String,
}

impl Client {
    // ---- shared helpers --------------------------------------------------

    /// Our own actor URL, derived from the configured keyId (minus the fragment).
    pub fn actor_url(&self) -> Result<String> {
        let kid = self
            .config
            .keys
            .key_id
            .as_deref()
            .ok_or_else(|| Error::NotConfigured("keys.key_id".into()))?;
        Ok(kid.split('#').next().unwrap_or(kid).to_string())
    }

    fn db(&self) -> Result<D1Db> {
        D1Db::from_config(&self.config.d1)
    }

    fn http(&self) -> ReqwestHttp {
        ReqwestHttp::new()
    }

    /// True when D1 credentials + a signing key are present (i.e. wire ops can run).
    pub fn can_federate(&self) -> bool {
        self.config.d1.is_complete()
            && self.config.keys.key_id.is_some()
            && self.config.keys.private_key_path.is_some()
    }

    // ---- post ------------------------------------------------------------

    /// Publish a post: persist to D1, deliver `Create(Note)` to follower inboxes,
    /// and mirror into the local Sent feed. Honors private-by-default + E2EE.
    pub async fn publish(
        &self,
        content: &str,
        visibility: Visibility,
        encrypt: bool,
        reply_to: Option<&str>,
    ) -> Result<PublishOutcome> {
        let actor = self.actor_url()?;
        let pk = self.config.read_private_key()?;
        let db = self.db()?;
        let http = self.http();

        // E2EE (encrypt-to-self, v1): the wire/DB carry only the fallback notice.
        let (stored_content, enc_ext) = if encrypt {
            let (enc, fallback) = crate::e2ee::encrypt_to_self(&self.config, content, None)?;
            (fallback, Some(serde_json::to_value(&enc)?))
        } else {
            (content.to_string(), None)
        };

        let (post_id, note) =
            federation::build_note(&actor, &stored_content, visibility, &[], reply_to, enc_ext.as_ref());
        let create = federation::build_create(&actor, &note);
        let activity_json = serde_json::to_string(&create)?;
        let published = note["published"].as_str().unwrap_or_default().to_string();

        db.execute(
            "INSERT INTO posts (id, actor_id, content, visibility, in_reply_to, published_at, protocol)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'activitypub')",
            &[
                Value::String(post_id.clone()),
                Value::String(actor.clone()),
                Value::String(stored_content.clone()),
                Value::String(visibility.as_str().to_string()),
                reply_to.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
                Value::String(published),
            ],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;

        // Deliver to follower inboxes (reuses core's query + signed delivery).
        let inboxes = dais_core::activitypub::get_follower_inboxes(&db, &actor)
            .await
            .map_err(|e| Error::other(e.to_string()))?;
        let mut delivered = 0;
        let mut failed = 0;
        for inbox in &inboxes {
            match federation::deliver(&http, inbox, &actor, &activity_json, &pk).await {
                Ok(()) => delivered += 1,
                Err(_) => failed += 1,
            }
        }

        // Mirror into the local Sent feed for instant read-back.
        let _ = self.store.upsert_post(
            Feed::Sent,
            &Post {
                id: post_id.clone(),
                author_handle: self.config.handle.clone().unwrap_or_default(),
                author_name: Some("You".into()),
                content: stored_content,
                visibility,
                encrypted: encrypt,
                published: Utc::now(),
                in_reply_to: reply_to.map(str::to_string),
                reply_count: 0,
                like_count: 0,
                boost_count: 0,
                is_friend: false,
                unread: false,
            },
        );

        Ok(PublishOutcome {
            post_id,
            followers_targeted: inboxes.len(),
            delivered,
            failed,
        })
    }

    // ---- follow ----------------------------------------------------------

    /// Follow `@user@host`: resolve, persist a pending `following` row, deliver `Follow`.
    pub async fn follow_add(&self, handle: &str) -> Result<String> {
        let actor = self.actor_url()?;
        let pk = self.config.read_private_key()?;
        let http = self.http();
        let target = federation::resolve(&http, handle).await?;
        let (follow_id, follow) = federation::build_follow(&actor, &target.id);

        let db = self.db()?;
        db.execute(
            "INSERT INTO following (id, actor_id, target_actor_id, target_inbox, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            &[
                Value::String(follow_id.clone()),
                Value::String(actor.clone()),
                Value::String(target.id.clone()),
                Value::String(target.inbox.clone()),
                Value::String(Utc::now().to_rfc3339()),
            ],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;

        federation::deliver(&http, &target.inbox, &actor, &serde_json::to_string(&follow)?, &pk).await?;
        Ok(target.id)
    }

    /// Unfollow `@user@host`: deliver `Undo(Follow)` and remove the `following` row.
    pub async fn follow_remove(&self, handle: &str) -> Result<()> {
        let actor = self.actor_url()?;
        let pk = self.config.read_private_key()?;
        let http = self.http();
        let target = federation::resolve(&http, handle).await?;
        let db = self.db()?;

        let rows = db
            .execute(
                "SELECT id, target_inbox FROM following WHERE actor_id = ?1 AND target_actor_id = ?2",
                &[Value::String(actor.clone()), Value::String(target.id.clone())],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        let follow_id = rows
            .first()
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{actor}/activities/unknown"));
        let inbox = rows
            .first()
            .and_then(|r| r.get("target_inbox"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| target.inbox.clone());

        let undo = federation::build_undo_follow(&actor, &follow_id, &target.id);
        federation::deliver(&http, &inbox, &actor, &serde_json::to_string(&undo)?, &pk).await?;

        db.execute(
            "DELETE FROM following WHERE actor_id = ?1 AND target_actor_id = ?2",
            &[Value::String(actor), Value::String(target.id)],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;
        Ok(())
    }

    pub async fn following_list(&self) -> Result<Vec<(String, String)>> {
        let actor = self.actor_url()?;
        let rows = self
            .db()?
            .execute(
                "SELECT target_actor_id, status FROM following WHERE actor_id = ?1 ORDER BY created_at DESC",
                &[Value::String(actor)],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some((
                    r.get("target_actor_id")?.as_str()?.to_string(),
                    r.get("status").and_then(|v| v.as_str()).unwrap_or("pending").to_string(),
                ))
            })
            .collect())
    }

    // ---- follow requests (the approval inbox) ----------------------------

    /// Pending follow requests from prod D1 (`followers` rows with status `pending`).
    pub async fn requests_remote(&self) -> Result<Vec<RemoteRequest>> {
        let actor = self.actor_url()?;
        let rows = self
            .db()?
            .execute(
                "SELECT follower_actor_id, follower_inbox, created_at FROM followers
                 WHERE actor_id = ?1 AND status = 'pending' ORDER BY created_at DESC",
                &[Value::String(actor)],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(RemoteRequest {
                    follower_actor_id: r.get("follower_actor_id")?.as_str()?.to_string(),
                    follower_inbox: r
                        .get("follower_inbox")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    created_at: r
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect())
    }

    /// Approve a follower (`@user@host`): mark approved + deliver `Accept`.
    pub async fn request_approve(&self, handle: &str) -> Result<()> {
        let actor = self.actor_url()?;
        let pk = self.config.read_private_key()?;
        let http = self.http();
        let follower = federation::resolve(&http, handle).await?;
        let db = self.db()?;

        let rows = db
            .execute(
                "SELECT id, follower_inbox FROM followers WHERE actor_id = ?1 AND follower_actor_id = ?2",
                &[Value::String(actor.clone()), Value::String(follower.id.clone())],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        let row = rows
            .first()
            .ok_or_else(|| Error::other(format!("no pending follow request from {handle}")))?;
        let follow_id = row.get("id").and_then(|v| v.as_str()).map(str::to_string);
        let inbox = row
            .get("follower_inbox")
            .and_then(|v| v.as_str())
            .unwrap_or(&follower.inbox)
            .to_string();

        db.execute(
            "UPDATE followers SET status = 'approved' WHERE actor_id = ?1 AND follower_actor_id = ?2",
            &[Value::String(actor.clone()), Value::String(follower.id.clone())],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;

        let accept = federation::build_accept(&actor, &follower.id, follow_id.as_deref());
        federation::deliver(&http, &inbox, &actor, &serde_json::to_string(&accept)?, &pk).await?;
        Ok(())
    }

    /// Reject a follower: deliver `Reject` + remove the `followers` row.
    pub async fn request_reject(&self, handle: &str) -> Result<()> {
        let actor = self.actor_url()?;
        let pk = self.config.read_private_key()?;
        let http = self.http();
        let follower = federation::resolve(&http, handle).await?;
        let db = self.db()?;

        let rows = db
            .execute(
                "SELECT follower_inbox FROM followers WHERE actor_id = ?1 AND follower_actor_id = ?2",
                &[Value::String(actor.clone()), Value::String(follower.id.clone())],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        let inbox = rows
            .first()
            .and_then(|r| r.get("follower_inbox"))
            .and_then(|v| v.as_str())
            .unwrap_or(&follower.inbox)
            .to_string();

        let reject = federation::build_reject(&actor, &follower.id, None);
        federation::deliver(&http, &inbox, &actor, &serde_json::to_string(&reject)?, &pk).await?;

        db.execute(
            "DELETE FROM followers WHERE actor_id = ?1 AND follower_actor_id = ?2",
            &[Value::String(actor), Value::String(follower.id)],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;
        Ok(())
    }

    /// Remove (kick) an approved follower: delete the `followers` row.
    pub async fn follower_remove(&self, handle: &str) -> Result<()> {
        let actor = self.actor_url()?;
        let follower = federation::resolve(&self.http(), handle).await?;
        self.db()?
            .execute(
                "DELETE FROM followers WHERE actor_id = ?1 AND follower_actor_id = ?2",
                &[Value::String(actor), Value::String(follower.id)],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(())
    }

    pub async fn followers_list(&self) -> Result<Vec<String>> {
        let actor = self.actor_url()?;
        let rows = self
            .db()?
            .execute(
                "SELECT follower_actor_id FROM followers WHERE actor_id = ?1 AND status = 'approved'
                 ORDER BY created_at DESC",
                &[Value::String(actor)],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.get("follower_actor_id").and_then(|v| v.as_str()).map(str::to_string))
            .collect())
    }

    // ---- direct messages -------------------------------------------------

    /// Send a DM to `@user@host` (Direct-addressed Note), optionally E2EE to them.
    pub async fn dm_send(&self, handle: &str, content: &str, encrypt: bool) -> Result<String> {
        let actor = self.actor_url()?;
        let pk = self.config.read_private_key()?;
        let http = self.http();
        let recipient = federation::resolve(&http, handle).await?;
        let db = self.db()?;

        let (stored_content, enc_ext) = if encrypt {
            let key_id = recipient
                .key_id
                .clone()
                .ok_or_else(|| Error::other(format!("{handle} publishes no key; cannot encrypt")))?;
            let pem = recipient
                .public_key_pem
                .clone()
                .ok_or_else(|| Error::other(format!("{handle} publishes no key; cannot encrypt")))?;
            let enc = crate::e2ee::encrypt_message(content, &[(key_id, pem)]).map_err(Error::Crypto)?;
            (crate::e2ee::fallback_content(None), Some(serde_json::to_value(&enc)?))
        } else {
            (content.to_string(), None)
        };

        let (note_id, note) = federation::build_note(
            &actor,
            &stored_content,
            Visibility::Direct,
            &[recipient.id.clone()],
            None,
            enc_ext.as_ref(),
        );
        let create = federation::build_create(&actor, &note);

        // Persist conversation + message (deterministic conversation id over participants).
        let mut participants = [actor.clone(), recipient.id.clone()];
        participants.sort();
        let conv_id = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            participants.join("|").as_bytes(),
        )
        .to_string();
        let now = Utc::now().to_rfc3339();

        db.execute(
            "INSERT INTO conversations (id, participants, last_message_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET last_message_at = excluded.last_message_at",
            &[
                Value::String(conv_id.clone()),
                Value::String(serde_json::to_string(&participants)?),
                Value::String(now.clone()),
            ],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;
        db.execute(
            "INSERT INTO direct_messages (id, conversation_id, sender_id, content, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            &[
                Value::String(note_id.clone()),
                Value::String(conv_id),
                Value::String(actor.clone()),
                Value::String(stored_content),
                Value::String(now),
            ],
        )
        .await
        .map_err(|e| Error::D1(e.to_string()))?;

        federation::deliver(&http, &recipient.inbox, &actor, &serde_json::to_string(&create)?, &pk).await?;
        Ok(note_id)
    }

    fn conversation_id(&self, other_actor: &str) -> Result<String> {
        let actor = self.actor_url()?;
        let mut participants = [actor, other_actor.to_string()];
        participants.sort();
        Ok(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, participants.join("|").as_bytes()).to_string())
    }

    /// List DM conversations (id, last_message_at), newest first.
    pub async fn dm_list(&self) -> Result<Vec<(String, String)>> {
        let rows = self
            .db()?
            .execute(
                "SELECT id, last_message_at FROM conversations ORDER BY last_message_at DESC",
                &[],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some((
                    r.get("id")?.as_str()?.to_string(),
                    r.get("last_message_at").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                ))
            })
            .collect())
    }

    /// Read a DM thread with `@user@host` (sender, content, published_at).
    pub async fn dm_thread(&self, handle: &str) -> Result<Vec<(String, String, String)>> {
        let other = federation::resolve(&self.http(), handle).await?;
        let conv = self.conversation_id(&other.id)?;
        let rows = self
            .db()?
            .execute(
                "SELECT sender_id, content, published_at FROM direct_messages
                 WHERE conversation_id = ?1 ORDER BY published_at ASC",
                &[Value::String(conv)],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get("sender_id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    r.get("content").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                    r.get("published_at").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                )
            })
            .collect())
    }

    // ---- notifications ---------------------------------------------------

    pub async fn notify_list(&self, limit: usize) -> Result<Vec<(String, String, String)>> {
        let rows = self
            .db()?
            .execute(
                "SELECT type, actor_id, content, created_at FROM notifications
                 ORDER BY created_at DESC LIMIT ?1",
                &[Value::Number(serde_json::Number::from(limit as u64))],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let typ = r.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let who = r.get("actor_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let when = r.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                (typ, who, when)
            })
            .collect())
    }

    /// Mark notifications read — one by id, or all when `id` is None.
    pub async fn notify_mark_read(&self, id: Option<&str>) -> Result<()> {
        let db = self.db()?;
        match id {
            Some(nid) => {
                db.execute(
                    "UPDATE notifications SET read = 1 WHERE id = ?1",
                    &[Value::String(nid.to_string())],
                )
                .await
            }
            None => db.execute("UPDATE notifications SET read = 1", &[]).await,
        }
        .map_err(|e| Error::D1(e.to_string()))?;
        Ok(())
    }

    // ---- blocks ----------------------------------------------------------

    /// Block an actor URL/handle or a whole domain (no `@`/`/` → treated as a domain).
    pub async fn block_add(&self, target: &str) -> Result<()> {
        let is_domain = !target.contains('@') && !target.contains('/');
        let blocked_domain = if is_domain { Value::String(target.to_string()) } else { Value::Null };
        self.db()?
            .execute(
                "INSERT INTO blocks (id, actor_id, blocked_domain, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(actor_id) DO NOTHING",
                &[
                    Value::String(uuid::Uuid::new_v4().to_string()),
                    Value::String(target.to_string()),
                    blocked_domain,
                    Value::String(Utc::now().to_rfc3339()),
                ],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(())
    }

    pub async fn block_list(&self) -> Result<Vec<String>> {
        let rows = self
            .db()?
            .execute("SELECT actor_id, blocked_domain FROM blocks ORDER BY created_at DESC", &[])
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.get("actor_id").and_then(|v| v.as_str()).map(str::to_string))
            .collect())
    }

    pub async fn block_remove(&self, target: &str) -> Result<()> {
        self.db()?
            .execute(
                "DELETE FROM blocks WHERE actor_id = ?1",
                &[Value::String(target.to_string())],
            )
            .await
            .map_err(|e| Error::D1(e.to_string()))?;
        Ok(())
    }
}
