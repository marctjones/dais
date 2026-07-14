//! Personal Bluesky AppView (issue #50, Track B): a Durable Object that
//! holds a persistent outbound WebSocket connection to Bluesky's public
//! relay firehose (`com.atproto.sync.subscribeRepos`), decodes commits for
//! DIDs the owner follows, and indexes posts/likes/follows into D1.
//!
//! `subscribeRepos` has no server-side DID filter -- it streams every
//! commit on the network, and filtering to the owner's follows happens here,
//! after decode. That is why this holds one always-on connection rather
//! than duty-cycling: catching up after a gap means decoding the entire
//! network's commit volume for that window, not just the followed DIDs'
//! commits, so a bounded connection window would fall further behind on
//! every reconnect rather than catching up.
//!
//! The read loop (`run_read_loop`) is spawned once via `State::wait_until`,
//! which keeps this Durable Object resident for as long as the loop's
//! future is pending, and owns its own clone of the `WebSocket` -- the
//! struct's own `socket` field exists only so `alarm()`'s watchdog can close
//! the shared connection for budget enforcement. `alarm()` itself is a
//! periodic watchdog, not the per-message driver: it re-starts the read
//! loop if it isn't running (including after a cold restart, when the
//! struct's in-memory `socket` field has reset to `None`), and enforces the
//! daily active-time budget as a runaway-loop backstop.
//!
//! [`ensure_firehose_subscription_running`] is what actually starts this --
//! called from the router's existing 30-minute cron, gated behind the
//! `FIREHOSE_ENABLED` var (unset/absent means disabled). Deploying this
//! code does not by itself open a connection to Bluesky's relay in any
//! environment; that var is only meant to be set once the live-relay smoke
//! test in issue #50 has passed for that environment.

use dais_core::atproto::firehose::{decode_frame, record_bytes_to_json, FirehoseEvent};
use dais_core::atproto::mst::{extract_commit_changes, RepoChange};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;
use worker::*;

const RELAY_HOST: &str = "bsky.network";
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(60);
/// How often the read loop persists its cursor and re-checks the follow
/// list, in processed firehose messages (matched or not). A crash between
/// flushes just means re-decoding up to this many already-seen messages on
/// reconnect -- harmless, since every D1 write here is an idempotent
/// upsert/delete keyed by URI.
const FLUSH_EVERY_MESSAGES: u32 = 200;

/// Kicks the firehose Durable Object if `FIREHOSE_ENABLED` is `"true"` for
/// this environment; a no-op otherwise, so shipping this code does not by
/// itself start consuming Bluesky's live relay anywhere. Safe to call
/// repeatedly -- `fetch()` on the DO is idempotent (see `ensure_running`).
pub(crate) async fn ensure_firehose_subscription_running(env: &Env) -> Result<()> {
    let enabled = env
        .var("FIREHOSE_ENABLED")
        .map(|value| value.to_string())
        .unwrap_or_default()
        == "true";
    if !enabled {
        return Ok(());
    }

    let namespace = env.durable_object("FIREHOSE_SUBSCRIPTION")?;
    let id = namespace.id_from_name("default")?;
    let stub = id.get_stub()?;
    stub.fetch_with_str("https://firehose-subscription/start")
        .await?;
    Ok(())
}

#[durable_object]
pub struct FirehoseSubscription {
    state: State,
    env: Env,
    socket: Rc<RefCell<Option<WebSocket>>>,
}

impl DurableObject for FirehoseSubscription {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            socket: Rc::new(RefCell::new(None)),
        }
    }

    async fn fetch(&self, _req: Request) -> Result<Response> {
        self.ensure_running().await?;
        self.state.storage().set_alarm(WATCHDOG_INTERVAL).await?;
        Response::ok("ok")
    }

    async fn alarm(&self) -> Result<Response> {
        if let Err(error) = self.watchdog_tick().await {
            console_log!("firehose watchdog error: {error}");
        }
        self.state.storage().set_alarm(WATCHDOG_INTERVAL).await?;
        Response::ok("tick")
    }
}

impl FirehoseSubscription {
    async fn watchdog_tick(&self) -> Result<()> {
        let today = today_utc_date();
        let checkpoint = load_checkpoint(&self.env).await.map_err(Error::RustError)?;

        if checkpoint.budget_date != today {
            reset_daily_budget(&self.env, &today)
                .await
                .map_err(Error::RustError)?;
        } else if checkpoint.status == "budget_exceeded" {
            self.close_socket();
            return Ok(());
        }

        self.ensure_running().await
    }

    fn close_socket(&self) {
        if let Some(ws) = self.socket.borrow_mut().take() {
            let _ = ws.close(None::<u16>, Some("stopping"));
        }
    }

    async fn ensure_running(&self) -> Result<()> {
        if self.socket.borrow().is_some() {
            return Ok(());
        }

        let today = today_utc_date();
        let checkpoint = load_checkpoint(&self.env).await.map_err(Error::RustError)?;
        if checkpoint.status == "budget_exceeded" && checkpoint.budget_date == today {
            return Ok(());
        }

        let dids = load_followed_dids(&self.env)
            .await
            .map_err(Error::RustError)?;
        if dids.is_empty() {
            return Ok(());
        }

        let url = Url::parse(&relay_url(checkpoint.last_seq))
            .map_err(|error| Error::RustError(format!("invalid relay url: {error}")))?;
        let ws = WebSocket::connect(url).await?;
        *self.socket.borrow_mut() = Some(ws.clone());
        mark_running(&self.env).await.map_err(Error::RustError)?;

        let task_env = self.env.clone();
        let task_socket = self.socket.clone();
        self.state.wait_until(async move {
            if let Err(error) = run_read_loop(task_env, ws, dids).await {
                console_log!("firehose read loop ended: {error}");
            }
            // The loop only returns when the connection is gone (closed,
            // errored, or budget-closed by the watchdog) -- clear the shared
            // handle so the next `ensure_running` call (from `fetch()` or
            // the next `alarm()` tick) knows to reconnect instead of seeing
            // a stale `Some` and assuming it's still live.
            task_socket.borrow_mut().take();
        });

        Ok(())
    }
}

fn relay_url(last_seq: i64) -> String {
    if last_seq > 0 {
        format!("wss://{RELAY_HOST}/xrpc/com.atproto.sync.subscribeRepos?cursor={last_seq}")
    } else {
        format!("wss://{RELAY_HOST}/xrpc/com.atproto.sync.subscribeRepos")
    }
}

async fn run_read_loop(env: Env, ws: WebSocket, mut dids: HashSet<String>) -> Result<()> {
    use futures_util::StreamExt;

    console_log!("firehose read loop starting, {} followed dids", dids.len());
    let mut events = ws.events()?;
    let mut processed_since_flush: u32 = 0;
    let mut total_processed: u64 = 0;
    let mut last_seq: i64 = 0;
    let mut last_flush_at = js_sys::Date::now();

    while let Some(event) = events.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                console_log!("firehose event stream error: {error}");
                break;
            }
        };
        if total_processed == 0 {
            console_log!("firehose read loop received its first event");
        }
        let WebsocketEvent::Message(message) = event else {
            console_log!("firehose read loop got a Close event, exiting");
            break; // Close event: the stream's guaranteed final item.
        };
        let Some(bytes) = message.bytes() else {
            console_log!("firehose read loop got a non-binary message, skipping");
            continue; // Non-binary frame; the wire protocol only sends binary.
        };
        total_processed += 1;

        match decode_frame(&bytes) {
            Ok(FirehoseEvent::Commit(commit)) => {
                last_seq = commit.seq as i64;
                if dids.contains(&commit.repo_did) {
                    if let Ok(changes) =
                        extract_commit_changes(&commit.car, commit.commit_cid, &commit.ops)
                    {
                        let writes = changes_to_index_writes(&commit.repo_did, changes);
                        for write in &writes {
                            if let Err(error) = apply_index_write(&env, write).await {
                                console_log!("firehose index write failed: {error}");
                            }
                        }
                    }
                }
            }
            Ok(FirehoseEvent::Other(_)) => {}
            Err(error) => {
                console_log!("firehose frame decode failed: {error}");
            }
        }

        processed_since_flush += 1;
        if processed_since_flush >= FLUSH_EVERY_MESSAGES {
            processed_since_flush = 0;
            let now = js_sys::Date::now();
            let delta_seconds = ((now - last_flush_at) / 1000.0) as i64;
            last_flush_at = now;
            if let Err(error) = flush_checkpoint(&env, last_seq, delta_seconds.max(0)).await {
                console_log!("firehose checkpoint flush failed: {error}");
            }
            match load_followed_dids(&env).await {
                Ok(refreshed) => dids = refreshed,
                Err(error) => console_log!("firehose follow list refresh failed: {error}"),
            }
        }
    }

    Ok(())
}

/// The parts of an indexed change a D1 writer needs, independent of how the
/// record bytes were decoded or which Workers APIs write them -- kept
/// separate from `apply_index_write` so the create/update/delete -> table
/// mapping is unit-testable without a live D1 binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IndexWrite {
    UpsertPost {
        object_id: String,
        actor_id: String,
        content: String,
        in_reply_to: Option<String>,
        published_at: String,
    },
    DeletePost {
        object_id: String,
    },
    UpsertLike {
        uri: String,
        actor_did: String,
        subject_uri: String,
        subject_cid: Option<String>,
        created_at: String,
    },
    DeleteLike {
        uri: String,
    },
    UpsertFollow {
        uri: String,
        actor_did: String,
        subject_did: String,
        created_at: String,
    },
    DeleteFollow {
        uri: String,
    },
}

pub(crate) fn changes_to_index_writes(repo_did: &str, changes: Vec<RepoChange>) -> Vec<IndexWrite> {
    changes
        .into_iter()
        .filter_map(|change| change_to_index_write(repo_did, change))
        .collect()
}

fn change_to_index_write(repo_did: &str, change: RepoChange) -> Option<IndexWrite> {
    let path = change.path().to_string();
    let uri = format!("at://{repo_did}/{path}");
    let collection = path.split('/').next().unwrap_or("").to_string();

    match change {
        RepoChange::Created { record_bytes, .. } | RepoChange::Updated { record_bytes, .. } => {
            let json = record_bytes_to_json(&record_bytes).ok()?;
            match collection.as_str() {
                "app.bsky.feed.post" => Some(IndexWrite::UpsertPost {
                    object_id: uri,
                    actor_id: repo_did.to_string(),
                    content: json_str(&json, "text"),
                    in_reply_to: json
                        .get("reply")
                        .and_then(|reply| reply.get("parent"))
                        .and_then(|parent| parent.get("uri"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    published_at: json_str(&json, "createdAt"),
                }),
                "app.bsky.feed.like" => Some(IndexWrite::UpsertLike {
                    uri,
                    actor_did: repo_did.to_string(),
                    subject_uri: json
                        .get("subject")
                        .and_then(|subject| subject.get("uri"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    subject_cid: json
                        .get("subject")
                        .and_then(|subject| subject.get("cid"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    created_at: json_str(&json, "createdAt"),
                }),
                "app.bsky.graph.follow" => Some(IndexWrite::UpsertFollow {
                    uri,
                    actor_did: repo_did.to_string(),
                    subject_did: json
                        .get("subject")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    created_at: json_str(&json, "createdAt"),
                }),
                _ => None,
            }
        }
        RepoChange::Deleted { .. } => match collection.as_str() {
            "app.bsky.feed.post" => Some(IndexWrite::DeletePost { object_id: uri }),
            "app.bsky.feed.like" => Some(IndexWrite::DeleteLike { uri }),
            "app.bsky.graph.follow" => Some(IndexWrite::DeleteFollow { uri }),
            _ => None,
        },
    }
}

fn json_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

async fn apply_index_write(env: &Env, write: &IndexWrite) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;

    match write {
        IndexWrite::UpsertPost {
            object_id,
            actor_id,
            content,
            in_reply_to,
            published_at,
        } => {
            let object_id_arg = D1Type::Text(object_id);
            let actor_id_arg = D1Type::Text(actor_id);
            let content_arg = D1Type::Text(content);
            let in_reply_to_arg = in_reply_to
                .as_deref()
                .map(D1Type::Text)
                .unwrap_or(D1Type::Null);
            let published_at_arg = D1Type::Text(published_at);
            let id_arg = D1Type::Text(object_id);
            db.prepare(
                r#"
                INSERT INTO timeline_posts (
                  id, object_id, actor_id, content, visibility, in_reply_to,
                  published_at, protocol, created_at
                ) VALUES (?1, ?2, ?3, ?4, 'public', ?5, ?6, 'atproto', CURRENT_TIMESTAMP)
                ON CONFLICT(object_id) DO UPDATE SET
                  content = excluded.content,
                  in_reply_to = excluded.in_reply_to,
                  updated_at = CURRENT_TIMESTAMP,
                  deleted_at = NULL
                "#,
            )
            .bind_refs([
                &id_arg,
                &object_id_arg,
                &actor_id_arg,
                &content_arg,
                &in_reply_to_arg,
                &published_at_arg,
            ])
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
        }
        IndexWrite::DeletePost { object_id } => {
            let object_id_arg = D1Type::Text(object_id);
            db.prepare(
                "UPDATE timeline_posts SET deleted_at = CURRENT_TIMESTAMP WHERE object_id = ?1",
            )
            .bind_refs([&object_id_arg])
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
        }
        IndexWrite::UpsertLike {
            uri,
            actor_did,
            subject_uri,
            subject_cid,
            created_at,
        } => {
            let uri_arg = D1Type::Text(uri);
            let actor_arg = D1Type::Text(actor_did);
            let subject_uri_arg = D1Type::Text(subject_uri);
            let subject_cid_arg = subject_cid
                .as_deref()
                .map(D1Type::Text)
                .unwrap_or(D1Type::Null);
            let created_at_arg = D1Type::Text(created_at);
            db.prepare(
                r#"
                INSERT OR IGNORE INTO atproto_likes (
                  uri, actor_did, subject_uri, subject_cid, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
            )
            .bind_refs([
                &uri_arg,
                &actor_arg,
                &subject_uri_arg,
                &subject_cid_arg,
                &created_at_arg,
            ])
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
        }
        IndexWrite::DeleteLike { uri } => {
            let uri_arg = D1Type::Text(uri);
            db.prepare("DELETE FROM atproto_likes WHERE uri = ?1")
                .bind_refs([&uri_arg])
                .map_err(|error| error.to_string())?
                .run()
                .await
                .map_err(|error| error.to_string())?;
        }
        IndexWrite::UpsertFollow {
            uri,
            actor_did,
            subject_did,
            created_at,
        } => {
            let uri_arg = D1Type::Text(uri);
            let actor_arg = D1Type::Text(actor_did);
            let subject_arg = D1Type::Text(subject_did);
            let created_at_arg = D1Type::Text(created_at);
            db.prepare(
                r#"
                INSERT OR IGNORE INTO atproto_follows (
                  uri, actor_did, subject_did, created_at
                ) VALUES (?1, ?2, ?3, ?4)
                "#,
            )
            .bind_refs([&uri_arg, &actor_arg, &subject_arg, &created_at_arg])
            .map_err(|error| error.to_string())?
            .run()
            .await
            .map_err(|error| error.to_string())?;
        }
        IndexWrite::DeleteFollow { uri } => {
            let uri_arg = D1Type::Text(uri);
            db.prepare("DELETE FROM atproto_follows WHERE uri = ?1")
                .bind_refs([&uri_arg])
                .map_err(|error| error.to_string())?
                .run()
                .await
                .map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

struct Checkpoint {
    last_seq: i64,
    status: String,
    budget_date: String,
    active_seconds_today: i64,
    daily_budget_seconds: i64,
}

async fn load_checkpoint(env: &Env) -> std::result::Result<Checkpoint, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let row = db
        .prepare(
            r#"
            INSERT INTO atproto_firehose_checkpoint (id) VALUES ('default')
            ON CONFLICT(id) DO NOTHING
            RETURNING last_seq, status, budget_date, active_seconds_today, daily_budget_seconds
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?;

    let row = match row {
        Some(row) => row,
        None => db
            .prepare(
                "SELECT last_seq, status, budget_date, active_seconds_today, daily_budget_seconds \
                 FROM atproto_firehose_checkpoint WHERE id = 'default'",
            )
            .first::<Map<String, Value>>(None)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "checkpoint row missing after upsert".to_string())?,
    };

    Ok(Checkpoint {
        last_seq: field_i64(&row, "last_seq"),
        status: field_str(&row, "status").unwrap_or_else(|| "stopped".to_string()),
        budget_date: field_str(&row, "budget_date").unwrap_or_default(),
        active_seconds_today: field_i64(&row, "active_seconds_today"),
        daily_budget_seconds: field_i64(&row, "daily_budget_seconds"),
    })
}

async fn mark_running(env: &Env) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    db.prepare(
        "UPDATE atproto_firehose_checkpoint SET status = 'running', updated_at = CURRENT_TIMESTAMP \
         WHERE id = 'default'",
    )
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn reset_daily_budget(env: &Env, today: &str) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let today_arg = D1Type::Text(today);
    db.prepare(
        r#"
        UPDATE atproto_firehose_checkpoint
        SET budget_date = ?1, active_seconds_today = 0, status = 'stopped',
            updated_at = CURRENT_TIMESTAMP
        WHERE id = 'default'
        "#,
    )
    .bind_refs([&today_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Persists progress since the last flush. `delta_seconds` is the elapsed
/// time since the previous flush (not since the loop started), so repeated
/// flushes across a connection's lifetime -- and across reconnects within
/// the same day -- accumulate onto `active_seconds_today` rather than
/// resetting it each time a fresh read loop starts.
async fn flush_checkpoint(
    env: &Env,
    last_seq: i64,
    delta_seconds: i64,
) -> std::result::Result<(), String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let checkpoint = load_checkpoint(env).await?;

    let active_seconds_today = checkpoint.active_seconds_today + delta_seconds;
    let daily_budget = checkpoint.daily_budget_seconds;
    let status = if budget_exceeded(active_seconds_today, daily_budget) {
        "budget_exceeded"
    } else {
        "running"
    };

    let last_seq_arg = D1Type::Real(last_seq as f64);
    let active_seconds_arg = D1Type::Real(active_seconds_today as f64);
    let status_arg = D1Type::Text(status);
    db.prepare(
        r#"
        UPDATE atproto_firehose_checkpoint
        SET last_seq = ?1, active_seconds_today = ?2, status = ?3,
            reconnect_count = reconnect_count, updated_at = CURRENT_TIMESTAMP
        WHERE id = 'default'
        "#,
    )
    .bind_refs([&last_seq_arg, &active_seconds_arg, &status_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// The owner's own AT Protocol identity, derived the same way the `pds`
/// worker's `identity()` does: `did:web:{ACTIVITYPUB_DOMAIN}`. Router has no
/// other source of truth for this -- it's not stored in D1.
pub(crate) fn owner_did(env: &Env) -> String {
    let handle = env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());
    format!("did:web:{handle}")
}

/// Likes, follows, and replies observed on the firehose that target the
/// owner, as a single feed ordered by recency. A like/reply "targets the
/// owner" if its subject/parent at-uri starts with the owner's own DID --
/// this doesn't need to know the owner's individual post rkeys (which live
/// in the `pds` worker's `posts` table, not here) because every one of the
/// owner's own at-uris shares that prefix regardless of which post it is.
pub(crate) async fn owner_atproto_notifications(
    env: &Env,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let did = owner_did(env);
    let owner_prefix = format!("at://{did}/%");
    let prefix_arg = D1Type::Text(&owner_prefix);
    let did_arg = D1Type::Text(&did);
    let limit_arg = D1Type::Integer(limit.clamp(1, 200));

    let rows = db
        .prepare(
            r#"
            SELECT * FROM (
                SELECT 'like' AS kind, actor_did AS actor, subject_uri AS target, created_at
                FROM atproto_likes WHERE subject_uri LIKE ?1
                UNION ALL
                SELECT 'follow' AS kind, actor_did AS actor, subject_did AS target, created_at
                FROM atproto_follows WHERE subject_did = ?2
                UNION ALL
                SELECT 'reply' AS kind, actor_id AS actor, in_reply_to AS target,
                       published_at AS created_at
                FROM timeline_posts
                WHERE in_reply_to LIKE ?1 AND deleted_at IS NULL AND protocol = 'atproto'
            )
            ORDER BY created_at DESC
            LIMIT ?3
            "#,
        )
        .bind_refs([&prefix_arg, &did_arg, &limit_arg])
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;

    Ok(rows)
}

pub(crate) async fn owner_atproto_likes(
    env: &Env,
    subject_uri: &str,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let subject_arg = D1Type::Text(subject_uri);
    let limit_arg = D1Type::Integer(limit.clamp(1, 200));
    let rows = db
        .prepare(
            "SELECT uri, actor_did, subject_uri, subject_cid, created_at FROM atproto_likes \
             WHERE subject_uri = ?1 ORDER BY created_at DESC LIMIT ?2",
        )
        .bind_refs([&subject_arg, &limit_arg])
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub(crate) async fn owner_atproto_followers(
    env: &Env,
    subject_did: &str,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let subject_arg = D1Type::Text(subject_did);
    let limit_arg = D1Type::Integer(limit.clamp(1, 200));
    let rows = db
        .prepare(
            "SELECT uri, actor_did, subject_did, created_at FROM atproto_follows \
             WHERE subject_did = ?1 ORDER BY created_at DESC LIMIT ?2",
        )
        .bind_refs([&subject_arg, &limit_arg])
        .map_err(|error| error.to_string())?
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

async fn load_followed_dids(env: &Env) -> std::result::Result<HashSet<String>, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let rows = db
        .prepare(
            r#"
            SELECT target_actor_id FROM following
            WHERE status = 'accepted' AND target_actor_id LIKE 'did:%'
            "#,
        )
        .all()
        .await
        .map_err(|error| error.to_string())?
        .results::<Map<String, Value>>()
        .map_err(|error| error.to_string())?;

    Ok(rows
        .into_iter()
        .filter_map(|row| field_str(&row, "target_actor_id"))
        .collect())
}

fn field_str(row: &Map<String, Value>, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_string)
}

fn field_i64(row: &Map<String, Value>, key: &str) -> i64 {
    row.get(key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number as i64))
        })
        .unwrap_or(0)
}

/// Whether today's accumulated active time has crossed the daily budget --
/// a runaway-loop backstop (default 22h/day), not a routine duty cycle.
pub(crate) fn budget_exceeded(active_seconds_today: i64, daily_budget_seconds: i64) -> bool {
    active_seconds_today >= daily_budget_seconds
}

fn today_utc_date() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
        .chars()
        .take(10)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dais_core::atproto::mst::RepoChange;
    use dais_core::atproto::repo::CarBlock;

    fn record_change(action: &str, path: &str, json: Value) -> RepoChange {
        let bytes = serde_ipld_dagcbor::to_vec(&json_to_ipld(&json)).expect("encode record");
        match action {
            "create" => RepoChange::Created {
                path: path.to_string(),
                cid: sample_record_cid(),
                record_bytes: bytes,
            },
            "update" => RepoChange::Updated {
                path: path.to_string(),
                cid: sample_record_cid(),
                record_bytes: bytes,
            },
            "delete" => RepoChange::Deleted {
                path: path.to_string(),
            },
            other => panic!("unexpected action {other}"),
        }
    }

    fn sample_record_cid() -> cid::Cid {
        use multihash_codetable::{Code, MultihashDigest};
        cid::Cid::new_v1(0x71, Code::Sha2_256.digest(b"test"))
    }

    fn json_to_ipld(value: &Value) -> ipld_core::ipld::Ipld {
        use ipld_core::ipld::Ipld;
        match value {
            Value::Null => Ipld::Null,
            Value::Bool(b) => Ipld::Bool(*b),
            Value::Number(n) => Ipld::Integer(n.as_i64().unwrap_or(0) as i128),
            Value::String(s) => Ipld::String(s.clone()),
            Value::Array(items) => Ipld::List(items.iter().map(json_to_ipld).collect()),
            Value::Object(fields) => Ipld::Map(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_ipld(v)))
                    .collect(),
            ),
        }
    }

    #[allow(unused_imports)]
    use CarBlock as _UnusedCarBlockImportGuard;

    #[test]
    fn maps_a_post_create_into_an_upsert_post_write() {
        let change = record_change(
            "create",
            "app.bsky.feed.post/abc",
            serde_json::json!({"$type": "app.bsky.feed.post", "text": "hello", "createdAt": "2026-01-01T00:00:00Z"}),
        );
        let writes = changes_to_index_writes("did:plc:alice", vec![change]);
        assert_eq!(
            writes,
            vec![IndexWrite::UpsertPost {
                object_id: "at://did:plc:alice/app.bsky.feed.post/abc".to_string(),
                actor_id: "did:plc:alice".to_string(),
                content: "hello".to_string(),
                in_reply_to: None,
                published_at: "2026-01-01T00:00:00Z".to_string(),
            }]
        );
    }

    #[test]
    fn maps_a_reply_post_with_its_parent_uri() {
        let change = record_change(
            "create",
            "app.bsky.feed.post/reply1",
            serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": "yes",
                "createdAt": "2026-01-01T00:00:00Z",
                "reply": {
                    "parent": {"uri": "at://did:plc:bob/app.bsky.feed.post/parent1", "cid": "bafy"},
                    "root": {"uri": "at://did:plc:bob/app.bsky.feed.post/parent1", "cid": "bafy"}
                }
            }),
        );
        let writes = changes_to_index_writes("did:plc:alice", vec![change]);
        match &writes[0] {
            IndexWrite::UpsertPost { in_reply_to, .. } => {
                assert_eq!(
                    in_reply_to.as_deref(),
                    Some("at://did:plc:bob/app.bsky.feed.post/parent1")
                );
            }
            other => panic!("expected UpsertPost, got {other:?}"),
        }
    }

    #[test]
    fn maps_a_like_create_into_an_upsert_like_write() {
        let change = record_change(
            "create",
            "app.bsky.feed.like/xyz",
            serde_json::json!({
                "$type": "app.bsky.feed.like",
                "subject": {"uri": "at://did:plc:bob/app.bsky.feed.post/p1", "cid": "bafy123"},
                "createdAt": "2026-01-01T00:00:00Z"
            }),
        );
        let writes = changes_to_index_writes("did:plc:alice", vec![change]);
        assert_eq!(
            writes,
            vec![IndexWrite::UpsertLike {
                uri: "at://did:plc:alice/app.bsky.feed.like/xyz".to_string(),
                actor_did: "did:plc:alice".to_string(),
                subject_uri: "at://did:plc:bob/app.bsky.feed.post/p1".to_string(),
                subject_cid: Some("bafy123".to_string()),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            }]
        );
    }

    #[test]
    fn maps_a_follow_create_into_an_upsert_follow_write() {
        let change = record_change(
            "create",
            "app.bsky.graph.follow/f1",
            serde_json::json!({
                "$type": "app.bsky.graph.follow",
                "subject": "did:plc:carol",
                "createdAt": "2026-01-01T00:00:00Z"
            }),
        );
        let writes = changes_to_index_writes("did:plc:alice", vec![change]);
        assert_eq!(
            writes,
            vec![IndexWrite::UpsertFollow {
                uri: "at://did:plc:alice/app.bsky.graph.follow/f1".to_string(),
                actor_did: "did:plc:alice".to_string(),
                subject_did: "did:plc:carol".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            }]
        );
    }

    #[test]
    fn maps_deletes_for_each_known_collection() {
        let post_delete = record_change("delete", "app.bsky.feed.post/abc", Value::Null);
        let like_delete = record_change("delete", "app.bsky.feed.like/xyz", Value::Null);
        let follow_delete = record_change("delete", "app.bsky.graph.follow/f1", Value::Null);

        let writes = changes_to_index_writes(
            "did:plc:alice",
            vec![post_delete, like_delete, follow_delete],
        );

        assert_eq!(
            writes,
            vec![
                IndexWrite::DeletePost {
                    object_id: "at://did:plc:alice/app.bsky.feed.post/abc".to_string()
                },
                IndexWrite::DeleteLike {
                    uri: "at://did:plc:alice/app.bsky.feed.like/xyz".to_string()
                },
                IndexWrite::DeleteFollow {
                    uri: "at://did:plc:alice/app.bsky.graph.follow/f1".to_string()
                },
            ]
        );
    }

    #[test]
    fn ignores_collections_this_indexer_does_not_care_about() {
        let change = record_change(
            "create",
            "app.bsky.actor.profile/self",
            serde_json::json!({"$type": "app.bsky.actor.profile", "displayName": "Alice"}),
        );
        let writes = changes_to_index_writes("did:plc:alice", vec![change]);
        assert!(writes.is_empty());
    }

    #[test]
    fn relay_url_omits_cursor_on_first_run_and_includes_it_on_resume() {
        assert_eq!(
            relay_url(0),
            "wss://bsky.network/xrpc/com.atproto.sync.subscribeRepos"
        );
        assert_eq!(
            relay_url(42),
            "wss://bsky.network/xrpc/com.atproto.sync.subscribeRepos?cursor=42"
        );
    }

    #[test]
    fn budget_exceeded_trips_at_the_configured_threshold() {
        assert!(!budget_exceeded(79199, 79200));
        assert!(budget_exceeded(79200, 79200));
        assert!(budget_exceeded(90000, 79200));
    }
}
