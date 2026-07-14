//! Independent verification of a single record against a
//! `com.atproto.sync.getRecord` response, without trusting whatever claimed
//! the record changed (e.g. a Jetstream event) at face value.
//!
//! `getRecord`'s CAR response has the same shape as a firehose `#commit`
//! frame's `blocks` (a signed commit as the CAR root, plus the MST nodes
//! needed to prove one key's presence or absence) -- this reuses
//! [`super::mst::decode_commit`] and [`super::mst::mst_get`] rather than
//! introducing a second decode path.

use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use crate::error::{CoreError, CoreResult};

use super::car::decode_car;
use super::mst::{decode_commit, mst_get};

/// The outcome of proving a record's presence or absence in a repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordProof {
    /// The record exists under the proven root, and its content hash matches
    /// its CID (recomputed independently, not trusted from the CAR label).
    Present { cid: Cid, record_bytes: Vec<u8> },
    /// The MST proves no record exists at this key under the proven root --
    /// evidence a claimed delete is real.
    Absent,
}

/// Verify a `getRecord` CAR response for `did`/`collection`/`rkey`.
///
/// Returns an error if the response is malformed, or if it's a proof for a
/// different repo than `expected_did` (a misdirected or spoofed response).
pub fn verify_record_proof(
    getrecord_car_bytes: &[u8],
    expected_did: &str,
    collection: &str,
    rkey: &str,
) -> CoreResult<RecordProof> {
    let car = decode_car(getrecord_car_bytes)?;
    let commit_cid = *car
        .roots
        .first()
        .ok_or_else(|| CoreError::InvalidAtProto("getRecord CAR has no root".to_string()))?;
    let commit = decode_commit(&car, commit_cid)?;
    if commit.did != expected_did {
        return Err(CoreError::InvalidAtProto(format!(
            "getRecord proof is for repo '{}', expected '{expected_did}'",
            commit.did
        )));
    }

    let key = format!("{collection}/{rkey}");
    match mst_get(&car, commit.data, key.as_bytes())? {
        None => Ok(RecordProof::Absent),
        Some(cid) => {
            let record_bytes = car
                .block(&cid)
                .ok_or_else(|| {
                    CoreError::InvalidAtProto(
                        "mst_get resolved a cid whose block is missing from the CAR".to_string(),
                    )
                })?
                .to_vec();
            verify_block_hash(cid, &record_bytes)?;
            Ok(RecordProof::Present { cid, record_bytes })
        }
    }
}

/// The CAR container indexes blocks by a CID label it does not itself
/// verify (see [`decode_car`]) -- for this proof-verification entry point we
/// recompute the hash ourselves so a relabeled block can't pass silently.
fn verify_block_hash(cid: Cid, bytes: &[u8]) -> CoreResult<()> {
    let recomputed = Cid::new_v1(cid.codec(), Code::Sha2_256.digest(bytes));
    if recomputed != cid {
        return Err(CoreError::InvalidAtProto(format!(
            "record block content does not hash to its claimed cid {cid} (got {recomputed})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atproto::repo::{
        encode_car, repo_snapshot_from_records, AtprotoIdentity, CarBlock, RepoRecord,
    };

    const DID: &str = "did:plc:example";

    /// Build a full repo CAR (one post record) the same way [`super::super::mst`]'s
    /// own tests do, via the real repo encoder -- not a hand-rolled fixture.
    fn getrecord_style_car() -> (Cid, Vec<u8>) {
        let identity = AtprotoIdentity::new(DID, "example.social", "pds.example");
        let snapshot = repo_snapshot_from_records(
            &identity,
            "3lzzz",
            "owner-token",
            vec![RepoRecord {
                path: "app.bsky.feed.post/aaa".to_string(),
                value: serde_json::json!({"$type": "app.bsky.feed.post", "text": "hi", "createdAt": "2026-07-04T12:00:00.000Z"}),
            }],
        )
        .expect("snapshot");
        let commit_cid: Cid = snapshot.commit_cid.parse().expect("commit cid");
        (commit_cid, snapshot.car_bytes)
    }

    #[test]
    fn proves_presence_of_an_existing_record() {
        let (_commit_cid, car_bytes) = getrecord_style_car();
        let proof =
            verify_record_proof(&car_bytes, DID, "app.bsky.feed.post", "aaa").expect("verify");
        match proof {
            RecordProof::Present { record_bytes, .. } => {
                let value: serde_json::Value =
                    serde_ipld_dagcbor::from_slice(&record_bytes).expect("decode record");
                assert_eq!(value["text"], "hi");
            }
            RecordProof::Absent => panic!("expected Present"),
        }
    }

    #[test]
    fn proves_absence_of_a_missing_record() {
        let (_commit_cid, car_bytes) = getrecord_style_car();
        let proof =
            verify_record_proof(&car_bytes, DID, "app.bsky.feed.post", "zzz").expect("verify");
        assert_eq!(proof, RecordProof::Absent);
    }

    #[test]
    fn rejects_a_proof_for_the_wrong_repo() {
        let (_commit_cid, car_bytes) = getrecord_style_car();
        let error = verify_record_proof(
            &car_bytes,
            "did:plc:someone-else",
            "app.bsky.feed.post",
            "aaa",
        )
        .expect_err("wrong repo rejected");
        assert!(error.to_string().contains("did:plc:someone-else"));
    }

    #[test]
    fn rejects_a_block_whose_content_does_not_hash_to_its_claimed_cid() {
        let (commit_cid, car_bytes) = getrecord_style_car();
        let mut car = decode_car(&car_bytes).expect("decode car");
        let commit = decode_commit(&car, commit_cid).expect("decode commit");
        let target_cid = mst_get(&car, commit.data, b"app.bsky.feed.post/aaa")
            .expect("mst_get")
            .expect("present");
        car.blocks.insert(target_cid, b"tampered content".to_vec());
        let tampered_bytes = encode_car(
            commit_cid,
            &car.blocks
                .into_iter()
                .map(|(cid, bytes)| CarBlock { cid, bytes })
                .collect::<Vec<_>>(),
        )
        .expect("re-encode");

        let error = verify_record_proof(&tampered_bytes, DID, "app.bsky.feed.post", "aaa")
            .expect_err("tampered block rejected");
        assert!(error.to_string().contains("does not hash"));
    }
}
