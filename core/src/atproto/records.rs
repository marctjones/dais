//! AT Protocol record primitives.

use crate::{CoreError, CoreResult};
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

fn canonical_record_seed(uri: &str, value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    format!("{uri}\n{serialized}")
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
}
