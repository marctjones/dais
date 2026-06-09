//! Outbound federation: resolve remote actors (WebFinger), build ActivityPub
//! activities, and deliver them signed.
//!
//! Delivery + signing reuse `dais_core::activitypub::delivery::deliver_to_inbox`
//! (the same code the Workers use) via the native [`crate::platform`] adapters, so
//! there is one audited signing/delivery path. Only the outbound *resolution* and
//! activity *construction* live here (core's webfinger is the inbound/server side).

use std::collections::HashMap;

use chrono::Utc;
use dais_core::traits::{HttpProvider, Method, Request as CoreRequest};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::model::Visibility;

const AS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";
const ACTIVITY_JSON: &str = "application/activity+json";

/// A resolved remote actor.
#[derive(Debug, Clone)]
pub struct ActorRef {
    pub id: String,
    pub inbox: String,
    pub shared_inbox: Option<String>,
    pub preferred_username: Option<String>,
    /// `publicKey.id` (the keyId), if present — used to wrap E2EE DMs.
    pub key_id: Option<String>,
    /// `publicKey.publicKeyPem`, if present.
    pub public_key_pem: Option<String>,
}

/// Resolve `@user@host` to its actor id + inbox via WebFinger then actor fetch.
pub async fn resolve(http: &dyn HttpProvider, handle: &str) -> Result<ActorRef> {
    let h = handle.trim().trim_start_matches('@');
    let (user, host) = h
        .split_once('@')
        .ok_or_else(|| Error::other(format!("invalid handle (want @user@host): {handle}")))?;

    // 1. WebFinger → actor URL.
    let wf_url =
        format!("https://{host}/.well-known/webfinger?resource=acct:{user}@{host}");
    let wf = http_get(http, &wf_url, "application/jrd+json").await?;
    if !(200..300).contains(&wf.status) {
        return Err(Error::other(format!(
            "webfinger {handle}: HTTP {}",
            wf.status
        )));
    }
    let jrd: Value = serde_json::from_slice(&wf.body)?;
    let actor_url = jrd["links"]
        .as_array()
        .and_then(|links| {
            links.iter().find(|l| {
                l["rel"] == "self"
                    && l["type"]
                        .as_str()
                        .map(|t| t.contains("activity"))
                        .unwrap_or(false)
            })
        })
        .and_then(|l| l["href"].as_str())
        .ok_or_else(|| Error::other(format!("webfinger {handle}: no activity+json self link")))?
        .to_string();
    require_https(&actor_url, "actor URL")?;

    // 2. Fetch the actor object → inbox.
    let actor_resp = http_get(http, &actor_url, ACTIVITY_JSON).await?;
    if !(200..300).contains(&actor_resp.status) {
        return Err(Error::other(format!(
            "fetch actor {actor_url}: HTTP {}",
            actor_resp.status
        )));
    }
    let actor: Value = serde_json::from_slice(&actor_resp.body)?;
    let id = actor["id"].as_str().unwrap_or(&actor_url).to_string();
    let inbox = actor["inbox"]
        .as_str()
        .ok_or_else(|| Error::other(format!("actor {actor_url} has no inbox")))?
        .to_string();
    require_https(&inbox, "actor inbox")?;
    let shared_inbox = actor["endpoints"]["sharedInbox"]
        .as_str()
        .map(str::to_string);
    let preferred_username = actor["preferredUsername"].as_str().map(str::to_string);
    let key_id = actor["publicKey"]["id"].as_str().map(str::to_string);
    let public_key_pem = actor["publicKey"]["publicKeyPem"].as_str().map(str::to_string);

    Ok(ActorRef {
        id,
        inbox,
        shared_inbox,
        preferred_username,
        key_id,
        public_key_pem,
    })
}

async fn http_get(
    http: &dyn HttpProvider,
    url: &str,
    accept: &str,
) -> Result<dais_core::traits::Response> {
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), accept.to_string());
    let req = CoreRequest {
        url: url.to_string(),
        method: Method::Get,
        headers,
        body: None,
        timeout: Some(30),
        follow_redirects: true,
    };
    http.fetch(req)
        .await
        .map_err(|e| Error::other(format!("GET {url}: {e}")))
}

/// Require an `https://` URL — a single chokepoint against delivering signed
/// requests (or fetching) to `http://localhost`, internal services, or other
/// non-https targets injected via remote actor/inbox data. Every real fediverse
/// endpoint is https, so this rejects only abuse.
fn require_https(url: &str, what: &str) -> Result<()> {
    if url.starts_with("https://") {
        Ok(())
    } else {
        Err(Error::other(format!("{what} must be https, refusing: {url}")))
    }
}

/// Deliver a pre-built activity JSON to a remote inbox, signed with our key.
/// Reuses core's `deliver_to_inbox` (HTTP Signatures + digest).
pub async fn deliver(
    http: &dyn HttpProvider,
    inbox_url: &str,
    actor_url: &str,
    activity_json: &str,
    private_key_pem: &str,
) -> Result<()> {
    // Guards every delivery path (post/follow/accept/reject/dm/undo), including the
    // approve/reject path that reads `follower_inbox` from D1 rather than re-resolving.
    require_https(inbox_url, "delivery inbox")?;
    dais_core::activitypub::deliver_to_inbox(http, inbox_url, actor_url, activity_json, private_key_pem)
        .await
        .map_err(|e| Error::other(format!("deliver to {inbox_url}: {e}")))
}

// ---- activity builders (pure) --------------------------------------------

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn new_id(actor_url: &str, kind: &str) -> String {
    format!("{actor_url}/{kind}/{}", uuid::Uuid::new_v4())
}

/// Addressing (`to`/`cc`) for a visibility tier.
fn addressing(actor_url: &str, visibility: Visibility, direct_to: &[String]) -> (Vec<String>, Vec<String>) {
    let public = format!("{AS_CONTEXT}#Public");
    let followers = format!("{actor_url}/followers");
    match visibility {
        Visibility::Public => (vec![public], vec![followers]),
        Visibility::Followers => (vec![followers], vec![]),
        Visibility::Direct => (direct_to.to_vec(), vec![]),
    }
}

/// Build a `Note` object (returns the note id and the JSON).
#[allow(clippy::too_many_arguments)]
pub fn build_note(
    actor_url: &str,
    content: &str,
    visibility: Visibility,
    direct_to: &[String],
    in_reply_to: Option<&str>,
    encrypted_message: Option<&Value>,
) -> (String, Value) {
    let note_id = new_id(actor_url, "posts");
    let (to, cc) = addressing(actor_url, visibility, direct_to);
    let published = now_rfc3339();

    let mut note = json!({
        "id": note_id,
        "type": "Note",
        "attributedTo": actor_url,
        "content": content,
        "published": published,
        "to": to,
        "cc": cc,
    });
    if let Some(reply) = in_reply_to {
        note["inReplyTo"] = json!(reply);
    }
    if let Some(enc) = encrypted_message {
        // dais E2EE extension: ciphertext rides alongside the fallback `content`.
        note["encryptedMessage"] = enc.clone();
    }
    (note_id, note)
}

/// Wrap a Note in a `Create` activity.
pub fn build_create(actor_url: &str, note: &Value) -> Value {
    let to = note["to"].clone();
    let cc = note["cc"].clone();
    json!({
        "@context": AS_CONTEXT,
        "id": format!("{}/activity", note["id"].as_str().unwrap_or_default()),
        "type": "Create",
        "actor": actor_url,
        "published": note["published"].clone(),
        "to": to,
        "cc": cc,
        "object": note,
    })
}

/// Build a `Follow` activity targeting `target_actor`.
pub fn build_follow(actor_url: &str, target_actor: &str) -> (String, Value) {
    let id = new_id(actor_url, "activities");
    let activity = json!({
        "@context": AS_CONTEXT,
        "id": id,
        "type": "Follow",
        "actor": actor_url,
        "object": target_actor,
    });
    (id, activity)
}

/// Build an `Undo` of a previously sent `Follow`.
pub fn build_undo_follow(actor_url: &str, follow_id: &str, target_actor: &str) -> Value {
    json!({
        "@context": AS_CONTEXT,
        "id": new_id(actor_url, "activities"),
        "type": "Undo",
        "actor": actor_url,
        "object": {
            "id": follow_id,
            "type": "Follow",
            "actor": actor_url,
            "object": target_actor,
        },
    })
}

/// Build an `Accept` of an incoming `Follow` from `follower_actor`.
/// `follow_id` echoes the original Follow's id when known (improves interop).
pub fn build_accept(actor_url: &str, follower_actor: &str, follow_id: Option<&str>) -> Value {
    let mut follow_obj = json!({
        "type": "Follow",
        "actor": follower_actor,
        "object": actor_url,
    });
    if let Some(fid) = follow_id {
        follow_obj["id"] = json!(fid);
    }
    json!({
        "@context": AS_CONTEXT,
        "id": new_id(actor_url, "activities"),
        "type": "Accept",
        "actor": actor_url,
        "object": follow_obj,
    })
}

/// Build a `Reject` of an incoming `Follow` from `follower_actor`.
pub fn build_reject(actor_url: &str, follower_actor: &str, follow_id: Option<&str>) -> Value {
    let mut follow_obj = json!({
        "type": "Follow",
        "actor": follower_actor,
        "object": actor_url,
    });
    if let Some(fid) = follow_id {
        follow_obj["id"] = json!(fid);
    }
    json!({
        "@context": AS_CONTEXT,
        "id": new_id(actor_url, "activities"),
        "type": "Reject",
        "actor": actor_url,
        "object": follow_obj,
    })
}

/// Build a `Delete` activity for one of our objects.
pub fn build_delete(actor_url: &str, object_id: &str) -> Value {
    json!({
        "@context": AS_CONTEXT,
        "id": new_id(actor_url, "activities"),
        "type": "Delete",
        "actor": actor_url,
        "to": [format!("{AS_CONTEXT}#Public")],
        "object": {
            "id": object_id,
            "type": "Tombstone",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_addressing_by_visibility() {
        let actor = "https://social.dais.social/users/social";
        let (_, public) = build_note(actor, "hi", Visibility::Public, &[], None, None);
        assert_eq!(public["to"][0], format!("{AS_CONTEXT}#Public"));
        assert_eq!(public["cc"][0], format!("{actor}/followers"));

        let (_, followers) = build_note(actor, "hi", Visibility::Followers, &[], None, None);
        assert_eq!(followers["to"][0], format!("{actor}/followers"));
        assert_eq!(followers["cc"].as_array().unwrap().len(), 0);

        let (_, direct) =
            build_note(actor, "hi", Visibility::Direct, &["https://x/u".into()], None, None);
        assert_eq!(direct["to"][0], "https://x/u");
    }

    #[test]
    fn create_wraps_note_and_copies_addressing() {
        let actor = "https://social.dais.social/users/social";
        let (_id, note) = build_note(actor, "gm", Visibility::Followers, &[], None, None);
        let create = build_create(actor, &note);
        assert_eq!(create["type"], "Create");
        assert_eq!(create["actor"], actor);
        assert_eq!(create["object"]["content"], "gm");
        assert_eq!(create["to"], note["to"]);
    }

    #[test]
    fn follow_and_accept_shapes() {
        let me = "https://social.dais.social/users/social";
        let (fid, follow) = build_follow(me, "https://remote/users/bob");
        assert_eq!(follow["type"], "Follow");
        assert_eq!(follow["object"], "https://remote/users/bob");
        assert!(fid.starts_with(me));

        let accept = build_accept(me, "https://remote/users/bob", Some("https://remote/act/1"));
        assert_eq!(accept["type"], "Accept");
        assert_eq!(accept["object"]["id"], "https://remote/act/1");
        assert_eq!(accept["object"]["object"], me);
    }

    // ---- network path (mock HttpProvider, no real I/O) -------------------

    use dais_core::traits::{HttpProvider, PlatformResult, Response as CoreResponse};
    use std::cell::RefCell;

    /// A canned HTTP provider: serves WebFinger + actor JSON, captures POSTs.
    struct FakeHttp {
        sent: RefCell<Vec<CoreRequest>>,
    }

    #[async_trait::async_trait(?Send)]
    impl HttpProvider for FakeHttp {
        async fn fetch(&self, request: CoreRequest) -> PlatformResult<CoreResponse> {
            let url = request.url.clone();
            if request.method == Method::Post {
                self.sent.borrow_mut().push(request.clone());
                return Ok(CoreResponse { status: 202, headers: HashMap::new(), body: vec![], url });
            }
            let body: Vec<u8> = if url.contains(".well-known/webfinger") {
                br#"{"links":[{"rel":"self","type":"application/activity+json","href":"https://remote.example/users/bob"}]}"#.to_vec()
            } else if url == "https://remote.example/users/bob" {
                br#"{"id":"https://remote.example/users/bob","inbox":"https://remote.example/users/bob/inbox","preferredUsername":"bob","publicKey":{"id":"https://remote.example/users/bob#main-key","publicKeyPem":"x"}}"#.to_vec()
            } else {
                b"{}".to_vec()
            };
            Ok(CoreResponse { status: 200, headers: HashMap::new(), body, url })
        }
    }

    fn test_key_pem() -> String {
        use dais_shared::rsa::pkcs8::{EncodePrivateKey, LineEnding};
        use dais_shared::rsa::RsaPrivateKey;
        use rand_core::OsRng;
        let key = RsaPrivateKey::new(&mut OsRng, 2048).expect("keygen");
        key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string()
    }

    #[tokio::test]
    async fn resolve_follows_webfinger_to_inbox() {
        let http = FakeHttp { sent: RefCell::new(vec![]) };
        let actor = resolve(&http, "@bob@remote.example").await.unwrap();
        assert_eq!(actor.id, "https://remote.example/users/bob");
        assert_eq!(actor.inbox, "https://remote.example/users/bob/inbox");
        assert_eq!(actor.key_id.as_deref(), Some("https://remote.example/users/bob#main-key"));
    }

    #[tokio::test]
    async fn deliver_signs_the_request() {
        let http = FakeHttp { sent: RefCell::new(vec![]) };
        let actor = "https://social.dais.social/users/social";
        let (_id, note) = build_note(actor, "gm", Visibility::Followers, &[], None, None);
        let activity = serde_json::to_string(&build_create(actor, &note)).unwrap();

        deliver(&http, "https://remote.example/users/bob/inbox", actor, &activity, &test_key_pem())
            .await
            .unwrap();

        let sent = http.sent.borrow();
        assert_eq!(sent.len(), 1, "exactly one delivery POST");
        let req = &sent[0];
        assert_eq!(req.method, Method::Post);
        // Signed + integrity-protected, per HTTP Signatures.
        let sig = req.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("signature"));
        assert!(sig.is_some(), "missing Signature header: {:?}", req.headers);
        assert!(sig.unwrap().1.contains("keyId="), "malformed signature");
        assert!(
            req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("digest")),
            "missing Digest header"
        );
    }

    struct EvilHttp;
    #[async_trait::async_trait(?Send)]
    impl HttpProvider for EvilHttp {
        async fn fetch(&self, request: CoreRequest) -> PlatformResult<CoreResponse> {
            // Serves a WebFinger pointing the actor at an internal http:// URL.
            let body = br#"{"links":[{"rel":"self","type":"application/activity+json","href":"http://localhost/users/x"}]}"#.to_vec();
            Ok(CoreResponse { status: 200, headers: HashMap::new(), body, url: request.url })
        }
    }

    #[tokio::test]
    async fn deliver_refuses_non_https_inbox() {
        let http = FakeHttp { sent: RefCell::new(vec![]) };
        let err = deliver(&http, "http://localhost:6379/inbox", "https://me/users/x", "{}", "k").await;
        assert!(err.is_err(), "must refuse non-https inbox (SSRF guard)");
        assert_eq!(http.sent.borrow().len(), 0, "nothing should be sent");
    }

    #[tokio::test]
    async fn resolve_refuses_non_https_actor() {
        let err = resolve(&EvilHttp, "@x@evil.example").await;
        assert!(err.is_err(), "must reject an http:// actor URL from WebFinger");
    }

    #[test]
    fn reply_and_encrypted_note() {
        let actor = "https://social.dais.social/users/social";
        let enc = json!({"v": 1, "alg": "AES-256-GCM"});
        let (_id, note) = build_note(
            actor,
            "🔒 encrypted — open in dais",
            Visibility::Followers,
            &[],
            Some("https://remote/posts/9"),
            Some(&enc),
        );
        assert_eq!(note["inReplyTo"], "https://remote/posts/9");
        assert_eq!(note["encryptedMessage"]["v"], 1);
    }
}
