//! AT Protocol repository response helpers.
//!
//! These are platform-agnostic pieces of the PDS compatibility surface. The
//! Cloudflare workers still own DB/R2 reads, but repo identity, status, and
//! describe/list response construction live here so router/PDS code can call
//! core behavior instead of duplicating protocol shapes.

use crate::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};

pub const SUPPORTED_COLLECTIONS: [&str; 5] = [
    "app.bsky.actor.profile",
    "app.bsky.feed.post",
    "app.bsky.feed.like",
    "app.bsky.feed.repost",
    "app.bsky.graph.follow",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AtprotoIdentity {
    pub did: String,
    pub handle: String,
    pub pds_hostname: String,
}

impl AtprotoIdentity {
    pub fn new(
        did: impl Into<String>,
        handle: impl Into<String>,
        pds_hostname: impl Into<String>,
    ) -> Self {
        Self {
            did: did.into(),
            handle: handle.into(),
            pds_hostname: pds_hostname.into(),
        }
    }

    pub fn matches_repo(&self, repo: &str) -> bool {
        repo == self.did || repo == self.handle
    }

    pub fn require_repo(&self, repo: &str) -> CoreResult<()> {
        if self.matches_repo(repo) {
            Ok(())
        } else {
            Err(CoreError::NotFound("Repo not found".to_string()))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepoSnapshot {
    pub rev: String,
    pub commit_cid: String,
    pub car_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepoStats {
    pub head: String,
    pub rev: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LatestCommitResponse {
    pub cid: String,
    pub rev: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepoStatusResponse {
    pub did: String,
    pub active: bool,
    pub status: String,
    pub rev: String,
    pub head: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListReposResponse {
    pub repos: Vec<RepoStatusResponse>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DidDocService {
    pub id: String,
    #[serde(rename = "type")]
    pub service_type: String,
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepoDidDoc {
    pub id: String,
    pub service: Vec<DidDocService>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DescribeRepoResponse {
    pub handle: String,
    pub did: String,
    #[serde(rename = "didDoc")]
    pub did_doc: RepoDidDoc,
    pub collections: Vec<String>,
    #[serde(rename = "handleIsCorrect")]
    pub handle_is_correct: bool,
}

pub fn repo_stats(snapshot: &RepoSnapshot) -> RepoStats {
    RepoStats {
        head: snapshot.commit_cid.clone(),
        rev: snapshot.rev.clone(),
    }
}

pub fn get_repo(snapshot: &RepoSnapshot) -> CoreResult<Vec<u8>> {
    if snapshot.car_bytes.is_empty() {
        return Err(CoreError::InvalidAtProto(
            "ATProto repo snapshot has no CAR bytes".to_string(),
        ));
    }
    Ok(snapshot.car_bytes.clone())
}

pub fn latest_commit(stats: &RepoStats) -> LatestCommitResponse {
    LatestCommitResponse {
        cid: stats.head.clone(),
        rev: stats.rev.clone(),
    }
}

pub fn repo_status(repo: &str, stats: &RepoStats) -> RepoStatusResponse {
    RepoStatusResponse {
        did: repo.to_string(),
        active: true,
        status: "active".to_string(),
        rev: stats.rev.clone(),
        head: stats.head.clone(),
    }
}

pub fn list_repos(identity: &AtprotoIdentity, stats: &RepoStats) -> ListReposResponse {
    ListReposResponse {
        repos: vec![repo_status(&identity.did, stats)],
    }
}

pub fn describe_repo(identity: &AtprotoIdentity) -> DescribeRepoResponse {
    DescribeRepoResponse {
        handle: identity.handle.clone(),
        did: identity.did.clone(),
        did_doc: RepoDidDoc {
            id: identity.did.clone(),
            service: vec![DidDocService {
                id: "#atproto_pds".to_string(),
                service_type: "AtprotoPersonalDataServer".to_string(),
                service_endpoint: format!("https://{}", identity.pds_hostname),
            }],
        },
        collections: SUPPORTED_COLLECTIONS
            .iter()
            .map(|collection| collection.to_string())
            .collect(),
        handle_is_correct: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> AtprotoIdentity {
        AtprotoIdentity::new("did:web:pds.example", "social.example", "pds.example")
    }

    #[test]
    fn repo_identity_accepts_did_or_handle_only() {
        let identity = identity();

        assert!(identity.matches_repo("did:web:pds.example"));
        assert!(identity.matches_repo("social.example"));
        assert!(identity.require_repo("did:web:pds.example").is_ok());
        assert!(matches!(
            identity.require_repo("other.example"),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn repo_status_and_describe_shapes_match_pds_surface() {
        let identity = identity();
        let snapshot = RepoSnapshot {
            rev: "3lxyz".to_string(),
            commit_cid: "bafycommit".to_string(),
            car_bytes: vec![1, 2, 3],
        };
        let stats = repo_stats(&snapshot);

        assert_eq!(get_repo(&snapshot).unwrap(), vec![1, 2, 3]);
        assert_eq!(
            latest_commit(&stats),
            LatestCommitResponse {
                cid: "bafycommit".to_string(),
                rev: "3lxyz".to_string(),
            }
        );
        assert_eq!(list_repos(&identity, &stats).repos[0].did, identity.did);

        let describe = describe_repo(&identity);
        assert_eq!(describe.did_doc.id, "did:web:pds.example");
        assert_eq!(
            describe.did_doc.service[0].service_endpoint,
            "https://pds.example"
        );
        assert!(describe
            .collections
            .contains(&"app.bsky.feed.post".to_string()));
        let json = serde_json::to_value(&describe).unwrap();
        assert_eq!(
            json["didDoc"]["service"][0]["serviceEndpoint"],
            "https://pds.example"
        );
        assert_eq!(json["handleIsCorrect"], true);
    }

    #[test]
    fn empty_repo_snapshot_is_rejected() {
        let snapshot = RepoSnapshot {
            rev: "3lxyz".to_string(),
            commit_cid: "bafycommit".to_string(),
            car_bytes: Vec::new(),
        };

        assert!(matches!(
            get_repo(&snapshot),
            Err(CoreError::InvalidAtProto(_))
        ));
    }
}
