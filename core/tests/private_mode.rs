use async_trait::async_trait;
use dais_core::activitypub;
use dais_core::traits::{
    DatabaseDialect, DatabaseProvider, HttpProvider, PlatformResult, Request, Response, Row,
    Statement,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct FakeDb {
    state: Arc<Mutex<FakeDbState>>,
}

#[derive(Default)]
struct FakeDbState {
    actors: HashMap<String, Row>,
    posts: Vec<Row>,
    pending_following: HashSet<String>,
    approved_following: HashSet<String>,
    approved_followers: HashSet<String>,
    friends_rows: Vec<Row>,
    home_rows: Vec<Row>,
    timeline_posts: HashMap<String, Row>,
    replies: HashMap<String, Row>,
    conversations: HashMap<String, Row>,
    direct_messages: HashMap<String, Row>,
    e2ee_conversations: HashMap<String, Row>,
    e2ee_messages: HashMap<String, Row>,
    participants: HashSet<(String, String)>,
    notifications: Vec<Row>,
    closed_network: bool,
    allowlisted_hosts: HashSet<String>,
}

impl FakeDb {
    fn set_closed_network(&self, enabled: bool) {
        self.state.lock().unwrap().closed_network = enabled;
    }

    fn allow_host(&self, host: &str) {
        self.state
            .lock()
            .unwrap()
            .allowlisted_hosts
            .insert(host.to_string());
    }

    fn approve_follower(&self, actor_id: &str) {
        self.state
            .lock()
            .unwrap()
            .approved_followers
            .insert(actor_id.to_string());
    }

    fn approve_following(&self, actor_id: &str) {
        self.state
            .lock()
            .unwrap()
            .approved_following
            .insert(actor_id.to_string());
    }

    fn request_following(&self, actor_id: &str) {
        self.state
            .lock()
            .unwrap()
            .pending_following
            .insert(actor_id.to_string());
    }

    fn pending_following(&self, actor_id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .pending_following
            .contains(actor_id)
    }

    fn accepted_following(&self, actor_id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .approved_following
            .contains(actor_id)
    }

    fn insert_actor(&self, username: &str, actor_id: &str) {
        let mut row = Row::new();
        row.insert("id".to_string(), Value::String(actor_id.to_string()));
        row.insert("username".to_string(), Value::String(username.to_string()));
        self.state
            .lock()
            .unwrap()
            .actors
            .insert(username.to_string(), row);
    }

    fn insert_post(
        &self,
        actor_id: &str,
        post_id: &str,
        content: &str,
        visibility: &str,
        encrypted_message: Option<&str>,
    ) {
        let mut row = Row::new();
        row.insert("id".to_string(), Value::String(post_id.to_string()));
        row.insert("actor_id".to_string(), Value::String(actor_id.to_string()));
        row.insert("content".to_string(), Value::String(content.to_string()));
        row.insert(
            "content_html".to_string(),
            Value::String(content.to_string()),
        );
        row.insert(
            "visibility".to_string(),
            Value::String(visibility.to_string()),
        );
        row.insert(
            "published_at".to_string(),
            Value::String("2026-06-11T00:00:00Z".to_string()),
        );
        row.insert("in_reply_to".to_string(), Value::Null);
        row.insert("media_attachments".to_string(), Value::Null);
        row.insert("atproto_uri".to_string(), Value::Null);
        row.insert(
            "encrypted_message".to_string(),
            encrypted_message
                .map(|value| Value::String(value.to_string()))
                .unwrap_or(Value::Null),
        );
        self.state.lock().unwrap().posts.push(row);
    }

    fn set_friends_rows(&self, rows: Vec<Row>) {
        self.state.lock().unwrap().friends_rows = rows;
    }

    fn set_home_rows(&self, rows: Vec<Row>) {
        self.state.lock().unwrap().home_rows = rows;
    }

    fn timeline_post(&self, object_id: &str) -> Option<Row> {
        self.state
            .lock()
            .unwrap()
            .timeline_posts
            .get(object_id)
            .cloned()
    }

    fn direct_message(&self, message_id: &str) -> Option<Row> {
        self.state
            .lock()
            .unwrap()
            .direct_messages
            .get(message_id)
            .cloned()
    }

    fn e2ee_message(&self, message_id: &str) -> Option<Row> {
        self.state
            .lock()
            .unwrap()
            .e2ee_messages
            .get(message_id)
            .cloned()
    }

    fn reply(&self, reply_id: &str) -> Option<Row> {
        self.state.lock().unwrap().replies.get(reply_id).cloned()
    }

    fn conversation(&self, conversation_id: &str) -> Option<Row> {
        self.state
            .lock()
            .unwrap()
            .conversations
            .get(conversation_id)
            .cloned()
    }

    fn participant_count(&self) -> usize {
        self.state.lock().unwrap().participants.len()
    }
}

#[async_trait(?Send)]
impl DatabaseProvider for FakeDb {
    async fn execute(&self, sql: &str, params: &[Value]) -> PlatformResult<Vec<Row>> {
        let mut state = self.state.lock().unwrap();

        if sql.contains("SELECT id FROM actors WHERE username = ?1") {
            let username = params.first().and_then(Value::as_str).unwrap_or_default();
            return Ok(state
                .actors
                .get(username)
                .cloned()
                .map(|row| vec![row])
                .unwrap_or_default());
        }

        if sql.contains("SELECT closed_network FROM instance_settings") {
            let mut row = Row::new();
            row.insert(
                "closed_network".to_string(),
                Value::Number((state.closed_network as i64).into()),
            );
            return Ok(vec![row]);
        }

        if sql.contains("FROM federation_allowlist") {
            let host = params.first().and_then(Value::as_str).unwrap_or_default();
            return Ok(vec![count_row(
                state.allowlisted_hosts.contains(host) as u64
            )]);
        }

        if sql.contains("COUNT(*) as count") && sql.contains("FROM posts") {
            let actor_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let requires_public = sql.contains("visibility = 'public'");
            let excludes_encrypted = sql.contains("encrypted_message IS NULL");
            let excludes_fallback = sql.contains("End-to-end encrypted message");
            let count = state
                .posts
                .iter()
                .filter(|row| row.get_string("actor_id").as_deref() == Some(actor_id))
                .filter(|row| {
                    !requires_public || row.get_string("visibility").as_deref() == Some("public")
                })
                .filter(|row| {
                    !excludes_encrypted
                        || row
                            .get("encrypted_message")
                            .map(|value| value.is_null())
                            .unwrap_or(true)
                })
                .filter(|row| {
                    !excludes_fallback
                        || !row
                            .get_string("content")
                            .unwrap_or_default()
                            .contains("End-to-end encrypted message")
                })
                .count() as u64;
            return Ok(vec![count_row(count)]);
        }

        if sql.contains("SELECT id FROM posts WHERE id = ?1") {
            let post_id = params.first().and_then(Value::as_str).unwrap_or_default();
            return Ok(state
                .posts
                .iter()
                .find(|row| row.get_string("id").as_deref() == Some(post_id))
                .cloned()
                .map(|row| vec![row])
                .unwrap_or_default());
        }

        if sql.contains("FROM posts") && sql.contains("ORDER BY published_at DESC") {
            let actor_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let requires_public = sql.contains("visibility = 'public'");
            let excludes_encrypted = sql.contains("encrypted_message IS NULL");
            let excludes_fallback = sql.contains("End-to-end encrypted message");
            let rows = state
                .posts
                .iter()
                .filter(|row| row.get_string("actor_id").as_deref() == Some(actor_id))
                .filter(|row| {
                    !requires_public || row.get_string("visibility").as_deref() == Some("public")
                })
                .filter(|row| {
                    !excludes_encrypted
                        || row
                            .get("encrypted_message")
                            .map(|value| value.is_null())
                            .unwrap_or(true)
                })
                .filter(|row| {
                    !excludes_fallback
                        || !row
                            .get_string("content")
                            .unwrap_or_default()
                            .contains("End-to-end encrypted message")
                })
                .cloned()
                .collect::<Vec<_>>();
            return Ok(rows);
        }

        if sql.contains("SELECT COUNT(*) AS count FROM following") {
            let actor_id = params.first().and_then(Value::as_str).unwrap_or_default();
            return Ok(vec![count_row(
                (state.approved_following.contains(actor_id)) as u64,
            )]);
        }

        if sql.contains("UPDATE following SET status = 'accepted'") {
            let actor_id = params.first().and_then(Value::as_str).unwrap_or_default();
            if state.pending_following.remove(actor_id) {
                state.approved_following.insert(actor_id.to_string());
            }
            return Ok(Vec::new());
        }

        if sql.contains("DELETE FROM following") && sql.contains("status = 'pending'") {
            let actor_id = params.first().and_then(Value::as_str).unwrap_or_default();
            state.pending_following.remove(actor_id);
            return Ok(Vec::new());
        }

        if sql.contains("SELECT COUNT(*) as count FROM followers")
            && sql.contains("status = 'approved'")
        {
            let actor_id = params.first().and_then(Value::as_str).unwrap_or_default();
            return Ok(vec![count_row(
                (state.approved_followers.contains(actor_id)) as u64,
            )]);
        }

        if sql.contains("FROM friends") {
            return Ok(state.friends_rows.clone());
        }

        if sql.contains("FROM timeline_posts") && sql.contains("SELECT object_id") {
            if !state.home_rows.is_empty() {
                return Ok(state.home_rows.clone());
            }

            let mut rows: Vec<Row> = state.timeline_posts.values().cloned().collect();
            rows.sort_by(|left, right| {
                right
                    .get_string("published_at")
                    .cmp(&left.get_string("published_at"))
            });
            return Ok(rows);
        }

        if sql.contains("INSERT INTO timeline_posts") {
            let object_id = params.get(1).and_then(Value::as_str).unwrap_or_default();
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "object_id", params.get(1));
            insert_row_value(&mut row, "actor_id", params.get(2));
            insert_row_value(&mut row, "actor_username", params.get(3));
            insert_row_value(&mut row, "actor_display_name", params.get(4));
            insert_row_value(&mut row, "actor_avatar_url", params.get(5));
            insert_row_value(&mut row, "content", params.get(6));
            insert_row_value(&mut row, "content_html", params.get(7));
            insert_row_value(&mut row, "visibility", params.get(8));
            insert_row_value(&mut row, "in_reply_to", params.get(9));
            insert_row_value(&mut row, "published_at", params.get(10));
            insert_row_value(&mut row, "raw_object", params.get(11));
            insert_row_value(&mut row, "raw_activity", params.get(12));
            insert_row_value(&mut row, "encrypted_message", params.get(13));
            row.insert("deleted_at".to_string(), Value::Null);
            row.insert(
                "protocol".to_string(),
                Value::String("activitypub".to_string()),
            );
            state.timeline_posts.insert(object_id.to_string(), row);
            return Ok(Vec::new());
        }

        if sql.contains("UPDATE timeline_posts") && sql.contains("SET content") {
            let object_id = params.get(5).and_then(Value::as_str).unwrap_or_default();
            if let Some(row) = state.timeline_posts.get_mut(object_id) {
                set_row_value(row, "content", params.get(0));
                set_row_value(row, "content_html", params.get(1));
                set_row_value(row, "updated_at", params.get(2));
                set_row_value(row, "raw_object", params.get(3));
                set_row_value(row, "encrypted_message", params.get(4));
                row.insert("deleted_at".to_string(), Value::Null);
            }
            return Ok(Vec::new());
        }

        if sql.contains("UPDATE timeline_posts SET deleted_at") {
            let deleted_at = params.get(0).cloned().unwrap_or(Value::Null);
            let object_id = params.get(1).and_then(Value::as_str).unwrap_or_default();
            if let Some(row) = state.timeline_posts.get_mut(object_id) {
                row.insert("deleted_at".to_string(), deleted_at);
            }
            return Ok(Vec::new());
        }

        if sql.contains("INSERT OR IGNORE INTO conversations") {
            let conversation_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "participants", params.get(1));
            insert_row_value(&mut row, "last_message_at", params.get(2));
            state.conversations.insert(conversation_id.to_string(), row);
            return Ok(Vec::new());
        }

        if sql.contains("UPDATE conversations SET last_message_at") {
            let last_message_at = params.first().cloned().unwrap_or(Value::Null);
            let conversation_id = params.get(1).and_then(Value::as_str).unwrap_or_default();
            if let Some(row) = state.conversations.get_mut(conversation_id) {
                row.insert("last_message_at".to_string(), last_message_at);
            }
            return Ok(Vec::new());
        }

        if sql.contains("INSERT OR IGNORE INTO conversation_participants") {
            let conversation_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let actor_id = params.get(1).and_then(Value::as_str).unwrap_or_default();
            state
                .participants
                .insert((conversation_id.to_string(), actor_id.to_string()));
            return Ok(Vec::new());
        }

        if sql.contains("INSERT OR IGNORE INTO direct_messages") {
            let message_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "conversation_id", params.get(1));
            insert_row_value(&mut row, "sender_id", params.get(2));
            insert_row_value(&mut row, "content", params.get(3));
            insert_row_value(&mut row, "published_at", params.get(4));
            state.direct_messages.insert(message_id.to_string(), row);
            return Ok(Vec::new());
        }

        if sql.contains("INSERT INTO e2ee_conversations") {
            let conversation_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "participants", params.get(1));
            insert_row_value(&mut row, "created_at", params.get(2));
            insert_row_value(&mut row, "updated_at", params.get(2));
            state
                .e2ee_conversations
                .insert(conversation_id.to_string(), row);
            return Ok(Vec::new());
        }

        if sql.contains("INSERT OR IGNORE INTO e2ee_messages") {
            let message_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "conversation_id", params.get(1));
            insert_row_value(&mut row, "sender_actor_id", params.get(2));
            insert_row_value(&mut row, "sender_device_id", params.get(3));
            insert_row_value(&mut row, "ciphertext", params.get(4));
            insert_row_value(&mut row, "aad", params.get(5));
            insert_row_value(&mut row, "created_at", params.get(6));
            state.e2ee_messages.insert(message_id.to_string(), row);
            return Ok(Vec::new());
        }

        if sql.contains("INSERT OR IGNORE INTO replies") {
            let reply_id = params.first().and_then(Value::as_str).unwrap_or_default();
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "post_id", params.get(1));
            insert_row_value(&mut row, "actor_id", params.get(2));
            insert_row_value(&mut row, "actor_username", params.get(3));
            insert_row_value(&mut row, "actor_display_name", params.get(4));
            insert_row_value(&mut row, "actor_avatar_url", params.get(5));
            insert_row_value(&mut row, "content", params.get(6));
            insert_row_value(&mut row, "published_at", params.get(7));
            insert_row_value(&mut row, "visibility", params.get(8));
            insert_row_value(&mut row, "moderation_status", params.get(9));
            insert_row_value(&mut row, "moderation_score", params.get(10));
            insert_row_value(&mut row, "moderation_flags", params.get(11));
            insert_row_value(&mut row, "moderation_checked_at", params.get(12));
            insert_row_value(&mut row, "hidden", params.get(13));
            state.replies.insert(reply_id.to_string(), row);
            return Ok(Vec::new());
        }

        if sql.contains("INSERT INTO notifications") {
            let mut row = Row::new();
            insert_row_value(&mut row, "id", params.first());
            insert_row_value(&mut row, "type", params.get(1));
            insert_row_value(&mut row, "actor_id", params.get(2));
            insert_row_value(&mut row, "activity_id", params.get(7));
            insert_row_value(&mut row, "content", params.get(8));
            state.notifications.push(row);
            return Ok(Vec::new());
        }

        Ok(Vec::new())
    }

    async fn batch(&self, _statements: Vec<Statement>) -> PlatformResult<()> {
        Ok(())
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::SQLite
    }
}

struct FakeHttp {
    actor_json: String,
}

#[async_trait(?Send)]
impl HttpProvider for FakeHttp {
    async fn fetch(&self, request: Request) -> PlatformResult<Response> {
        Ok(Response {
            status: 200,
            headers: HashMap::new(),
            body: self.actor_json.clone().into_bytes(),
            url: request.url,
        })
    }
}

#[tokio::test]
async fn approved_follower_policy_accepts_known_actor() {
    let db = FakeDb::default();
    let actor_id = "https://mastodon.social/users/alice";

    assert!(!activitypub::is_approved_follower(&db, actor_id)
        .await
        .unwrap());

    db.approve_follower(actor_id);

    assert!(activitypub::is_approved_follower(&db, actor_id)
        .await
        .unwrap());
}

#[tokio::test]
async fn federation_allowlist_is_default_open() {
    let db = FakeDb::default();

    assert!(
        activitypub::is_federation_host_allowed(&db, "https://unlisted.example/users/alice")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn federation_allowlist_accepts_allowed_host_when_closed() {
    let db = FakeDb::default();
    db.set_closed_network(true);
    db.allow_host("trusted.example");

    assert!(
        activitypub::is_federation_host_allowed(&db, "https://trusted.example/users/alice")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn federation_allowlist_rejects_unlisted_host_when_closed() {
    let db = FakeDb::default();
    db.set_closed_network(true);
    db.allow_host("trusted.example");

    assert!(
        !activitypub::is_federation_host_allowed(&db, "https://unlisted.example/users/alice")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn public_outbox_excludes_unlisted_private_and_encrypted_posts() {
    let db = FakeDb::default();
    let actor_id = "https://social.dais.social/users/social";
    db.insert_actor("social", actor_id);
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/public",
        "public cleartext",
        "public",
        None,
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/unlisted",
        "unlisted cleartext",
        "unlisted",
        None,
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/followers",
        "followers cleartext",
        "followers",
        None,
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/encrypted",
        "encrypted fallback",
        "public",
        Some(r#"{"v":1}"#),
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/legacy-encrypted",
        "End-to-end encrypted message",
        "public",
        None,
    );

    let posts = activitypub::get_outbox_posts(&db, "social")
        .await
        .expect("outbox query should succeed");

    assert_eq!(posts.len(), 1);
    assert_eq!(
        posts[0].id,
        "https://social.dais.social/users/social/posts/public"
    );
    assert_eq!(posts[0].content, "public cleartext");
    assert!(posts[0].encrypted_message.is_none());
}

#[tokio::test]
async fn public_actor_count_excludes_private_and_encrypted_posts() {
    let db = FakeDb::default();
    let actor_id = "https://social.dais.social/users/social";
    db.insert_actor("social", actor_id);
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/public",
        "public cleartext",
        "public",
        None,
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/direct",
        "direct cleartext",
        "direct",
        None,
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/encrypted",
        "encrypted fallback",
        "public",
        Some(r#"{"v":1}"#),
    );
    db.insert_post(
        actor_id,
        "https://social.dais.social/users/social/posts/legacy-encrypted",
        "End-to-end encrypted message",
        "public",
        None,
    );

    let counts = activitypub::get_actor_counts(&db, actor_id)
        .await
        .expect("count query should succeed");

    assert_eq!(counts.post_count, 1);
}

#[tokio::test]
async fn anonymous_collection_pages_do_not_expose_social_graph_items() {
    let db = FakeDb::default();

    let followers = activitypub::get_followers(&db, "social", "social.dais.social", Some(1))
        .await
        .expect("followers page should render");
    let following = activitypub::get_following(&db, "social", "social.dais.social", Some(1))
        .await
        .expect("following page should render");

    assert_eq!(followers["type"], "OrderedCollectionPage");
    assert_eq!(followers["orderedItems"], json!([]));
    assert_eq!(following["type"], "OrderedCollectionPage");
    assert_eq!(following["orderedItems"], json!([]));
}

#[tokio::test]
async fn inbox_create_ingests_timeline_post_for_accepted_following() {
    let db = FakeDb::default();
    db.approve_following("https://remote.example/users/alice");
    let http = FakeHttp {
        actor_json: json!({
            "preferredUsername": "alice",
            "name": "Alice",
            "icon": { "url": "https://remote.example/avatar.png" }
        })
        .to_string(),
    };

    let activity = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Create".to_string(),
        id: "https://remote.example/activities/1".to_string(),
        actor: "https://remote.example/users/alice".to_string(),
        object: Some(json!({
            "type": "Note",
            "id": "https://remote.example/users/alice/statuses/1",
            "content": "private mode note",
            "published": "2026-06-10T12:00:00Z",
            "to": ["https://www.w3.org/ns/activitystreams#Public"]
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T12:00:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::process_inbox_activity(
        &db,
        &http,
        activity,
        "https://social.dais.social/users/social",
        "",
        None,
    )
    .await
    .expect("inbox create should ingest");

    let row = db
        .timeline_post("https://remote.example/users/alice/statuses/1")
        .expect("timeline row should be stored");

    assert_eq!(
        row.get_string("actor_id").as_deref(),
        Some("https://remote.example/users/alice")
    );
    assert_eq!(row.get_string("actor_username").as_deref(), Some("alice"));
    assert_eq!(
        row.get_string("content").as_deref(),
        Some("private mode note")
    );
    assert_eq!(row.get_string("visibility").as_deref(), Some("public"));
    assert_eq!(row.get_string("protocol").as_deref(), Some("activitypub"));
}

#[tokio::test]
async fn inbox_create_ingests_question_reply_for_accepted_following() {
    let db = FakeDb::default();
    db.approve_following("https://remote.example/users/alice");
    let http = FakeHttp {
        actor_json: json!({
            "preferredUsername": "alice",
            "name": "Alice",
            "icon": { "url": "https://remote.example/avatar.png" }
        })
        .to_string(),
    };

    let activity = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Create".to_string(),
        id: "https://remote.example/activities/poll-1".to_string(),
        actor: "https://remote.example/users/alice".to_string(),
        object: Some(json!({
            "type": "Question",
            "id": "https://remote.example/users/alice/statuses/poll-1",
            "content": "which client should ship next?",
            "published": "2026-06-10T13:00:00Z",
            "inReplyTo": "https://remote.example/users/bob/statuses/thread-root",
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "oneOf": [
                { "type": "Note", "name": "rust", "replies": { "type": "Collection", "totalItems": 2 } },
                { "type": "Note", "name": "tauri", "replies": { "type": "Collection", "totalItems": 1 } }
            ]
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T13:00:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::process_inbox_activity(
        &db,
        &http,
        activity,
        "https://social.dais.social/users/social",
        "",
        None,
    )
    .await
    .expect("question reply from followed actor should ingest");

    let row = db
        .timeline_post("https://remote.example/users/alice/statuses/poll-1")
        .expect("question timeline row should be stored");
    assert_eq!(
        row.get_string("in_reply_to").as_deref(),
        Some("https://remote.example/users/bob/statuses/thread-root")
    );
    let raw_object: Value = serde_json::from_str(
        row.get_string("raw_object")
            .expect("raw question object should be stored")
            .as_str(),
    )
    .unwrap();
    assert_eq!(raw_object["type"], "Question");
    assert_eq!(raw_object["oneOf"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn inbox_update_and_delete_modify_timeline_post() {
    let db = FakeDb::default();
    db.approve_following("https://remote.example/users/alice");
    db.set_home_rows(Vec::new());
    db.set_friends_rows(Vec::new());

    {
        let mut row = Row::new();
        row.insert("id".to_string(), Value::String("row-1".to_string()));
        row.insert(
            "object_id".to_string(),
            Value::String("https://remote.example/users/alice/statuses/1".to_string()),
        );
        row.insert(
            "actor_id".to_string(),
            Value::String("https://remote.example/users/alice".to_string()),
        );
        row.insert("content".to_string(), Value::String("before".to_string()));
        row.insert(
            "content_html".to_string(),
            Value::String("before".to_string()),
        );
        row.insert(
            "visibility".to_string(),
            Value::String("followers".to_string()),
        );
        row.insert(
            "published_at".to_string(),
            Value::String("2026-06-10T12:00:00Z".to_string()),
        );
        db.state.lock().unwrap().timeline_posts.insert(
            "https://remote.example/users/alice/statuses/1".to_string(),
            row,
        );
    }

    let update = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Update".to_string(),
        id: "https://remote.example/activities/2".to_string(),
        actor: "https://remote.example/users/alice".to_string(),
        object: Some(json!({
            "type": "Note",
            "id": "https://remote.example/users/alice/statuses/1",
            "content": "after",
            "updated": "2026-06-10T12:05:00Z",
            "encryptedMessage": { "v": 1, "alg": "AES-256-GCM", "keyWrap": "RSA-OAEP-256", "iv": "a", "ciphertext": "b", "tag": "c", "recipients": [] }
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T12:05:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::inbox::handle_update(&db, &update)
        .await
        .expect("update should apply");

    let updated = db
        .timeline_post("https://remote.example/users/alice/statuses/1")
        .expect("timeline row should remain");
    assert_eq!(updated.get_string("content").as_deref(), Some("after"));
    assert_eq!(
        updated.get_string("updated_at").as_deref(),
        Some("2026-06-10T12:05:00Z")
    );
    assert_eq!(updated.get_string("deleted_at"), None);

    let delete = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Delete".to_string(),
        id: "https://remote.example/activities/3".to_string(),
        actor: "https://remote.example/users/alice".to_string(),
        object: Some(Value::String(
            "https://remote.example/users/alice/statuses/1".to_string(),
        )),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T12:06:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::inbox::handle_delete(&db, &delete)
        .await
        .expect("delete should apply");

    let deleted = db
        .timeline_post("https://remote.example/users/alice/statuses/1")
        .expect("timeline row should still exist");
    assert_eq!(
        deleted.get_string("deleted_at").as_deref(),
        Some("2026-06-10T12:06:00Z")
    );
}

#[tokio::test]
async fn inbox_update_refreshes_question_timeline_post() {
    let db = FakeDb::default();
    db.approve_following("https://remote.example/users/alice");

    {
        let mut row = Row::new();
        row.insert("id".to_string(), Value::String("row-question".to_string()));
        row.insert(
            "object_id".to_string(),
            Value::String("https://remote.example/users/alice/statuses/poll-1".to_string()),
        );
        row.insert(
            "actor_id".to_string(),
            Value::String("https://remote.example/users/alice".to_string()),
        );
        row.insert(
            "content".to_string(),
            Value::String("which client should ship next?".to_string()),
        );
        row.insert(
            "content_html".to_string(),
            Value::String("which client should ship next?".to_string()),
        );
        row.insert(
            "visibility".to_string(),
            Value::String("public".to_string()),
        );
        row.insert(
            "published_at".to_string(),
            Value::String("2026-06-10T13:00:00Z".to_string()),
        );
        db.state.lock().unwrap().timeline_posts.insert(
            "https://remote.example/users/alice/statuses/poll-1".to_string(),
            row,
        );
    }

    let update = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Update".to_string(),
        id: "https://remote.example/activities/poll-update-1".to_string(),
        actor: "https://remote.example/users/alice".to_string(),
        object: Some(json!({
            "type": "Question",
            "id": "https://remote.example/users/alice/statuses/poll-1",
            "content": "which client shipped first?",
            "updated": "2026-06-10T13:05:00Z",
            "oneOf": [
                { "type": "Note", "name": "rust", "replies": { "type": "Collection", "totalItems": 3 } },
                { "type": "Note", "name": "tauri", "replies": { "type": "Collection", "totalItems": 1 } }
            ]
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T13:05:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::inbox::handle_update(&db, &update)
        .await
        .expect("question update should apply");

    let updated = db
        .timeline_post("https://remote.example/users/alice/statuses/poll-1")
        .expect("question timeline row should remain");
    assert_eq!(
        updated.get_string("content").as_deref(),
        Some("which client shipped first?")
    );
    let raw_object: Value = serde_json::from_str(
        updated
            .get_string("raw_object")
            .expect("raw question object should be refreshed")
            .as_str(),
    )
    .unwrap();
    assert_eq!(raw_object["type"], "Question");
    assert_eq!(raw_object["oneOf"][0]["replies"]["totalItems"], 3);
}

#[tokio::test]
async fn inbox_accept_marks_pending_following_accepted() {
    let db = FakeDb::default();
    let actor = "https://remote.example/users/alice";
    db.request_following(actor);

    let accept = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Accept".to_string(),
        id: "https://remote.example/activities/accept-follow".to_string(),
        actor: actor.to_string(),
        object: Some(json!({
            "type": "Follow",
            "actor": "https://social.dais.social/users/social",
            "object": actor
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T14:00:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::inbox::handle_accept(&db, &accept)
        .await
        .expect("accept should mark following accepted");

    assert!(!db.pending_following(actor));
    assert!(db.accepted_following(actor));
}

#[tokio::test]
async fn inbox_reject_removes_pending_following() {
    let db = FakeDb::default();
    let actor = "https://remote.example/users/alice";
    db.request_following(actor);

    let reject = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Reject".to_string(),
        id: "https://remote.example/activities/reject-follow".to_string(),
        actor: actor.to_string(),
        object: Some(json!({
            "type": "Follow",
            "actor": "https://social.dais.social/users/social",
            "object": actor
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T14:00:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::inbox::handle_reject(&db, &reject)
        .await
        .expect("reject should remove pending following");

    assert!(!db.pending_following(actor));
    assert!(!db.accepted_following(actor));
}

#[tokio::test]
async fn home_timeline_and_friends_views_map_rows() {
    let db = FakeDb::default();

    let mut home_row = Row::new();
    home_row.insert("object_id".to_string(), Value::String("post-1".to_string()));
    home_row.insert(
        "actor_id".to_string(),
        Value::String("https://remote.example/users/alice".to_string()),
    );
    home_row.insert(
        "actor_username".to_string(),
        Value::String("alice".to_string()),
    );
    home_row.insert(
        "actor_display_name".to_string(),
        Value::String("Alice".to_string()),
    );
    home_row.insert("content".to_string(), Value::String("hello".to_string()));
    home_row.insert(
        "visibility".to_string(),
        Value::String("followers".to_string()),
    );
    home_row.insert(
        "published_at".to_string(),
        Value::String("2026-06-10T12:00:00Z".to_string()),
    );
    home_row.insert(
        "protocol".to_string(),
        Value::String("activitypub".to_string()),
    );
    db.set_home_rows(vec![home_row]);

    let mut friend_row = Row::new();
    friend_row.insert(
        "local_actor_id".to_string(),
        Value::String("https://social.dais.social/users/social".to_string()),
    );
    friend_row.insert(
        "friend_actor_id".to_string(),
        Value::String("https://remote.example/users/alice".to_string()),
    );
    friend_row.insert(
        "friend_inbox".to_string(),
        Value::String("https://remote.example/inbox".to_string()),
    );
    friend_row.insert("friend_shared_inbox".to_string(), Value::Null);
    friend_row.insert(
        "follower_since".to_string(),
        Value::String("2026-06-01T00:00:00Z".to_string()),
    );
    friend_row.insert(
        "following_since".to_string(),
        Value::String("2026-06-02T00:00:00Z".to_string()),
    );
    friend_row.insert(
        "accepted_at".to_string(),
        Value::String("2026-06-03T00:00:00Z".to_string()),
    );
    db.set_friends_rows(vec![friend_row]);

    let timeline = activitypub::get_home_timeline(&db, 20, None)
        .await
        .expect("home timeline should load");
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].content, "hello");
    assert_eq!(timeline[0].visibility, "followers");

    let friends = activitypub::get_friends(&db, "https://social.dais.social/users/social", 20)
        .await
        .expect("friends should load");
    assert_eq!(friends.len(), 1);
    assert_eq!(
        friends[0].friend_actor_id,
        "https://remote.example/users/alice"
    );
    assert_eq!(
        friends[0].accepted_at.as_deref(),
        Some("2026-06-03T00:00:00Z")
    );
}

#[tokio::test]
async fn inbox_direct_message_uses_conversation_schema() {
    let db = FakeDb::default();
    let http = FakeHttp {
        actor_json: json!({
            "preferredUsername": "alice",
            "name": "Alice",
            "icon": { "url": "https://remote.example/avatar.png" }
        })
        .to_string(),
    };

    let local_actor = "https://social.dais.social/users/social";
    let remote_actor = "https://remote.example/users/alice";
    let activity = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Create".to_string(),
        id: "https://remote.example/activities/dm-1".to_string(),
        actor: remote_actor.to_string(),
        object: Some(json!({
            "type": "Note",
            "id": "https://remote.example/users/alice/statuses/dm-1",
            "content": "private hello",
            "published": "2026-06-10T12:00:00Z",
            "to": [local_actor]
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T12:00:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::process_inbox_activity(&db, &http, activity, local_actor, "", None)
        .await
        .expect("direct message should ingest");

    let message = db
        .direct_message("https://remote.example/users/alice/statuses/dm-1")
        .expect("direct message row should be stored");
    let conversation_id = message
        .get_string("conversation_id")
        .expect("message should reference conversation");
    let conversation = db
        .conversation(&conversation_id)
        .expect("conversation row should be stored");

    assert_eq!(
        message.get_string("sender_id").as_deref(),
        Some(remote_actor)
    );
    assert_eq!(
        message.get_string("content").as_deref(),
        Some("private hello")
    );
    assert_eq!(db.participant_count(), 2);
    assert!(conversation
        .get_string("participants")
        .expect("participants json should be stored")
        .contains(local_actor));
}

#[tokio::test]
async fn inbox_encrypted_direct_message_persists_e2ee_envelope() {
    let db = FakeDb::default();
    let http = FakeHttp {
        actor_json: json!({
            "preferredUsername": "alice",
            "name": "Alice",
            "icon": { "url": "https://remote.example/avatar.png" }
        })
        .to_string(),
    };

    let local_actor = "https://social.dais.social/users/social";
    let remote_actor = "https://remote.example/users/alice";
    let message_id = "https://remote.example/users/alice/e2ee/messages/1";
    let encrypted_message = json!({
        "v": 1,
        "alg": "AES-256-GCM",
        "keyWrap": "RSA-OAEP-256",
        "iv": "MTIzNDU2Nzg5MDEy",
        "ciphertext": "Y2lwaGVydGV4dA==",
        "recipients": [
            {
                "keyId": "https://social.dais.social/users/social#main-key",
                "wrappedKey": "d3JhcHBlZA=="
            }
        ]
    });
    let activity = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Create".to_string(),
        id: "https://remote.example/activities/e2ee-1".to_string(),
        actor: remote_actor.to_string(),
        object: Some(json!({
            "type": "Note",
            "id": message_id,
            "attributedTo": remote_actor,
            "to": [local_actor],
            "published": "2026-06-10T12:00:00Z",
            "content": "Encrypted message. Open in a dais client to decrypt.",
            "daisE2ee": {
                "v": 1,
                "protocol": "dais-mls-v1",
                "senderDeviceId": "alice-phone"
            },
            "encryptedMessage": encrypted_message
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T12:00:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::process_inbox_activity(&db, &http, activity, local_actor, "", None)
        .await
        .expect("encrypted direct message should ingest");

    let fallback = db
        .direct_message(message_id)
        .expect("fallback direct message row should be stored");
    assert_eq!(
        fallback.get_string("content").as_deref(),
        Some("Encrypted message. Open in a dais client to decrypt.")
    );

    let encrypted = db
        .e2ee_message(message_id)
        .expect("E2EE message row should be stored");
    assert_eq!(
        encrypted.get_string("sender_actor_id").as_deref(),
        Some(remote_actor)
    );
    assert_eq!(
        encrypted.get_string("sender_device_id").as_deref(),
        Some("alice-phone")
    );
    assert!(
        encrypted
            .get_string("ciphertext")
            .expect("ciphertext JSON should be stored")
            .contains("\"encryptedMessage\"")
            || encrypted
                .get_string("ciphertext")
                .expect("ciphertext JSON should be stored")
                .contains("\"AES-256-GCM\"")
    );
    assert!(encrypted
        .get_string("aad")
        .expect("aad JSON should be stored")
        .contains(local_actor));
}

#[tokio::test]
async fn inbox_reply_preserves_followers_only_visibility() {
    let db = FakeDb::default();
    let http = FakeHttp {
        actor_json: json!({
            "preferredUsername": "alice",
            "name": "Alice",
            "icon": { "url": "https://remote.example/avatar.png" }
        })
        .to_string(),
    };

    let local_post = "https://social.dais.social/users/social/posts/private-root";
    db.insert_post(
        "https://social.dais.social/users/social",
        local_post,
        "followers root",
        "followers",
        None,
    );

    let activity = activitypub::Activity {
        context: activitypub::Context::default(),
        activity_type: "Create".to_string(),
        id: "https://remote.example/activities/reply-1".to_string(),
        actor: "https://remote.example/users/alice".to_string(),
        object: Some(json!({
            "type": "Note",
            "id": "https://remote.example/users/alice/statuses/reply-1",
            "content": "followers-only reply",
            "published": "2026-06-10T12:10:00Z",
            "inReplyTo": local_post,
            "to": ["https://social.dais.social/users/social/followers"]
        })),
        target: None,
        to: None,
        cc: None,
        published: Some("2026-06-10T12:10:00Z".to_string()),
        extra: HashMap::new(),
    };

    activitypub::process_inbox_activity(
        &db,
        &http,
        activity,
        "https://social.dais.social/users/social",
        "",
        None,
    )
    .await
    .expect("followers-only reply should ingest");

    let reply = db
        .reply("https://remote.example/users/alice/statuses/reply-1")
        .expect("reply row should be stored");
    assert_eq!(reply.get_string("post_id").as_deref(), Some(local_post));
    assert_eq!(
        reply.get_string("content").as_deref(),
        Some("followers-only reply")
    );
    assert_eq!(reply.get_string("visibility").as_deref(), Some("followers"));
}

fn count_row(count: u64) -> Row {
    let mut row = Row::new();
    row.insert("count".to_string(), Value::from(count));
    row
}

fn insert_row_value(row: &mut Row, key: &str, value: Option<&Value>) {
    row.insert(
        key.to_string(),
        value.cloned().unwrap_or_else(|| Value::Null),
    );
}

fn set_row_value(row: &mut Row, key: &str, value: Option<&Value>) {
    row.insert(
        key.to_string(),
        value.cloned().unwrap_or_else(|| Value::Null),
    );
}
