//! Merkle Search Tree (MST) walk over a decoded CAR block map.
//!
//! A firehose `#commit` frame ships an authoritative `ops` list
//! (`{action, path, cid}`, see [`super::sync::RepoOperation`]) plus a CAR file
//! that contains only the *changed* blocks: the commit block, the MST nodes
//! on the path to each changed key, and the changed record blocks — not the
//! full repo tree. This module does not diff two full trees (we never have
//! both, and the frame already tells us what changed); instead it performs a
//! targeted key lookup per op, starting from the commit's `data` root, to
//! confirm the op's claimed CID is really what the tree contains and to pull
//! the record bytes for create/update ops. A lookup only dereferences the
//! links it walks past, so subtrees the frame didn't ship (because they're
//! untouched) are never touched — they'd only become an error if an op's key
//! actually required descending into one, which a well-formed diff frame
//! never does.
//!
//! The node encoding matches `repo::mst_subtree`'s writer: `{l, e}` where `l`
//! is the left subtree link (or absent/null) and `e` is a list of entries
//! `{p, k, v, t}` — shared-prefix length with the previous key, the key's
//! remaining suffix bytes, the value CID, and an optional right subtree link.

use cid::Cid;
use ipld_core::ipld::Ipld;

use crate::error::{CoreError, CoreResult};

use super::car::CarFile;
use super::sync::RepoOperation;

/// The fields of a signed repo commit block relevant to walking its tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitData {
    pub did: String,
    pub rev: String,
    pub prev: Option<Cid>,
    pub data: Cid,
}

/// A record change extracted from a commit's `ops`, verified against the
/// commit's actual MST and (for create/update) carrying the record's raw
/// DAG-CBOR bytes so a caller can decode it into whatever shape it needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepoChange {
    Created {
        path: String,
        cid: Cid,
        record_bytes: Vec<u8>,
    },
    Updated {
        path: String,
        cid: Cid,
        record_bytes: Vec<u8>,
    },
    Deleted {
        path: String,
    },
}

impl RepoChange {
    pub fn path(&self) -> &str {
        match self {
            RepoChange::Created { path, .. }
            | RepoChange::Updated { path, .. }
            | RepoChange::Deleted { path } => path,
        }
    }
}

pub fn decode_commit(car: &CarFile, commit_cid: Cid) -> CoreResult<CommitData> {
    let bytes = car.block(&commit_cid).ok_or_else(|| {
        CoreError::InvalidAtProto(format!("commit block {commit_cid} not present in CAR"))
    })?;
    let commit: Ipld = serde_ipld_dagcbor::from_slice(bytes).map_err(|error| {
        CoreError::InvalidAtProto(format!("commit block is not DAG-CBOR: {error}"))
    })?;
    let Ipld::Map(fields) = commit else {
        return Err(CoreError::InvalidAtProto(
            "commit block must be a CBOR map".to_string(),
        ));
    };
    let did = match fields.get("did") {
        Some(Ipld::String(did)) => did.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "commit block is missing did".to_string(),
            ))
        }
    };
    let rev = match fields.get("rev") {
        Some(Ipld::String(rev)) => rev.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "commit block is missing rev".to_string(),
            ))
        }
    };
    let prev = match fields.get("prev") {
        Some(Ipld::Link(cid)) => Some(*cid),
        _ => None,
    };
    let data = match fields.get("data") {
        Some(Ipld::Link(cid)) => *cid,
        _ => {
            return Err(CoreError::InvalidAtProto(
                "commit block is missing data root".to_string(),
            ))
        }
    };
    Ok(CommitData {
        did,
        rev,
        prev,
        data,
    })
}

/// Look up `target_key` in the MST rooted at `root`, returning the value CID
/// if present. Returns `Ok(None)` if the key genuinely isn't in the tree, and
/// an error only if the search needs to descend into a subtree whose block
/// isn't in `car` — i.e. the frame didn't ship a node this lookup actually
/// required.
pub fn mst_get(car: &CarFile, root: Cid, target_key: &[u8]) -> CoreResult<Option<Cid>> {
    let node = decode_mst_node(car, root)?;

    let mut previous_key: Vec<u8> = Vec::new();
    let mut gap_link = node.left;
    for entry in &node.entries {
        let full_key = apply_prefix(&previous_key, entry.prefix_len, &entry.key_suffix)?;
        match target_key.cmp(full_key.as_slice()) {
            std::cmp::Ordering::Equal => return Ok(Some(entry.value)),
            std::cmp::Ordering::Less => {
                return match gap_link {
                    Some(child) => mst_get(car, child, target_key),
                    None => Ok(None),
                };
            }
            std::cmp::Ordering::Greater => {
                previous_key = full_key;
                gap_link = entry.right;
            }
        }
    }

    match gap_link {
        Some(child) => mst_get(car, child, target_key),
        None => Ok(None),
    }
}

/// Verify and extract the record changes for a commit's `ops` against its
/// actual MST, as shipped in `car`. See the module docs for why this is a
/// per-op targeted lookup rather than a two-tree diff.
pub fn extract_commit_changes(
    car: &CarFile,
    commit_cid: Cid,
    ops: &[RepoOperation],
) -> CoreResult<Vec<RepoChange>> {
    let commit = decode_commit(car, commit_cid)?;
    ops.iter()
        .map(|op| extract_one_change(car, commit.data, op))
        .collect()
}

fn extract_one_change(car: &CarFile, data_root: Cid, op: &RepoOperation) -> CoreResult<RepoChange> {
    if op.action == "delete" {
        return Ok(RepoChange::Deleted {
            path: op.path.clone(),
        });
    }
    if op.action != "create" && op.action != "update" {
        return Err(CoreError::InvalidAtProto(format!(
            "unsupported repo op action: {}",
            op.action
        )));
    }

    let found = mst_get(car, data_root, op.path.as_bytes())?.ok_or_else(|| {
        CoreError::InvalidAtProto(format!("commit op for '{}' not found in its MST", op.path))
    })?;
    if let Some(declared) = &op.cid {
        let declared_cid: Cid = declared.parse().map_err(|error| {
            CoreError::InvalidAtProto(format!("invalid op cid '{declared}': {error}"))
        })?;
        if declared_cid != found {
            return Err(CoreError::InvalidAtProto(format!(
                "commit op cid mismatch for '{}': op declared {declared_cid}, MST has {found}",
                op.path
            )));
        }
    }
    let record_bytes = car
        .block(&found)
        .ok_or_else(|| {
            CoreError::InvalidAtProto(format!(
                "record block {found} for '{}' not present in CAR",
                op.path
            ))
        })?
        .to_vec();

    Ok(if op.action == "create" {
        RepoChange::Created {
            path: op.path.clone(),
            cid: found,
            record_bytes,
        }
    } else {
        RepoChange::Updated {
            path: op.path.clone(),
            cid: found,
            record_bytes,
        }
    })
}

struct MstEntry {
    prefix_len: usize,
    key_suffix: Vec<u8>,
    value: Cid,
    right: Option<Cid>,
}

struct MstNode {
    left: Option<Cid>,
    entries: Vec<MstEntry>,
}

fn decode_mst_node(car: &CarFile, cid: Cid) -> CoreResult<MstNode> {
    let bytes = car
        .block(&cid)
        .ok_or_else(|| CoreError::InvalidAtProto(format!("MST node {cid} not present in CAR")))?;
    let node: Ipld = serde_ipld_dagcbor::from_slice(bytes)
        .map_err(|error| CoreError::InvalidAtProto(format!("MST node is not DAG-CBOR: {error}")))?;
    let Ipld::Map(fields) = node else {
        return Err(CoreError::InvalidAtProto(
            "MST node must be a CBOR map".to_string(),
        ));
    };
    let left = match fields.get("l") {
        Some(Ipld::Link(cid)) => Some(*cid),
        _ => None,
    };
    let Some(Ipld::List(entries)) = fields.get("e") else {
        return Err(CoreError::InvalidAtProto(
            "MST node is missing entries".to_string(),
        ));
    };
    let entries = entries
        .iter()
        .map(decode_mst_entry)
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(MstNode { left, entries })
}

fn decode_mst_entry(entry: &Ipld) -> CoreResult<MstEntry> {
    let Ipld::Map(fields) = entry else {
        return Err(CoreError::InvalidAtProto(
            "MST entry must be a CBOR map".to_string(),
        ));
    };
    let prefix_len = match fields.get("p") {
        Some(Ipld::Integer(value)) if *value >= 0 => *value as usize,
        _ => {
            return Err(CoreError::InvalidAtProto(
                "MST entry has an invalid prefix length".to_string(),
            ))
        }
    };
    let key_suffix = match fields.get("k") {
        Some(Ipld::Bytes(bytes)) => bytes.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "MST entry is missing its key suffix".to_string(),
            ))
        }
    };
    let value = match fields.get("v") {
        Some(Ipld::Link(cid)) => *cid,
        _ => {
            return Err(CoreError::InvalidAtProto(
                "MST entry is missing its value link".to_string(),
            ))
        }
    };
    let right = match fields.get("t") {
        Some(Ipld::Link(cid)) => Some(*cid),
        _ => None,
    };
    Ok(MstEntry {
        prefix_len,
        key_suffix,
        value,
        right,
    })
}

fn apply_prefix(previous_key: &[u8], prefix_len: usize, suffix: &[u8]) -> CoreResult<Vec<u8>> {
    if prefix_len > previous_key.len() {
        return Err(CoreError::InvalidAtProto(
            "MST entry prefix length exceeds the previous key".to_string(),
        ));
    }
    let mut key = previous_key[..prefix_len].to_vec();
    key.extend_from_slice(suffix);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atproto::car::decode_car;
    use crate::atproto::repo::{
        encode_car, mst_subtree, repo_key_depth, repo_record_block, repo_snapshot_from_records,
        AtprotoIdentity, CarBlock, RepoRecord,
    };

    fn signed_commit(did: &str, rev: &str, prev: Option<Cid>, data: Cid) -> (Cid, Vec<u8>) {
        use ipld_core::ipld::Ipld;
        use std::collections::BTreeMap;

        let mut fields = BTreeMap::new();
        fields.insert("did".to_string(), Ipld::String(did.to_string()));
        fields.insert("version".to_string(), Ipld::Integer(3.into()));
        fields.insert("rev".to_string(), Ipld::String(rev.to_string()));
        fields.insert(
            "prev".to_string(),
            prev.map(Ipld::Link).unwrap_or(Ipld::Null),
        );
        fields.insert("data".to_string(), Ipld::Link(data));
        let bytes = serde_ipld_dagcbor::to_vec(&Ipld::Map(fields)).expect("encode commit");
        let cid = Cid::new_v1(0x71, {
            use multihash_codetable::{Code, MultihashDigest};
            Code::Sha2_256.digest(&bytes)
        });
        (cid, bytes)
    }

    fn full_repo_car(records: &[(&str, serde_json::Value)]) -> (Cid, Cid, CarFile) {
        let identity = AtprotoIdentity::new("did:plc:example", "example.social", "pds.example");
        let snapshot = repo_snapshot_from_records(
            &identity,
            "3lzzz",
            "owner-token",
            records
                .iter()
                .map(|(path, value)| RepoRecord {
                    path: path.to_string(),
                    value: value.clone(),
                })
                .collect(),
        )
        .expect("snapshot");
        let commit_cid: Cid = snapshot.commit_cid.parse().expect("commit cid");
        let car = decode_car(&snapshot.car_bytes).expect("decode car");
        let data_root = decode_commit(&car, commit_cid).expect("decode commit").data;
        (commit_cid, data_root, car)
    }

    #[test]
    fn mst_get_finds_every_record_written_into_the_tree() {
        let (_, data_root, car) = full_repo_car(&[
            (
                "app.bsky.feed.post/aaa",
                serde_json::json!({"$type": "app.bsky.feed.post", "text": "one", "createdAt": "2026-07-04T12:00:00.000Z"}),
            ),
            (
                "app.bsky.feed.post/bbb",
                serde_json::json!({"$type": "app.bsky.feed.post", "text": "two", "createdAt": "2026-07-04T12:00:01.000Z"}),
            ),
            (
                "app.bsky.graph.follow/ccc",
                serde_json::json!({"$type": "app.bsky.graph.follow", "subject": "did:plc:other", "createdAt": "2026-07-04T12:00:02.000Z"}),
            ),
        ]);

        let aaa = mst_get(&car, data_root, b"app.bsky.feed.post/aaa")
            .expect("lookup aaa")
            .expect("aaa present");
        let bbb = mst_get(&car, data_root, b"app.bsky.feed.post/bbb")
            .expect("lookup bbb")
            .expect("bbb present");
        let ccc = mst_get(&car, data_root, b"app.bsky.graph.follow/ccc")
            .expect("lookup ccc")
            .expect("ccc present");
        assert_ne!(aaa, bbb);
        assert_ne!(bbb, ccc);

        let missing = mst_get(&car, data_root, b"app.bsky.feed.post/zzz").expect("lookup missing");
        assert_eq!(missing, None);
    }

    /// `mst_get_finds_every_record_written_into_the_tree` above proves lookup
    /// correctness but not the module's central claim: that a lookup only
    /// needs the nodes on its own path, so blocks for untouched subtrees
    /// (which a real diff frame never ships) can be absent without error. A
    /// tree small enough to hand-write is usually a single root node with no
    /// child links at all, which would make that claim untested by
    /// vacuously succeeding. This uses enough records to force real
    /// branching, records which node CIDs a given lookup actually visits,
    /// and checks both directions: a lookup restricted to only the visited
    /// nodes still succeeds, and a lookup for a key that needed a node
    /// outside that set fails with a "not present" error rather than a
    /// wrong answer.
    #[test]
    fn mst_get_tolerates_missing_untouched_subtrees_but_errors_when_a_lookup_needs_them() {
        let paths: Vec<String> = (0..200)
            .map(|index| format!("app.bsky.feed.post/{index:04}"))
            .collect();
        let records: Vec<(&str, serde_json::Value)> = paths
            .iter()
            .map(|path| {
                (
                    path.as_str(),
                    serde_json::json!({"$type": "app.bsky.feed.post", "text": "x", "createdAt": "2026-07-04T12:00:00.000Z"}),
                )
            })
            .collect();
        let (_, data_root, car) = full_repo_car(&records);

        let min_depth = paths
            .iter()
            .map(|path| repo_key_depth(path.as_bytes()))
            .min()
            .expect("min depth");
        let target_path = paths
            .iter()
            .find(|path| repo_key_depth(path.as_bytes()) > min_depth)
            .expect("at least one record should be nested below the root level");

        let (target_cid, target_visited) = collect_visited(&car, data_root, target_path.as_bytes())
            .expect("collect visited for target");
        let target_cid = target_cid.expect("target present");
        assert!(
            target_visited.len() > 1,
            "target lookup should recurse through at least one child link"
        );

        let victim_path = paths
            .iter()
            .find(|path| {
                *path != target_path
                    && match collect_visited(&car, data_root, path.as_bytes()) {
                        Ok((_, visited)) => !visited.iter().all(|cid| target_visited.contains(cid)),
                        Err(_) => false,
                    }
            })
            .expect("at least one other record should need a node outside target's path");

        let filtered = CarFile {
            roots: car.roots.clone(),
            blocks: car
                .blocks
                .iter()
                .filter(|(cid, _)| target_visited.contains(cid))
                .map(|(cid, bytes)| (*cid, bytes.clone()))
                .collect(),
        };

        let found = mst_get(&filtered, data_root, target_path.as_bytes())
            .expect("target still resolves without blocks for untouched subtrees");
        assert_eq!(found, Some(target_cid));

        let error = mst_get(&filtered, data_root, victim_path.as_bytes())
            .expect_err("victim lookup needed a node the filtered frame doesn't have");
        assert!(error.to_string().contains("not present in CAR"));
    }

    /// Mirrors `mst_get`'s traversal exactly, but records every node CID it
    /// dereferences instead of just the final answer, so a test can compute
    /// which blocks a given lookup actually needs.
    fn collect_visited(
        car: &CarFile,
        root: Cid,
        target_key: &[u8],
    ) -> CoreResult<(Option<Cid>, Vec<Cid>)> {
        let mut visited = Vec::new();
        let found = collect_visited_inner(car, root, target_key, &mut visited)?;
        Ok((found, visited))
    }

    fn collect_visited_inner(
        car: &CarFile,
        root: Cid,
        target_key: &[u8],
        visited: &mut Vec<Cid>,
    ) -> CoreResult<Option<Cid>> {
        visited.push(root);
        let node = decode_mst_node(car, root)?;

        let mut previous_key: Vec<u8> = Vec::new();
        let mut gap_link = node.left;
        for entry in &node.entries {
            let full_key = apply_prefix(&previous_key, entry.prefix_len, &entry.key_suffix)?;
            match target_key.cmp(full_key.as_slice()) {
                std::cmp::Ordering::Equal => return Ok(Some(entry.value)),
                std::cmp::Ordering::Less => {
                    return match gap_link {
                        Some(child) => collect_visited_inner(car, child, target_key, visited),
                        None => Ok(None),
                    };
                }
                std::cmp::Ordering::Greater => {
                    previous_key = full_key;
                    gap_link = entry.right;
                }
            }
        }

        match gap_link {
            Some(child) => collect_visited_inner(car, child, target_key, visited),
            None => Ok(None),
        }
    }

    #[test]
    fn extract_commit_changes_reads_create_and_delete_ops_from_a_partial_diff_frame() {
        // Simulate a real firehose frame: a full tree built by the trusted
        // encoder, then everything except the commit block, the path to the
        // changed key, and that key's own record block is dropped — the
        // untouched subtrees a real relay would never ship.
        let identity = AtprotoIdentity::new("did:plc:example", "example.social", "pds.example");
        let mut sorted = vec![
            repo_record_block(
                "app.bsky.feed.post/aaa".to_string(),
                serde_json::json!({"$type": "app.bsky.feed.post", "text": "one", "createdAt": "2026-07-04T12:00:00.000Z"}),
            )
            .expect("record block"),
            repo_record_block(
                "app.bsky.feed.post/bbb".to_string(),
                serde_json::json!({"$type": "app.bsky.feed.post", "text": "two", "createdAt": "2026-07-04T12:00:01.000Z"}),
            )
            .expect("record block"),
        ];
        sorted.sort_by(|left, right| left.path.cmp(&right.path));
        let min_depth = sorted
            .iter()
            .map(|record| repo_key_depth(record.path.as_bytes()))
            .min()
            .expect("min depth");
        let (data_root, mst_blocks) =
            mst_subtree(&sorted, 0..sorted.len(), min_depth).expect("mst");
        let (commit_cid, commit_bytes) = signed_commit(&identity.did, "3lzzz", None, data_root);

        let target = sorted
            .iter()
            .find(|record| record.path == "app.bsky.feed.post/bbb")
            .expect("bbb record");
        let changed_op = RepoOperation::create("app.bsky.feed.post/bbb", target.cid.to_string());
        let deleted_op = RepoOperation::delete("app.bsky.feed.post/does-not-exist");

        // A real diff frame ships the commit block, every MST node, and only
        // the changed record's block — not the sibling record.
        let mut diff_blocks = vec![CarBlock {
            cid: commit_cid,
            bytes: commit_bytes,
        }];
        diff_blocks.extend(mst_blocks);
        diff_blocks.push(CarBlock {
            cid: target.cid,
            bytes: target.bytes.clone(),
        });
        let car_bytes = encode_car(commit_cid, &diff_blocks).expect("encode diff car");
        let car = decode_car(&car_bytes).expect("decode diff car");

        let changes = extract_commit_changes(&car, commit_cid, &[changed_op, deleted_op])
            .expect("extract changes");

        assert_eq!(changes.len(), 2);
        match &changes[0] {
            RepoChange::Created {
                path,
                cid,
                record_bytes,
            } => {
                assert_eq!(path, "app.bsky.feed.post/bbb");
                assert_eq!(*cid, target.cid);
                assert_eq!(record_bytes, &target.bytes);
            }
            other => panic!("expected Created, got {other:?}"),
        }
        assert_eq!(
            changes[1],
            RepoChange::Deleted {
                path: "app.bsky.feed.post/does-not-exist".to_string()
            }
        );
    }

    #[test]
    fn extract_commit_changes_rejects_a_cid_mismatch_against_the_real_tree() {
        let (commit_cid, _, car) = full_repo_car(&[(
            "app.bsky.feed.post/aaa",
            serde_json::json!({"$type": "app.bsky.feed.post", "text": "one", "createdAt": "2026-07-04T12:00:00.000Z"}),
        )]);
        let wrong_cid = Cid::new_v1(0x71, {
            use multihash_codetable::{Code, MultihashDigest};
            Code::Sha2_256.digest(b"not the real record")
        });
        let op = RepoOperation::create("app.bsky.feed.post/aaa", wrong_cid.to_string());

        let error = extract_commit_changes(&car, commit_cid, &[op]).expect_err("mismatch rejected");
        assert!(error.to_string().contains("cid mismatch"));
    }

    #[test]
    fn extract_commit_changes_rejects_an_op_for_a_path_the_tree_does_not_have() {
        let (commit_cid, _, car) = full_repo_car(&[(
            "app.bsky.feed.post/aaa",
            serde_json::json!({"$type": "app.bsky.feed.post", "text": "one", "createdAt": "2026-07-04T12:00:00.000Z"}),
        )]);
        let op = RepoOperation::create("app.bsky.feed.post/does-not-exist", "bafyfake");

        let error =
            extract_commit_changes(&car, commit_cid, &[op]).expect_err("missing path rejected");
        assert!(error.to_string().contains("not found"));
    }
}
