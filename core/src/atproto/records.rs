//! AT Protocol record primitives.

use crate::{CoreError, CoreResult};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::repo::RepoStats;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordRef {
    pub uri: String,
    pub cid: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordResponse {
    pub uri: String,
    pub cid: String,
    pub value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitRef {
    pub cid: String,
    pub rev: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateRecordResponse {
    pub uri: String,
    pub cid: String,
    pub commit: CommitRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeleteRecordResponse {
    pub commit: CommitRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedFeedPostRecord {
    pub text: String,
    pub created_at: Option<String>,
    pub in_reply_to: Option<String>,
    pub reply_json: Option<String>,
}

pub fn stable_cid(value: &str) -> String {
    use multihash_codetable::{Code, MultihashDigest};

    cid::Cid::new_v1(0x55, Code::Sha2_256.digest(value.as_bytes())).to_string()
}

pub fn record_uri(did: &str, collection: &str, rkey: &str) -> String {
    format!("at://{did}/{collection}/{rkey}")
}

pub fn repo_path(collection: &str, rkey: &str) -> CoreResult<String> {
    let collection = collection.trim();
    let rkey = rkey.trim();
    if collection.is_empty() || rkey.is_empty() || collection.contains('/') || rkey.contains('/') {
        return Err(CoreError::InvalidAtProto(
            "ATProto collection and rkey must be non-empty path segments".to_string(),
        ));
    }
    Ok(format!("{collection}/{rkey}"))
}

pub fn repo_path_from_at_uri(uri: &str) -> CoreResult<String> {
    let rest = uri.strip_prefix("at://").ok_or_else(|| {
        CoreError::InvalidAtProto("ATProto URI must start with at://".to_string())
    })?;
    let mut parts = rest.splitn(3, '/');
    let did = parts.next().unwrap_or_default();
    let collection = parts.next().unwrap_or_default();
    let rkey = parts.next().unwrap_or_default();
    if did.is_empty() {
        return Err(CoreError::InvalidAtProto(
            "ATProto URI is missing repo DID".to_string(),
        ));
    }
    repo_path(collection, rkey)
}

pub fn record_ref(uri: &str, value: &Value) -> RecordRef {
    RecordRef {
        uri: uri.to_string(),
        cid: stable_cid(&canonical_record_seed(uri, value)),
    }
}

pub fn record_response(uri: &str, value: Value) -> RecordResponse {
    let reference = record_ref(uri, &value);
    RecordResponse {
        uri: reference.uri,
        cid: reference.cid,
        value,
    }
}

pub fn create_record_response(
    uri: &str,
    value: &Value,
    repo_stats: &RepoStats,
) -> CreateRecordResponse {
    let reference = record_ref(uri, value);
    CreateRecordResponse {
        uri: reference.uri,
        cid: reference.cid,
        commit: CommitRef {
            cid: repo_stats.head.clone(),
            rev: repo_stats.rev.clone(),
        },
    }
}

pub fn delete_record_response(repo_stats: &RepoStats) -> DeleteRecordResponse {
    DeleteRecordResponse {
        commit: CommitRef {
            cid: repo_stats.head.clone(),
            rev: repo_stats.rev.clone(),
        },
    }
}

pub fn generated_rkey(created_at: &str, seed: &str) -> String {
    format!(
        "{}-{}",
        created_at
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(14)
            .collect::<String>(),
        stable_cid(&format!("{created_at}\n{seed}"))
            .chars()
            .skip(4)
            .take(8)
            .collect::<String>()
    )
}

pub fn validate_record_type(collection: &str, record: &Value) -> CoreResult<()> {
    let record_type = required_string_field(record, "$type")?;
    if record_type != collection {
        return Err(CoreError::InvalidAtProto(
            "record $type must match collection".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_record_key(collection: &str, rkey: &str) -> CoreResult<()> {
    repo_path(collection, rkey).map(|_| ())
}

pub fn validate_feed_post_record(record: &Value) -> CoreResult<ValidatedFeedPostRecord> {
    validate_record_type("app.bsky.feed.post", record)?;
    validate_public_visibility(record)?;

    let text = required_string_field(record, "text")?.trim().to_string();
    if text.is_empty() {
        return Err(CoreError::InvalidAtProto(
            "post text is required".to_string(),
        ));
    }

    let created_at = optional_string_field(record, "createdAt")?
        .map(|value| {
            if value.trim().is_empty() {
                return Err(CoreError::InvalidAtProto(
                    "createdAt must be a non-empty RFC3339 timestamp".to_string(),
                ));
            }
            DateTime::parse_from_rfc3339(value).map_err(|_| {
                CoreError::InvalidAtProto("createdAt must be a valid RFC3339 timestamp".to_string())
            })?;
            Ok(value.to_string())
        })
        .transpose()?;

    let (in_reply_to, reply_json) = validate_reply_ref(record)?;

    Ok(ValidatedFeedPostRecord {
        text,
        created_at,
        in_reply_to,
        reply_json,
    })
}

fn canonical_record_seed(uri: &str, value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    format!("{uri}\n{serialized}")
}

fn validate_public_visibility(record: &Value) -> CoreResult<()> {
    for field in ["visibility", "daisVisibility", "access"] {
        let Some(value) = record.get(field) else {
            continue;
        };
        let Some(value) = value.as_str() else {
            return Err(CoreError::InvalidAtProto(format!(
                "{field} must be the string 'public' for ATProto createRecord"
            )));
        };
        if value != "public" {
            return Err(private_visibility_error());
        }
    }

    if let Some(value) = record.get("private") {
        match value.as_bool() {
            Some(false) => {}
            Some(true) => return Err(private_visibility_error()),
            None => return Err(private_visibility_error()),
        }
    }

    if record.get("audience").is_some() {
        return Err(private_visibility_error());
    }

    Ok(())
}

fn private_visibility_error() -> CoreError {
    CoreError::InvalidAtProto(
        "ATProto createRecord in dais PDS compatibility mode only supports public posts"
            .to_string(),
    )
}

fn validate_reply_ref(record: &Value) -> CoreResult<(Option<String>, Option<String>)> {
    let Some(reply) = record.get("reply") else {
        return Ok((None, None));
    };
    let Some(reply) = reply.as_object() else {
        return Err(CoreError::InvalidAtProto(
            "reply must be an object".to_string(),
        ));
    };
    let root = reply
        .get("root")
        .ok_or_else(|| CoreError::InvalidAtProto("reply.root is required".to_string()))?;
    let parent = reply
        .get("parent")
        .ok_or_else(|| CoreError::InvalidAtProto("reply.parent is required".to_string()))?;
    validate_strong_ref("reply.root", root)?;
    let parent_uri = validate_strong_ref("reply.parent", parent)?;
    let reply_json = serde_json::to_string(reply)
        .map_err(|error| CoreError::Serialization(error.to_string()))?;
    Ok((Some(parent_uri), Some(reply_json)))
}

fn validate_strong_ref(name: &str, value: &Value) -> CoreResult<String> {
    let Some(value) = value.as_object() else {
        return Err(CoreError::InvalidAtProto(format!(
            "{name} must be an object"
        )));
    };
    let uri = value
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::InvalidAtProto(format!("{name}.uri is required")))?;
    if !uri.starts_with("at://") || !uri.contains("/app.bsky.feed.post/") {
        return Err(CoreError::InvalidAtProto(format!(
            "{name}.uri must reference an ATProto feed post"
        )));
    }
    let cid = value
        .get("cid")
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::InvalidAtProto(format!("{name}.cid is required")))?;
    if cid.trim().is_empty() {
        return Err(CoreError::InvalidAtProto(format!(
            "{name}.cid must be non-empty"
        )));
    }
    Ok(uri.to_string())
}

fn required_string_field<'a>(record: &'a Value, field: &str) -> CoreResult<&'a str> {
    record
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::InvalidAtProto(format!("{field} must be a string on the record")))
}

fn optional_string_field<'a>(record: &'a Value, field: &str) -> CoreResult<Option<&'a str>> {
    match record.get(field) {
        Some(Value::String(value)) => Ok(Some(value.as_str())),
        Some(_) => Err(CoreError::InvalidAtProto(format!(
            "{field} must be a string when present"
        ))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_cid_is_cidv1_and_changes_with_input() {
        let first = stable_cid("hello");
        let second = stable_cid("goodbye");

        assert!(first.starts_with("baf"));
        assert_ne!(first, second);
    }

    #[test]
    fn record_uri_and_repo_path_round_trip() {
        let uri = record_uri("did:web:pds.example", "app.bsky.feed.post", "abc123");

        assert_eq!(uri, "at://did:web:pds.example/app.bsky.feed.post/abc123");
        assert_eq!(
            repo_path_from_at_uri(&uri).unwrap(),
            "app.bsky.feed.post/abc123"
        );
        assert!(repo_path_from_at_uri("https://example.com").is_err());
        assert!(repo_path("app.bsky.feed.post", "").is_err());
    }

    #[test]
    fn create_and_delete_record_responses_carry_commit_ref() {
        let stats = RepoStats {
            head: "bafycommit".to_string(),
            rev: "3lxyz".to_string(),
        };
        let uri = record_uri("did:web:pds.example", "app.bsky.feed.post", "abc123");
        let value = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "hello"
        });

        let created = create_record_response(&uri, &value, &stats);
        assert_eq!(created.uri, uri);
        assert!(created.cid.starts_with("baf"));
        assert_eq!(created.commit.cid, "bafycommit");
        assert_eq!(created.commit.rev, "3lxyz");

        let deleted = delete_record_response(&stats);
        assert_eq!(deleted.commit, created.commit);
    }

    #[test]
    fn generated_rkey_uses_timestamp_and_stable_suffix() {
        let first = generated_rkey("2026-07-04T12:34:56Z", "hello");
        let second = generated_rkey("2026-07-04T12:34:56Z", "hello");

        assert_eq!(first, second);
        assert!(first.starts_with("20260704123456-"));
    }

    #[test]
    fn validates_public_feed_post_records() {
        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "  hello atproto  ",
            "createdAt": "2026-07-04T12:34:56Z",
            "visibility": "public"
        });

        let validated = validate_feed_post_record(&record).unwrap();

        assert_eq!(validated.text, "hello atproto");
        assert_eq!(
            validated.created_at.as_deref(),
            Some("2026-07-04T12:34:56Z")
        );
        assert_eq!(validated.in_reply_to, None);
        assert_eq!(validated.reply_json, None);
    }

    #[test]
    fn rejects_non_public_feed_post_records() {
        for record in [
            serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": "friends only",
                "visibility": "followers"
            }),
            serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": "direct",
                "private": true
            }),
            serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": "direct as string",
                "private": "true"
            }),
            serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": "known people",
                "audience": ["did:plc:alice"]
            }),
        ] {
            let error = validate_feed_post_record(&record).unwrap_err();
            assert!(error.to_string().contains("only supports public posts"));
        }
    }

    #[test]
    fn validates_feed_post_replies() {
        let record = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "replying",
            "reply": {
                "root": {
                    "uri": "at://did:plc:alice/app.bsky.feed.post/root",
                    "cid": "bafyroot"
                },
                "parent": {
                    "uri": "at://did:plc:alice/app.bsky.feed.post/parent",
                    "cid": "bafyparent"
                }
            }
        });

        let validated = validate_feed_post_record(&record).unwrap();

        assert_eq!(
            validated.in_reply_to.as_deref(),
            Some("at://did:plc:alice/app.bsky.feed.post/parent")
        );
        assert!(validated
            .reply_json
            .as_deref()
            .unwrap()
            .contains("bafyparent"));
    }

    #[test]
    fn rejects_malformed_feed_post_replies() {
        let missing_root = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "replying",
            "reply": {
                "parent": {
                    "uri": "at://did:plc:alice/app.bsky.feed.post/parent",
                    "cid": "bafyparent"
                }
            }
        });
        let bad_parent = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "replying",
            "reply": {
                "root": {
                    "uri": "at://did:plc:alice/app.bsky.feed.post/root",
                    "cid": "bafyroot"
                },
                "parent": {
                    "uri": "https://example.com/posts/1",
                    "cid": "bafyparent"
                }
            }
        });

        assert!(validate_feed_post_record(&missing_root)
            .unwrap_err()
            .to_string()
            .contains("reply.root is required"));
        assert!(validate_feed_post_record(&bad_parent)
            .unwrap_err()
            .to_string()
            .contains("reply.parent.uri must reference"));
    }

    #[test]
    fn rejects_invalid_feed_post_shape() {
        let wrong_type = serde_json::json!({
            "$type": "app.bsky.feed.like",
            "text": "nope"
        });
        let missing_text = serde_json::json!({
            "$type": "app.bsky.feed.post"
        });
        let bad_timestamp = serde_json::json!({
            "$type": "app.bsky.feed.post",
            "text": "bad timestamp",
            "createdAt": "July 4"
        });

        assert!(validate_feed_post_record(&wrong_type)
            .unwrap_err()
            .to_string()
            .contains("record $type must match collection"));
        assert!(validate_feed_post_record(&missing_text)
            .unwrap_err()
            .to_string()
            .contains("text must be a string"));
        assert!(validate_feed_post_record(&bad_timestamp)
            .unwrap_err()
            .to_string()
            .contains("createdAt must be a valid RFC3339"));
    }

    #[test]
    fn validates_record_keys_for_create_and_delete_paths() {
        assert!(validate_record_key("app.bsky.feed.post", "abc123").is_ok());
        assert!(validate_record_key("app.bsky.feed.post", "").is_err());
        assert!(validate_record_key("app.bsky.feed.post", "bad/slash").is_err());
        assert!(validate_record_key("app/bsky/feed/post", "abc123").is_err());
    }
}
