//! Personal Bluesky AppView (issue #50 Track B, cost-optimized per issue
//! #377): a Durable Object that holds a persistent connection to
//! [Jetstream](https://github.com/bluesky-social/jetstream), Bluesky's
//! filtered JSON relay proxy, and indexes posts/likes/follows for DIDs the
//! owner follows into D1.
//!
//! The original design connected directly to Bluesky's raw relay
//! (`com.atproto.sync.subscribeRepos`), which has no server-side DID
//! filter -- it streamed every commit on the network, and decoding each one
//! (CBOR envelope + CAR blocks) before filtering burned through a
//! Cloudflare free-plan daily limit processing ~100% of Bluesky's global
//! traffic to extract a handful of followed accounts' data. Jetstream's
//! `wantedDids`/`wantedCollections` query params filter server-side, so this
//! consumer only ever receives events for followed accounts in the first
//! place.
//!
//! Jetstream's JSON is not independently verifiable -- Bluesky's own docs
//! state its events carry no signatures or MST proofs. So this does not
//! trust Jetstream's `record`/`cid` fields directly: for every matched
//! event it resolves the DID's real PDS (Jetstream only *detects* a change;
//! the aggregator relay does not implement `getRecord` at all, only the
//! account's own PDS does -- see [`resolve_pds_endpoint`]) and calls
//! `com.atproto.sync.getRecord` there, then verifies the response with
//! [`dais_core::atproto::record_proof::verify_record_proof`] -- the same
//! `decode_commit`/`mst_get` primitives issue #50's original design used,
//! reused here as an existence/non-existence proof rather than a commit-diff
//! walk. Only a verified result is written to D1.
//!
//! The read loop (`run_jetstream_loop`) is spawned once via
//! `State::wait_until`, which keeps this Durable Object resident for as long
//! as the loop's future is pending, and owns its own clone of the
//! `WebSocket` -- the struct's own `socket` field exists only so `alarm()`'s
//! watchdog can close the shared connection for budget enforcement.
//! `alarm()` itself is a periodic watchdog, not the per-message driver: it
//! re-starts the read loop if it isn't running (including after a cold
//! restart, when the struct's in-memory `socket` field has reset to `None`),
//! and enforces the daily active-time budget as a runaway-loop backstop.
//!
//! [`ensure_firehose_subscription_running`] is what actually starts this --
//! called from the router's existing 30-minute cron, gated behind the
//! `FIREHOSE_ENABLED` var (unset/absent means disabled). Deploying this
//! code does not by itself open a connection to Bluesky in any environment;
//! that var is only meant to be set once the live smoke test in issue #50
//! has passed for that environment.

use dais_core::atproto::firehose::record_bytes_to_json;
use dais_core::atproto::record_proof::{verify_record_proof, RecordProof};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;
use worker::*;

const JETSTREAM_HOST: &str = "jetstream2.us-east.bsky.network";
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(60);
/// How often the read loop persists its cursor and re-checks the follow
/// list, in processed events. A crash between flushes just means replaying
/// up to this many already-seen events on reconnect (Jetstream's `cursor`
/// replays inclusively) -- harmless, since every D1 write here is an
/// idempotent upsert/delete keyed by URI.
const FLUSH_EVERY_MESSAGES: u32 = 200;
/// Collections this consumer indexes; also Jetstream's server-side
/// collection filter, so events outside this set are never even sent to us.
const WANTED_COLLECTIONS: [&str; 3] = [
    "app.bsky.feed.post",
    "app.bsky.feed.like",
    "app.bsky.graph.follow",
];

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

        let url = Url::parse(&jetstream_url(checkpoint.last_seq, &dids))
            .map_err(|error| Error::RustError(format!("invalid jetstream url: {error}")))?;
        let ws = WebSocket::connect(url).await?;
        *self.socket.borrow_mut() = Some(ws.clone());
        mark_running(&self.env).await.map_err(Error::RustError)?;

        let task_env = self.env.clone();
        let task_socket = self.socket.clone();
        self.state.wait_until(async move {
            if let Err(error) = run_jetstream_loop(task_env, ws, dids).await {
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

/// A Jetstream event, per <https://github.com/bluesky-social/jetstream>'s
/// wire format. Only the fields needed to *detect* a change and locate the
/// real record are parsed -- `record`/`record_cbor` are deliberately not
/// modeled here, since they are exactly the unverified data this consumer
/// never trusts (see the module doc comment).
#[derive(Debug, Deserialize)]
struct JetstreamEvent {
    did: String,
    time_us: i64,
    commit: Option<JetstreamCommit>,
}

/// Deliberately does not carry Jetstream's own claimed `operation`
/// (create/update/delete): [`handle_commit_event`] always asks the PDS for
/// the record's *current* verified state via `getRecord` and acts on that,
/// so a stale or wrong operation label from Jetstream can't cause a wrong
/// write -- it can at most cause an extra (harmless, idempotent) check.
#[derive(Debug, Deserialize)]
struct JetstreamCommit {
    collection: String,
    rkey: String,
}

fn jetstream_url(cursor: i64, dids: &HashSet<String>) -> String {
    let mut url =
        Url::parse(&format!("wss://{JETSTREAM_HOST}/subscribe")).expect("static url is valid");
    {
        let mut pairs = url.query_pairs_mut();
        for collection in WANTED_COLLECTIONS {
            pairs.append_pair("wantedCollections", collection);
        }
        for did in dids {
            pairs.append_pair("wantedDids", did);
        }
        if cursor > 0 {
            pairs.append_pair("cursor", &cursor.to_string());
        }
    }
    url.to_string()
}

async fn run_jetstream_loop(env: Env, ws: WebSocket, mut dids: HashSet<String>) -> Result<()> {
    use futures_util::StreamExt;

    console_log!("firehose read loop starting, {} followed dids", dids.len());
    let mut events = ws.events()?;
    ws.accept()?;
    let mut processed_since_flush: u32 = 0;
    let mut total_processed: u64 = 0;
    let mut last_cursor: i64 = 0;
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
        let Some(text) = message.text() else {
            console_log!("firehose read loop got a non-text message, skipping");
            continue; // Jetstream's uncompressed wire is JSON text frames.
        };
        total_processed += 1;

        match serde_json::from_str::<JetstreamEvent>(&text) {
            Ok(jetstream_event) => {
                last_cursor = jetstream_event.time_us;
                // wantedDids already filters server-side; this check is a
                // defense-in-depth backstop, not the primary filter.
                if dids.contains(&jetstream_event.did) {
                    if let Some(commit) = &jetstream_event.commit {
                        if let Err(error) =
                            handle_commit_event(&env, &jetstream_event.did, commit).await
                        {
                            console_log!("firehose commit handling failed: {error}");
                        }
                    }
                }
            }
            Err(error) => {
                console_log!("firehose jetstream event parse failed: {error}");
            }
        }

        if total_processed == 1 {
            // Flush immediately on the very first message so the checkpoint
            // row is an externally-queryable (D1) heartbeat -- logs emitted
            // from inside this wait_until-detached loop don't reliably reach
            // `wrangler tail`, so D1 is the only observable signal that the
            // socket is actually receiving frames.
            if let Err(error) = flush_checkpoint(&env, last_cursor, 0).await {
                console_log!("firehose first-message checkpoint flush failed: {error}");
            }
        }

        processed_since_flush += 1;
        if processed_since_flush >= FLUSH_EVERY_MESSAGES {
            processed_since_flush = 0;
            let now = js_sys::Date::now();
            let delta_seconds = ((now - last_flush_at) / 1000.0) as i64;
            last_flush_at = now;
            if let Err(error) = flush_checkpoint(&env, last_cursor, delta_seconds.max(0)).await {
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

/// Resolves the record Jetstream claims changed, independently verifies it
/// against the owning PDS's own `getRecord` proof, and only then applies the
/// resulting D1 write. This is the entire point of the hybrid design: a
/// compromised or buggy Jetstream instance can at most cause us to *check*
/// the wrong thing, never to *store* unverified content.
async fn handle_commit_event(
    env: &Env,
    did: &str,
    commit: &JetstreamCommit,
) -> std::result::Result<(), String> {
    if !WANTED_COLLECTIONS.contains(&commit.collection.as_str()) {
        return Ok(());
    }

    let pds_endpoint = resolve_pds_endpoint(env, did).await?;
    let car_bytes = fetch_get_record(&pds_endpoint, did, &commit.collection, &commit.rkey).await?;
    let proof = verify_record_proof(&car_bytes, did, &commit.collection, &commit.rkey)
        .map_err(|error| error.to_string())?;

    let uri = format!("at://{did}/{}/{}", commit.collection, commit.rkey);
    let write = match proof {
        RecordProof::Present { record_bytes, .. } => {
            let json = record_bytes_to_json(&record_bytes).map_err(|error| error.to_string())?;
            index_write_for_upsert(did, &commit.collection, &uri, &json)
        }
        RecordProof::Absent => index_write_for_delete(&commit.collection, &uri),
    };

    match write {
        Some(write) => apply_index_write(env, &write).await,
        None => Ok(()),
    }
}

/// Looks up (and caches in D1) the PDS host that actually serves `did`'s
/// repo. Bluesky's aggregator relay does not implement
/// `com.atproto.sync.getRecord` itself -- only the account's own PDS does --
/// so this resolution is required before every `getRecord` call, not an
/// optimization.
async fn resolve_pds_endpoint(env: &Env, did: &str) -> std::result::Result<String, String> {
    let db = env.d1("DB").map_err(|error| error.to_string())?;
    let did_arg = D1Type::Text(did);
    let cached = db
        .prepare("SELECT pds_endpoint FROM atproto_pds_cache WHERE did = ?1")
        .bind_refs([&did_arg])
        .map_err(|error| error.to_string())?
        .first::<Map<String, Value>>(None)
        .await
        .map_err(|error| error.to_string())?
        .and_then(|row| field_str(&row, "pds_endpoint"));
    if let Some(endpoint) = cached {
        return Ok(endpoint);
    }

    let endpoint = fetch_pds_endpoint_from_did_document(did).await?;

    let endpoint_arg = D1Type::Text(&endpoint);
    db.prepare(
        "INSERT INTO atproto_pds_cache (did, pds_endpoint) VALUES (?1, ?2) \
         ON CONFLICT(did) DO UPDATE SET pds_endpoint = excluded.pds_endpoint, \
         resolved_at = CURRENT_TIMESTAMP",
    )
    .bind_refs([&did_arg, &endpoint_arg])
    .map_err(|error| error.to_string())?
    .run()
    .await
    .map_err(|error| error.to_string())?;

    Ok(endpoint)
}

async fn fetch_pds_endpoint_from_did_document(did: &str) -> std::result::Result<String, String> {
    let doc_url = if did.starts_with("did:plc:") {
        format!("https://plc.directory/{did}")
    } else if let Some(web_domain) = did.strip_prefix("did:web:") {
        // did:web path-component encoding (`:` -> `/`) is rare in practice
        // for Bluesky accounts, but part of the did:web spec.
        format!(
            "https://{}/.well-known/did.json",
            web_domain.replace(':', "/")
        )
    } else {
        return Err(format!("unsupported did method for '{did}'"));
    };

    let document = crate::activitypub::fetch_json_with_accept_and_headers(
        &doc_url,
        "application/json",
        "did document",
        &[],
    )
    .await?;

    document
        .get("service")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|service| service.get("id").and_then(Value::as_str) == Some("#atproto_pds"))
        .and_then(|service| service.get("serviceEndpoint"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("did document for '{did}' has no #atproto_pds service endpoint"))
}

async fn fetch_get_record(
    pds_endpoint: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) -> std::result::Result<Vec<u8>, String> {
    let mut url = Url::parse(&format!("{pds_endpoint}/xrpc/com.atproto.sync.getRecord"))
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("did", did)
        .append_pair("collection", collection)
        .append_pair("rkey", rkey);

    let request =
        Request::new(url.as_str(), worker::Method::Get).map_err(|error| error.to_string())?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status_code();
    if !(200..=299).contains(&status) {
        return Err(format!(
            "getRecord for {did}/{collection}/{rkey} at {pds_endpoint} failed with HTTP {status}"
        ));
    }
    response.bytes().await.map_err(|error| error.to_string())
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

/// Maps a verified, present record to its D1 write. `did`/`uri` come from
/// the caller (already independently confirmed by [`verify_record_proof`]);
/// `json` is the decoded record body.
pub(crate) fn index_write_for_upsert(
    did: &str,
    collection: &str,
    uri: &str,
    json: &Value,
) -> Option<IndexWrite> {
    match collection {
        "app.bsky.feed.post" => Some(IndexWrite::UpsertPost {
            object_id: uri.to_string(),
            actor_id: did.to_string(),
            content: json_str(json, "text"),
            in_reply_to: json
                .get("reply")
                .and_then(|reply| reply.get("parent"))
                .and_then(|parent| parent.get("uri"))
                .and_then(Value::as_str)
                .map(str::to_string),
            published_at: json_str(json, "createdAt"),
        }),
        "app.bsky.feed.like" => Some(IndexWrite::UpsertLike {
            uri: uri.to_string(),
            actor_did: did.to_string(),
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
            created_at: json_str(json, "createdAt"),
        }),
        "app.bsky.graph.follow" => Some(IndexWrite::UpsertFollow {
            uri: uri.to_string(),
            actor_did: did.to_string(),
            subject_did: json
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            created_at: json_str(json, "createdAt"),
        }),
        _ => None,
    }
}

/// Maps a verified-absent record to its D1 delete.
pub(crate) fn index_write_for_delete(collection: &str, uri: &str) -> Option<IndexWrite> {
    match collection {
        "app.bsky.feed.post" => Some(IndexWrite::DeletePost {
            object_id: uri.to_string(),
        }),
        "app.bsky.feed.like" => Some(IndexWrite::DeleteLike {
            uri: uri.to_string(),
        }),
        "app.bsky.graph.follow" => Some(IndexWrite::DeleteFollow {
            uri: uri.to_string(),
        }),
        _ => None,
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

    #[test]
    fn maps_a_post_create_into_an_upsert_post_write() {
        let json = serde_json::json!({"$type": "app.bsky.feed.post", "text": "hello", "createdAt": "2026-01-01T00:00:00Z"});
        let write = index_write_for_upsert(
            "did:plc:alice",
            "app.bsky.feed.post",
            "at://did:plc:alice/app.bsky.feed.post/abc",
            &json,
        );
        assert_eq!(
            write,
            Some(IndexWrite::UpsertPost {
                object_id: "at://did:plc:alice/app.bsky.feed.post/abc".to_string(),
                actor_id: "did:plc:alice".to_string(),
                content: "hello".to_string(),
                in_reply_to: None,
                published_at: "2026-01-01T00:00:00Z".to_string(),
            })
        );
    }

    #[test]
    fn maps_a_reply_post_with_its_parent_uri() {
        let json = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "yes",
            "createdAt": "2026-01-01T00:00:00Z",
            "reply": {
                "parent": {"uri": "at://did:plc:bob/app.bsky.feed.post/parent1", "cid": "bafy"},
                "root": {"uri": "at://did:plc:bob/app.bsky.feed.post/parent1", "cid": "bafy"}
            }
        });
        let write = index_write_for_upsert(
            "did:plc:alice",
            "app.bsky.feed.post",
            "at://did:plc:alice/app.bsky.feed.post/reply1",
            &json,
        );
        match write {
            Some(IndexWrite::UpsertPost { in_reply_to, .. }) => {
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
        let json = serde_json::json!({
            "$type": "app.bsky.feed.like",
            "subject": {"uri": "at://did:plc:bob/app.bsky.feed.post/p1", "cid": "bafy123"},
            "createdAt": "2026-01-01T00:00:00Z"
        });
        let write = index_write_for_upsert(
            "did:plc:alice",
            "app.bsky.feed.like",
            "at://did:plc:alice/app.bsky.feed.like/xyz",
            &json,
        );
        assert_eq!(
            write,
            Some(IndexWrite::UpsertLike {
                uri: "at://did:plc:alice/app.bsky.feed.like/xyz".to_string(),
                actor_did: "did:plc:alice".to_string(),
                subject_uri: "at://did:plc:bob/app.bsky.feed.post/p1".to_string(),
                subject_cid: Some("bafy123".to_string()),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            })
        );
    }

    #[test]
    fn maps_a_follow_create_into_an_upsert_follow_write() {
        let json = serde_json::json!({
            "$type": "app.bsky.graph.follow",
            "subject": "did:plc:carol",
            "createdAt": "2026-01-01T00:00:00Z"
        });
        let write = index_write_for_upsert(
            "did:plc:alice",
            "app.bsky.graph.follow",
            "at://did:plc:alice/app.bsky.graph.follow/f1",
            &json,
        );
        assert_eq!(
            write,
            Some(IndexWrite::UpsertFollow {
                uri: "at://did:plc:alice/app.bsky.graph.follow/f1".to_string(),
                actor_did: "did:plc:alice".to_string(),
                subject_did: "did:plc:carol".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            })
        );
    }

    #[test]
    fn maps_deletes_for_each_known_collection() {
        assert_eq!(
            index_write_for_delete(
                "app.bsky.feed.post",
                "at://did:plc:alice/app.bsky.feed.post/abc"
            ),
            Some(IndexWrite::DeletePost {
                object_id: "at://did:plc:alice/app.bsky.feed.post/abc".to_string()
            })
        );
        assert_eq!(
            index_write_for_delete(
                "app.bsky.feed.like",
                "at://did:plc:alice/app.bsky.feed.like/xyz"
            ),
            Some(IndexWrite::DeleteLike {
                uri: "at://did:plc:alice/app.bsky.feed.like/xyz".to_string()
            })
        );
        assert_eq!(
            index_write_for_delete(
                "app.bsky.graph.follow",
                "at://did:plc:alice/app.bsky.graph.follow/f1"
            ),
            Some(IndexWrite::DeleteFollow {
                uri: "at://did:plc:alice/app.bsky.graph.follow/f1".to_string()
            })
        );
    }

    #[test]
    fn ignores_collections_this_indexer_does_not_care_about() {
        let json = serde_json::json!({"$type": "app.bsky.actor.profile", "displayName": "Alice"});
        assert_eq!(
            index_write_for_upsert(
                "did:plc:alice",
                "app.bsky.actor.profile",
                "at://did:plc:alice/app.bsky.actor.profile/self",
                &json
            ),
            None
        );
        assert_eq!(
            index_write_for_delete(
                "app.bsky.actor.profile",
                "at://did:plc:alice/app.bsky.actor.profile/self"
            ),
            None
        );
    }

    #[test]
    fn parses_a_real_shaped_jetstream_commit_event_extracting_only_what_it_needs() {
        // Shape confirmed live against wss://jetstream2.us-east.bsky.network/subscribe;
        // content is synthetic (never commit real captured post text/dids).
        let text = r#"{"did":"did:plc:example","time_us":1784051382391244,"kind":"commit","commit":{"rev":"3mqmng4deol2x","operation":"create","collection":"app.bsky.feed.post","rkey":"3mqmng4f67c2x","cid":"bafyreiexample","record":{"$type":"app.bsky.feed.post","text":"hi","createdAt":"2026-01-01T00:00:00Z"}}}"#;
        let event: JetstreamEvent = serde_json::from_str(text).expect("parse");
        assert_eq!(event.did, "did:plc:example");
        assert_eq!(event.time_us, 1784051382391244);
        let commit = event.commit.expect("commit present");
        assert_eq!(commit.collection, "app.bsky.feed.post");
        assert_eq!(commit.rkey, "3mqmng4f67c2x");
    }

    #[test]
    fn parses_a_jetstream_delete_event_with_no_record_or_cid() {
        let text = r#"{"did":"did:plc:example","time_us":1784051384077720,"kind":"commit","commit":{"rev":"3mqmngb4okb22","operation":"delete","collection":"app.bsky.feed.post","rkey":"3ly3sp7j4d22d"}}"#;
        let event: JetstreamEvent = serde_json::from_str(text).expect("parse");
        let commit = event.commit.expect("commit present");
        assert_eq!(commit.collection, "app.bsky.feed.post");
        assert_eq!(commit.rkey, "3ly3sp7j4d22d");
    }

    #[test]
    fn parses_a_jetstream_account_event_with_no_commit_field() {
        let text = r#"{"did":"did:plc:example","time_us":1784051384522819,"kind":"account","account":{"active":false,"did":"did:plc:example","seq":31832282146,"status":"deleted","time":"2026-01-01T00:00:00Z"}}"#;
        let event: JetstreamEvent = serde_json::from_str(text).expect("parse");
        assert!(event.commit.is_none());
    }

    #[test]
    fn jetstream_url_omits_cursor_on_first_run_and_includes_it_on_resume() {
        let dids = HashSet::new();
        let url = jetstream_url(0, &dids);
        assert!(url.starts_with("wss://jetstream2.us-east.bsky.network/subscribe?"));
        assert!(!url.contains("cursor="));
        assert!(url.contains("wantedCollections=app.bsky.feed.post"));
        assert!(url.contains("wantedCollections=app.bsky.feed.like"));
        assert!(url.contains("wantedCollections=app.bsky.graph.follow"));

        let resumed = jetstream_url(1_700_000_000_000_000, &dids);
        assert!(resumed.contains("cursor=1700000000000000"));
    }

    #[test]
    fn jetstream_url_includes_every_followed_did() {
        let mut dids = HashSet::new();
        dids.insert("did:plc:alice".to_string());
        let url = jetstream_url(0, &dids);
        assert!(url.contains("wantedDids=did%3Aplc%3Aalice"));
    }

    #[test]
    fn budget_exceeded_trips_at_the_configured_threshold() {
        assert!(!budget_exceeded(79199, 79200));
        assert!(budget_exceeded(79200, 79200));
        assert!(budget_exceeded(90000, 79200));
    }
}
