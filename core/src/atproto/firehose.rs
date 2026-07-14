//! Bluesky relay firehose (`com.atproto.sync.subscribeRepos`) wire decode.
//!
//! A firehose message is two concatenated DAG-CBOR values: a small header
//! `{op, t}` naming the event kind, followed by a payload map whose shape
//! depends on `t`. This module decodes that envelope and, for `#commit`
//! events, the payload's typed fields -- turning `ops` into
//! [`super::sync::RepoOperation`]s and `blocks` into a decoded
//! [`super::car::CarFile`] -- so a caller can hand the result straight to
//! [`super::mst::extract_commit_changes`].
//!
//! Verified against real frames captured from `bsky.network`'s public relay
//! (see `testdata/`, redacted: DIDs and record content replaced with made-up
//! substitute values, but the CBOR/CID/CAR framing left exactly as shaped as
//! what the relay actually sent) rather than the written spec alone, since a
//! hand-constructed test frame can silently diverge from real wire-format
//! subtleties (tag-42 CID encoding, `blocks` embedded as a raw CBOR byte
//! string) that only a real capture would reveal.

use cid::Cid;
use ipld_core::ipld::Ipld;

use crate::error::{CoreError, CoreResult};

use super::car::{decode_car, CarFile};
use super::sync::RepoOperation;

/// A decoded `#commit` event: everything [`super::mst::extract_commit_changes`]
/// needs, plus the metadata a consumer needs for cursor tracking and DID
/// filtering.
#[derive(Debug, Clone)]
pub struct CommitFrame {
    pub seq: u64,
    pub repo_did: String,
    pub rev: String,
    pub commit_cid: Cid,
    pub ops: Vec<RepoOperation>,
    pub car: CarFile,
}

/// A decoded firehose message. Only `#commit` is fully decoded; other event
/// kinds (`#identity`, `#account`, `#sync`, `#info`, `#tombstone`, or any
/// future kind) are recognized but not otherwise interpreted, since a
/// personal-AppView indexer that only cares about posts/likes/follows/replies
/// has no use for them beyond not erroring when one arrives.
#[derive(Debug, Clone)]
pub enum FirehoseEvent {
    Commit(CommitFrame),
    Other(String),
}

/// Decode one binary WebSocket message from `subscribeRepos` into a
/// [`FirehoseEvent`]. `bytes` is the raw message payload as received off the
/// wire -- no framing beyond the two concatenated CBOR values it already
/// contains.
pub fn decode_frame(bytes: &[u8]) -> CoreResult<FirehoseEvent> {
    let mut reader = std::io::Cursor::new(bytes);
    let header: Ipld = serde_ipld_dagcbor::de::from_reader_once(&mut reader).map_err(|error| {
        CoreError::InvalidAtProto(format!("firehose frame header is not DAG-CBOR: {error}"))
    })?;
    let Ipld::Map(header_fields) = header else {
        return Err(CoreError::InvalidAtProto(
            "firehose frame header must be a CBOR map".to_string(),
        ));
    };
    let kind = match header_fields.get("t") {
        Some(Ipld::String(kind)) => kind.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "firehose frame header is missing its event kind".to_string(),
            ))
        }
    };

    let payload: Ipld = serde_ipld_dagcbor::de::from_reader_once(&mut reader).map_err(|error| {
        CoreError::InvalidAtProto(format!("firehose frame payload is not DAG-CBOR: {error}"))
    })?;
    let consumed = reader.position() as usize;
    if consumed != bytes.len() {
        return Err(CoreError::InvalidAtProto(
            "firehose frame has trailing bytes after its two CBOR values".to_string(),
        ));
    }

    if kind != "#commit" {
        return Ok(FirehoseEvent::Other(kind));
    }

    let Ipld::Map(fields) = payload else {
        return Err(CoreError::InvalidAtProto(
            "#commit payload must be a CBOR map".to_string(),
        ));
    };

    let seq = match fields.get("seq") {
        Some(Ipld::Integer(value)) if *value >= 0 => *value as u64,
        _ => {
            return Err(CoreError::InvalidAtProto(
                "#commit payload is missing seq".to_string(),
            ))
        }
    };
    let repo_did = match fields.get("repo") {
        Some(Ipld::String(did)) => did.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "#commit payload is missing repo".to_string(),
            ))
        }
    };
    let rev = match fields.get("rev") {
        Some(Ipld::String(rev)) => rev.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "#commit payload is missing rev".to_string(),
            ))
        }
    };
    let commit_cid = match fields.get("commit") {
        Some(Ipld::Link(cid)) => *cid,
        _ => {
            return Err(CoreError::InvalidAtProto(
                "#commit payload is missing its commit link".to_string(),
            ))
        }
    };
    let blocks = match fields.get("blocks") {
        Some(Ipld::Bytes(bytes)) => bytes.as_slice(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "#commit payload is missing blocks".to_string(),
            ))
        }
    };
    let ops = match fields.get("ops") {
        Some(Ipld::List(items)) => items
            .iter()
            .map(decode_op)
            .collect::<CoreResult<Vec<_>>>()?,
        _ => {
            return Err(CoreError::InvalidAtProto(
                "#commit payload is missing ops".to_string(),
            ))
        }
    };

    let car = decode_car(blocks)?;

    Ok(FirehoseEvent::Commit(CommitFrame {
        seq,
        repo_did,
        rev,
        commit_cid,
        ops,
        car,
    }))
}

/// Convert a decoded record's raw DAG-CBOR bytes (as returned in
/// [`super::mst::RepoChange::Created`]/`Updated`'s `record_bytes`) into a
/// [`serde_json::Value`] so a caller can store it in D1 or hand it to
/// existing JSON-shaped code. `repo::record_value_to_ipld` only goes the
/// other direction (dais's own outbound JSON records, restricted to a fixed
/// `app.bsky.*` allowlist) -- this accepts any third-party record shape.
pub fn record_bytes_to_json(bytes: &[u8]) -> CoreResult<serde_json::Value> {
    let ipld: Ipld = serde_ipld_dagcbor::from_slice(bytes)
        .map_err(|error| CoreError::InvalidAtProto(format!("record is not DAG-CBOR: {error}")))?;
    ipld_to_json(ipld)
}

fn ipld_to_json(ipld: Ipld) -> CoreResult<serde_json::Value> {
    Ok(match ipld {
        Ipld::Null => serde_json::Value::Null,
        Ipld::Bool(value) => serde_json::Value::Bool(value),
        Ipld::Integer(value) => {
            serde_json::Value::Number(i64::try_from(value).map(serde_json::Number::from).map_err(
                |_| {
                    CoreError::InvalidAtProto(format!("record integer {value} is out of i64 range"))
                },
            )?)
        }
        Ipld::Float(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| CoreError::InvalidAtProto("record float is not finite".to_string()))?,
        Ipld::String(value) => serde_json::Value::String(value),
        Ipld::Bytes(bytes) => {
            use base64::Engine;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        Ipld::List(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(ipld_to_json)
                .collect::<CoreResult<Vec<_>>>()?,
        ),
        Ipld::Map(fields) => serde_json::Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| Ok((key, ipld_to_json(value)?)))
                .collect::<CoreResult<serde_json::Map<_, _>>>()?,
        ),
        Ipld::Link(cid) => serde_json::Value::String(cid.to_string()),
    })
}

fn decode_op(op: &Ipld) -> CoreResult<RepoOperation> {
    let Ipld::Map(fields) = op else {
        return Err(CoreError::InvalidAtProto(
            "commit op must be a CBOR map".to_string(),
        ));
    };
    let action = match fields.get("action") {
        Some(Ipld::String(action)) => action.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "commit op is missing action".to_string(),
            ))
        }
    };
    let path = match fields.get("path") {
        Some(Ipld::String(path)) => path.clone(),
        _ => {
            return Err(CoreError::InvalidAtProto(
                "commit op is missing path".to_string(),
            ))
        }
    };
    // Deletes carry no `cid` on the wire (only `prev`, the pre-delete value's
    // CID, which this consumer doesn't need -- recording that the path is
    // now gone doesn't require knowing what used to be there).
    let cid = match fields.get("cid") {
        Some(Ipld::Link(cid)) => Some(cid.to_string()),
        _ => None,
    };
    Ok(RepoOperation { action, path, cid })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_CREATE_FRAME: &[u8] = include_bytes!("testdata/firehose_commit_create.bin");
    const REAL_DELETE_FRAME: &[u8] = include_bytes!("testdata/firehose_commit_delete.bin");

    #[test]
    fn decodes_a_real_captured_create_commit_frame() {
        let event = decode_frame(REAL_CREATE_FRAME).expect("decode real create frame");
        let FirehoseEvent::Commit(commit) = event else {
            panic!("expected a #commit event");
        };
        assert_eq!(commit.repo_did, "did:plc:aaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(commit.ops.len(), 1);
        assert_eq!(commit.ops[0].action, "create");
        assert_eq!(commit.ops[0].path, "app.bsky.graph.follow/3mql4hp6pqd2v");
        assert!(commit.ops[0].cid.is_some());

        let changes = crate::atproto::mst::extract_commit_changes(
            &commit.car,
            commit.commit_cid,
            &commit.ops,
        )
        .expect("extract changes from real frame");
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            crate::atproto::mst::RepoChange::Created { record_bytes, .. } => {
                let record: Ipld =
                    serde_ipld_dagcbor::from_slice(record_bytes).expect("record is DAG-CBOR");
                let Ipld::Map(fields) = record else {
                    panic!("record must be a map");
                };
                assert_eq!(
                    fields.get("subject"),
                    Some(&Ipld::String(
                        "did:plc:aaaaaaaaaaaaaaaaaaaaaaaa".to_string()
                    ))
                );
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn decodes_a_real_captured_delete_commit_frame() {
        let event = decode_frame(REAL_DELETE_FRAME).expect("decode real delete frame");
        let FirehoseEvent::Commit(commit) = event else {
            panic!("expected a #commit event");
        };
        assert_eq!(commit.ops.len(), 1);
        assert_eq!(commit.ops[0].action, "delete");
        assert_eq!(commit.ops[0].path, "app.bsky.feed.like/3lqiqs43fsm2e");
        assert_eq!(commit.ops[0].cid, None);

        let changes = crate::atproto::mst::extract_commit_changes(
            &commit.car,
            commit.commit_cid,
            &commit.ops,
        )
        .expect("extract changes from real frame");
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0],
            crate::atproto::mst::RepoChange::Deleted {
                path: "app.bsky.feed.like/3lqiqs43fsm2e".to_string()
            }
        );
    }

    #[test]
    fn record_bytes_to_json_converts_a_real_redacted_follow_record() {
        let event = decode_frame(REAL_CREATE_FRAME).expect("decode real create frame");
        let FirehoseEvent::Commit(commit) = event else {
            panic!("expected a #commit event");
        };
        let changes = crate::atproto::mst::extract_commit_changes(
            &commit.car,
            commit.commit_cid,
            &commit.ops,
        )
        .expect("extract changes from real frame");
        let crate::atproto::mst::RepoChange::Created { record_bytes, .. } = &changes[0] else {
            panic!("expected Created");
        };

        let json = record_bytes_to_json(record_bytes).expect("record converts to json");
        assert_eq!(
            json.get("$type").and_then(|v| v.as_str()),
            Some("app.bsky.graph.follow")
        );
        assert_eq!(
            json.get("subject").and_then(|v| v.as_str()),
            Some("did:plc:aaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn rejects_a_frame_with_trailing_bytes() {
        let mut bytes = REAL_CREATE_FRAME.to_vec();
        bytes.push(0);
        let error = decode_frame(&bytes).expect_err("trailing byte must be rejected");
        assert!(error.to_string().contains("trailing bytes"));
    }

    #[test]
    fn recognizes_non_commit_event_kinds_without_erroring() {
        use std::collections::BTreeMap;
        let header = Ipld::Map(BTreeMap::from([
            ("op".to_string(), Ipld::Integer(1)),
            ("t".to_string(), Ipld::String("#info".to_string())),
        ]));
        let payload = Ipld::Map(BTreeMap::from([(
            "message".to_string(),
            Ipld::String("hello".to_string()),
        )]));
        let mut bytes = serde_ipld_dagcbor::to_vec(&header).unwrap();
        bytes.extend(serde_ipld_dagcbor::to_vec(&payload).unwrap());

        let event = decode_frame(&bytes).expect("decode #info frame");
        match event {
            FirehoseEvent::Other(kind) => assert_eq!(kind, "#info"),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_commit_payload_missing_required_fields() {
        use std::collections::BTreeMap;
        let header = Ipld::Map(BTreeMap::from([
            ("op".to_string(), Ipld::Integer(1)),
            ("t".to_string(), Ipld::String("#commit".to_string())),
        ]));
        let payload = Ipld::Map(BTreeMap::from([("seq".to_string(), Ipld::Integer(1))]));
        let mut bytes = serde_ipld_dagcbor::to_vec(&header).unwrap();
        bytes.extend(serde_ipld_dagcbor::to_vec(&payload).unwrap());

        let error = decode_frame(&bytes).expect_err("missing fields must be rejected");
        assert!(error.to_string().contains("missing repo"));
    }
}
