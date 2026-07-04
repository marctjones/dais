//! AT Protocol repo sync event helpers.

use serde::{Deserialize, Serialize};

use super::repo::{AtprotoIdentity, RepoStats};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepoOperation {
    pub action: String,
    pub path: String,
    pub cid: Option<String>,
}

impl RepoOperation {
    pub fn create(path: impl Into<String>, cid: impl Into<String>) -> Self {
        Self {
            action: "create".to_string(),
            path: path.into(),
            cid: Some(cid.into()),
        }
    }

    pub fn update(path: impl Into<String>, cid: impl Into<String>) -> Self {
        Self {
            action: "update".to_string(),
            path: path.into(),
            cid: Some(cid.into()),
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self {
            action: "delete".to_string(),
            path: path.into(),
            cid: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepoCommitEvent {
    pub repo: String,
    pub rev: String,
    pub seq: u64,
    pub time: String,
    pub commit: String,
    pub ops: Vec<RepoOperation>,
    #[serde(default)]
    pub blobs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SubscribeReposRequest {
    #[serde(rename = "type")]
    pub message_type: String,
    pub pds_url: String,
    pub repo_did: String,
    pub handle: String,
    pub sequence_hint: u64,
}

pub fn commit_event(
    identity: &AtprotoIdentity,
    stats: &RepoStats,
    seq: u64,
    time: impl Into<String>,
    ops: Vec<RepoOperation>,
) -> RepoCommitEvent {
    RepoCommitEvent {
        repo: identity.did.clone(),
        rev: stats.rev.clone(),
        seq: seq.max(1),
        time: time.into(),
        commit: stats.head.clone(),
        ops,
        blobs: Vec::new(),
    }
}

pub fn subscribe_repos_request(identity: &AtprotoIdentity) -> SubscribeReposRequest {
    SubscribeReposRequest {
        message_type: "atproto.sync.subscribeRepos".to_string(),
        pds_url: format!("https://{}", identity.pds_hostname),
        repo_did: identity.did.clone(),
        handle: identity.handle.clone(),
        sequence_hint: sequence_from_stable_value(&identity.did),
    }
}

pub fn sequence_from_stable_value(value: &str) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_event_uses_identity_stats_and_nonzero_sequence() {
        let identity = AtprotoIdentity::new("did:web:pds.example", "social.example", "pds.example");
        let stats = RepoStats {
            head: "bafycommit".to_string(),
            rev: "3lxyz".to_string(),
        };
        let event = commit_event(
            &identity,
            &stats,
            0,
            "2026-07-04T12:00:00Z",
            vec![RepoOperation::create(
                "app.bsky.feed.post/abc123",
                "bafyrecord",
            )],
        );

        assert_eq!(event.repo, "did:web:pds.example");
        assert_eq!(event.rev, "3lxyz");
        assert_eq!(event.commit, "bafycommit");
        assert_eq!(event.seq, 1);
        assert_eq!(event.ops[0].action, "create");
        assert_eq!(event.ops[0].cid.as_deref(), Some("bafyrecord"));
        assert!(event.blobs.is_empty());
    }

    #[test]
    fn sequence_from_stable_value_is_stable_and_nonzero() {
        let first = sequence_from_stable_value("repo-rev");
        let second = sequence_from_stable_value("repo-rev");

        assert_eq!(first, second);
        assert!(first > 0);
    }

    #[test]
    fn subscribe_repos_request_uses_identity_and_stable_sequence() {
        let identity = AtprotoIdentity::new("did:web:pds.example", "social.example", "pds.example");
        let request = subscribe_repos_request(&identity);

        assert_eq!(request.message_type, "atproto.sync.subscribeRepos");
        assert_eq!(request.pds_url, "https://pds.example");
        assert_eq!(request.repo_did, "did:web:pds.example");
        assert_eq!(request.handle, "social.example");
        assert_eq!(
            request.sequence_hint,
            sequence_from_stable_value("did:web:pds.example")
        );
    }
}
