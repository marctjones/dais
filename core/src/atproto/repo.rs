//! AT Protocol repository response helpers.
//!
//! These are platform-agnostic pieces of the PDS compatibility surface. The
//! Cloudflare workers still own DB/R2 reads, but repo identity, status, and
//! describe/list response construction live here so router/PDS code can call
//! core behavior instead of duplicating protocol shapes.

use crate::{CoreError, CoreResult};
use cid::Cid;
use ipld_core::ipld::Ipld;
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::records::stable_cid;

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

#[derive(Clone, Debug, PartialEq)]
pub struct RepoRecord {
    pub path: String,
    pub value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoRecordBlock {
    pub path: String,
    pub cid: Cid,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarBlock {
    pub cid: Cid,
    pub bytes: Vec<u8>,
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

pub fn signing_key_from_secret(secret: &str) -> CoreResult<SigningKey> {
    let digest = Sha256::digest(secret.as_bytes());
    SigningKey::from_bytes((&digest).into())
        .map_err(|error| CoreError::SignatureError(format!("invalid signing seed: {error}")))
}

pub fn repo_snapshot_from_records(
    identity: &AtprotoIdentity,
    rev: impl Into<String>,
    signing_secret: &str,
    records: Vec<RepoRecord>,
) -> CoreResult<RepoSnapshot> {
    let rev = rev.into();
    let mut blocks = records
        .into_iter()
        .map(|record| repo_record_block(record.path, record.value))
        .collect::<CoreResult<Vec<_>>>()?;
    blocks.sort_by(|left, right| left.path.cmp(&right.path));

    let (root_cid, mut car_blocks) = if blocks.is_empty() {
        let bytes = dag_cbor_bytes(&map_ipld([("l", Ipld::Null), ("e", Ipld::List(vec![]))]))?;
        let cid = dag_cbor_cid(&bytes);
        (cid, vec![CarBlock { cid, bytes }])
    } else {
        let min_depth = blocks
            .iter()
            .map(|record| repo_key_depth(record.path.as_bytes()))
            .min()
            .unwrap_or(0);
        mst_subtree(&blocks, 0..blocks.len(), min_depth)?
    };

    let key = signing_key_from_secret(signing_secret)?;
    let unsigned_commit = map_ipld([
        ("did", Ipld::String(identity.did.clone())),
        ("version", Ipld::Integer(3.into())),
        ("rev", Ipld::String(rev.clone())),
        ("prev", Ipld::Null),
        ("data", Ipld::Link(root_cid)),
    ]);
    let unsigned_bytes = dag_cbor_bytes(&unsigned_commit)?;
    let digest = Sha256::digest(&unsigned_bytes);
    let signature: Signature = key
        .sign_prehash(&digest)
        .map_err(|error| CoreError::SignatureError(format!("commit signing failed: {error}")))?;
    let signed_commit = map_ipld([
        ("did", Ipld::String(identity.did.clone())),
        ("version", Ipld::Integer(3.into())),
        ("rev", Ipld::String(rev.clone())),
        ("prev", Ipld::Null),
        ("data", Ipld::Link(root_cid)),
        ("sig", Ipld::Bytes(signature.to_bytes().to_vec())),
    ]);
    let commit_bytes = dag_cbor_bytes(&signed_commit)?;
    let commit_cid = dag_cbor_cid(&commit_bytes);
    car_blocks.insert(
        0,
        CarBlock {
            cid: commit_cid,
            bytes: commit_bytes,
        },
    );
    let car_bytes = encode_car(commit_cid, &car_blocks)?;
    Ok(RepoSnapshot {
        rev,
        commit_cid: commit_cid.to_string(),
        car_bytes,
    })
}

pub fn repo_record_block(path: String, value: Value) -> CoreResult<RepoRecordBlock> {
    if path.trim().is_empty() {
        return Err(CoreError::InvalidAtProto(
            "repo record path is required".to_string(),
        ));
    }
    let ipld = record_value_to_ipld(&value)?;
    let bytes = dag_cbor_bytes(&ipld)?;
    Ok(RepoRecordBlock {
        path,
        cid: dag_cbor_cid(&bytes),
        bytes,
    })
}

pub fn repo_key_depth(key: &[u8]) -> usize {
    let digest = Sha256::digest(key);
    let mut zero_bits = 0usize;
    for byte in digest {
        let count = byte.leading_zeros() as usize;
        zero_bits += count;
        if count != 8 {
            break;
        }
    }
    zero_bits / 2
}

pub fn mst_subtree(
    records: &[RepoRecordBlock],
    range: std::ops::Range<usize>,
    level: usize,
) -> CoreResult<(Cid, Vec<CarBlock>)> {
    if range.is_empty() {
        let bytes = dag_cbor_bytes(&map_ipld([("l", Ipld::Null), ("e", Ipld::List(vec![]))]))?;
        let cid = dag_cbor_cid(&bytes);
        return Ok((cid, vec![CarBlock { cid, bytes }]));
    }
    let slice = &records[range.clone()];
    let local_positions: Vec<usize> = slice
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (repo_key_depth(record.path.as_bytes()) == level).then_some(index)
        })
        .collect();
    if local_positions.is_empty() {
        let (child_cid, child_blocks) = mst_subtree(records, range, level + 1)?;
        let bytes = dag_cbor_bytes(&map_ipld([
            ("l", Ipld::Link(child_cid)),
            ("e", Ipld::List(vec![])),
        ]))?;
        let cid = dag_cbor_cid(&bytes);
        let mut blocks = vec![CarBlock { cid, bytes }];
        blocks.extend(child_blocks);
        return Ok((cid, blocks));
    }

    let mut child_ranges = Vec::new();
    let mut start = range.start;
    for position in &local_positions {
        let absolute = range.start + position;
        child_ranges.push(start..absolute);
        start = absolute + 1;
    }
    child_ranges.push(start..range.end);
    let left = if child_ranges.first().is_some_and(|range| !range.is_empty()) {
        let (cid, _) = mst_subtree(records, child_ranges[0].clone(), level + 1)?;
        Some(cid)
    } else {
        None
    };
    let absolute_positions: Vec<usize> = local_positions
        .iter()
        .map(|position| range.start + position)
        .collect();
    let (node_block, mut blocks) =
        mst_node_block(records, left, &absolute_positions, &child_ranges, level)?;
    blocks[0] = node_block.clone();
    Ok((node_block.cid, blocks))
}

pub fn encode_car(root: Cid, blocks: &[CarBlock]) -> CoreResult<Vec<u8>> {
    let header = map_ipld([
        ("version", Ipld::Integer(1.into())),
        ("roots", Ipld::List(vec![Ipld::Link(root)])),
    ]);
    let header_bytes = dag_cbor_bytes(&header)?;
    let mut output = Vec::new();
    write_uvarint(header_bytes.len() as u64, &mut output);
    output.extend(header_bytes);
    for block in blocks {
        let cid_bytes = block.cid.to_bytes();
        write_uvarint((cid_bytes.len() + block.bytes.len()) as u64, &mut output);
        output.extend(cid_bytes);
        output.extend(&block.bytes);
    }
    Ok(output)
}

fn dag_cbor_cid(bytes: &[u8]) -> Cid {
    use multihash_codetable::{Code, MultihashDigest};

    Cid::new_v1(0x71, Code::Sha2_256.digest(bytes))
}

fn dag_cbor_bytes<T: Serialize>(value: &T) -> CoreResult<Vec<u8>> {
    serde_ipld_dagcbor::to_vec(value)
        .map_err(|error| CoreError::Serialization(format!("dag-cbor encode failed: {error}")))
}

fn cid_link_or_fallback(value: &str, fallback_seed: &str) -> CoreResult<Ipld> {
    match value.parse::<Cid>() {
        Ok(cid) => Ok(Ipld::Link(cid)),
        Err(_) => stable_cid(fallback_seed)
            .parse::<Cid>()
            .map(Ipld::Link)
            .map_err(|error| CoreError::InvalidAtProto(format!("invalid cid '{value}': {error}"))),
    }
}

fn record_value_to_ipld(value: &Value) -> CoreResult<Ipld> {
    let Some(record_type) = value.get("$type").and_then(Value::as_str) else {
        return Err(CoreError::InvalidAtProto(
            "record is missing $type".to_string(),
        ));
    };
    match record_type {
        "app.bsky.actor.profile" => profile_record_ipld(value),
        "app.bsky.feed.post" => post_record_ipld(value),
        "app.bsky.feed.like" | "app.bsky.feed.repost" => subject_record_ipld(value, record_type),
        "app.bsky.graph.follow" => follow_record_ipld(value),
        other => Err(CoreError::InvalidAtProto(format!(
            "unsupported record type for repo export: {other}"
        ))),
    }
}

fn map_ipld(entries: impl IntoIterator<Item = (impl Into<String>, Ipld)>) -> Ipld {
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        map.insert(key.into(), value);
    }
    Ipld::Map(map)
}

fn profile_record_ipld(value: &Value) -> CoreResult<Ipld> {
    let mut entries = vec![(
        "$type".to_string(),
        Ipld::String("app.bsky.actor.profile".to_string()),
    )];
    if let Some(display_name) = value.get("displayName").and_then(Value::as_str) {
        entries.push((
            "displayName".to_string(),
            Ipld::String(display_name.to_string()),
        ));
    }
    if let Some(description) = value.get("description").and_then(Value::as_str) {
        entries.push((
            "description".to_string(),
            Ipld::String(description.to_string()),
        ));
    }
    Ok(map_ipld(entries))
}

fn post_record_ipld(value: &Value) -> CoreResult<Ipld> {
    let mut entries = vec![
        (
            "$type".to_string(),
            Ipld::String("app.bsky.feed.post".to_string()),
        ),
        (
            "text".to_string(),
            Ipld::String(
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        ),
        (
            "createdAt".to_string(),
            Ipld::String(
                value
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        ),
    ];
    if let Some(langs) = value.get("langs").and_then(Value::as_array) {
        entries.push((
            "langs".to_string(),
            Ipld::List(
                langs
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|entry| Ipld::String(entry.to_string()))
                    .collect(),
            ),
        ));
    }
    if let Some(facets) = value.get("facets").and_then(Value::as_array) {
        entries.push(("facets".to_string(), json_array_to_ipld(facets)?));
    }
    if let Some(tags) = value.get("tags").and_then(Value::as_array) {
        entries.push((
            "tags".to_string(),
            Ipld::List(
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(|tag| Ipld::String(tag.to_string()))
                    .collect(),
            ),
        ));
    }
    if let Some(labels) = value.get("labels") {
        entries.push(("labels".to_string(), json_to_plain_ipld(labels)?));
    }
    if let Some(reply) = value.get("reply").and_then(Value::as_object) {
        entries.push(("reply".to_string(), reply_ref_ipld(reply)?));
    }
    if let Some(embed) = value.get("embed").and_then(Value::as_object) {
        entries.push(("embed".to_string(), embed_ipld(embed)?));
    }
    Ok(map_ipld(entries))
}

fn reply_ref_ipld(value: &serde_json::Map<String, Value>) -> CoreResult<Ipld> {
    let root = value
        .get("root")
        .and_then(Value::as_object)
        .ok_or_else(|| CoreError::InvalidAtProto("reply.root is required".to_string()))?;
    let parent = value
        .get("parent")
        .and_then(Value::as_object)
        .ok_or_else(|| CoreError::InvalidAtProto("reply.parent is required".to_string()))?;
    Ok(map_ipld([
        ("root", strong_ref_ipld(root)?),
        ("parent", strong_ref_ipld(parent)?),
    ]))
}

fn strong_ref_ipld(value: &serde_json::Map<String, Value>) -> CoreResult<Ipld> {
    let uri = value.get("uri").and_then(Value::as_str).unwrap_or_default();
    Ok(map_ipld([
        ("uri", Ipld::String(uri.to_string())),
        (
            "cid",
            cid_link_or_fallback(
                value.get("cid").and_then(Value::as_str).unwrap_or_default(),
                uri,
            )?,
        ),
    ]))
}

fn embed_ipld(value: &serde_json::Map<String, Value>) -> CoreResult<Ipld> {
    if value.get("$type").and_then(Value::as_str) != Some("app.bsky.embed.images") {
        return Err(CoreError::InvalidAtProto(
            "only image embeds are supported in repo export".to_string(),
        ));
    }
    let images = value
        .get("images")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::InvalidAtProto("embed.images must be an array".to_string()))?;
    let images = images
        .iter()
        .map(|image| {
            let Some(image) = image.as_object() else {
                return Err(CoreError::InvalidAtProto(
                    "embed image must be an object".to_string(),
                ));
            };
            let blob = image
                .get("image")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    CoreError::InvalidAtProto("embed image blob is required".to_string())
                })?;
            Ok(map_ipld([
                (
                    "alt",
                    Ipld::String(
                        image
                            .get("alt")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    ),
                ),
                (
                    "image",
                    map_ipld([
                        ("$type", Ipld::String("blob".to_string())),
                        (
                            "ref",
                            cid_link_or_fallback(
                                blob.get("ref")
                                    .and_then(Value::as_object)
                                    .and_then(|value| value.get("$link"))
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                                image.get("alt").and_then(Value::as_str).unwrap_or("blob"),
                            )?,
                        ),
                        (
                            "mimeType",
                            Ipld::String(
                                blob.get("mimeType")
                                    .and_then(Value::as_str)
                                    .unwrap_or("image/png")
                                    .to_string(),
                            ),
                        ),
                        (
                            "size",
                            Ipld::Integer(
                                blob.get("size")
                                    .and_then(Value::as_u64)
                                    .unwrap_or_default()
                                    .into(),
                            ),
                        ),
                    ]),
                ),
            ]))
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(map_ipld([
        ("$type", Ipld::String("app.bsky.embed.images".to_string())),
        ("images", Ipld::List(images)),
    ]))
}

fn subject_record_ipld(value: &Value, record_type: &str) -> CoreResult<Ipld> {
    let subject = value
        .get("subject")
        .and_then(Value::as_object)
        .ok_or_else(|| CoreError::InvalidAtProto("subject is required".to_string()))?;
    let subject_uri = subject
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(map_ipld([
        ("$type", Ipld::String(record_type.to_string())),
        (
            "subject",
            map_ipld([
                ("uri", Ipld::String(subject_uri.to_string())),
                (
                    "cid",
                    cid_link_or_fallback(
                        subject
                            .get("cid")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        subject_uri,
                    )?,
                ),
            ]),
        ),
        (
            "createdAt",
            Ipld::String(
                value
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        ),
    ]))
}

fn follow_record_ipld(value: &Value) -> CoreResult<Ipld> {
    Ok(map_ipld([
        ("$type", Ipld::String("app.bsky.graph.follow".to_string())),
        (
            "subject",
            Ipld::String(
                value
                    .get("subject")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        ),
        (
            "createdAt",
            Ipld::String(
                value
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
        ),
    ]))
}

fn json_array_to_ipld(values: &[Value]) -> CoreResult<Ipld> {
    Ok(Ipld::List(
        values
            .iter()
            .map(json_to_plain_ipld)
            .collect::<CoreResult<Vec<_>>>()?,
    ))
}

fn json_to_plain_ipld(value: &Value) -> CoreResult<Ipld> {
    match value {
        Value::Null => Ok(Ipld::Null),
        Value::Bool(value) => Ok(Ipld::Bool(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Ipld::Integer(value.into()))
            } else if let Some(value) = value.as_u64() {
                Ok(Ipld::Integer(value.into()))
            } else {
                Err(CoreError::InvalidAtProto(
                    "floating-point values are not supported in repo export".to_string(),
                ))
            }
        }
        Value::String(value) => Ok(Ipld::String(value.clone())),
        Value::Array(values) => json_array_to_ipld(values),
        Value::Object(map) => Ok(Ipld::Map(
            map.iter()
                .map(|(key, value)| Ok((key.clone(), json_to_plain_ipld(value)?)))
                .collect::<CoreResult<BTreeMap<_, _>>>()?,
        )),
    }
}

fn common_prefix_len(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn mst_node_block(
    records: &[RepoRecordBlock],
    left: Option<Cid>,
    local_positions: &[usize],
    entry_ranges: &[std::ops::Range<usize>],
    level: usize,
) -> CoreResult<(CarBlock, Vec<CarBlock>)> {
    let mut node_entries = Vec::new();
    let mut ordered_blocks = Vec::new();
    if let Some(range) = entry_ranges.first().filter(|range| !range.is_empty()) {
        let (_, blocks) = mst_subtree(records, range.clone(), level + 1)?;
        ordered_blocks.extend(blocks);
    }
    let mut previous_key = Vec::<u8>::new();
    for (index, position) in local_positions.iter().enumerate() {
        let record = &records[*position];
        let key = record.path.as_bytes().to_vec();
        let prefix_len = if index == 0 {
            0
        } else {
            common_prefix_len(&previous_key, &key)
        };
        previous_key = key.clone();
        let mut entry = BTreeMap::new();
        entry.insert("p".to_string(), Ipld::Integer((prefix_len as i64).into()));
        entry.insert("k".to_string(), Ipld::Bytes(key[prefix_len..].to_vec()));
        entry.insert("v".to_string(), Ipld::Link(record.cid));
        if let Some(range) = entry_ranges
            .get(index + 1)
            .filter(|range| !range.is_empty())
        {
            let (child_cid, blocks) = mst_subtree(records, range.clone(), level + 1)?;
            entry.insert("t".to_string(), Ipld::Link(child_cid));
            ordered_blocks.push(record.clone().into());
            ordered_blocks.extend(blocks);
        } else {
            ordered_blocks.push(record.clone().into());
        }
        node_entries.push(Ipld::Map(entry));
    }
    let mut node = BTreeMap::new();
    node.insert("l".to_string(), left.map(Ipld::Link).unwrap_or(Ipld::Null));
    node.insert("e".to_string(), Ipld::List(node_entries));
    let bytes = dag_cbor_bytes(&Ipld::Map(node))?;
    let cid = dag_cbor_cid(&bytes);
    let mut blocks = vec![CarBlock { cid, bytes }];
    blocks.extend(ordered_blocks);
    Ok((blocks[0].clone(), blocks))
}

fn write_uvarint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

impl From<RepoRecordBlock> for CarBlock {
    fn from(value: RepoRecordBlock) -> Self {
        Self {
            cid: value.cid,
            bytes: value.bytes,
        }
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

    #[test]
    fn repo_record_block_emits_dag_cbor_cid() {
        let block = repo_record_block(
            "app.bsky.feed.post/aaa".to_string(),
            serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": "one",
                "createdAt": "2026-07-04T12:00:00.000Z"
            }),
        )
        .expect("record block");

        assert_eq!(block.cid.version(), cid::Version::V1);
        assert_eq!(block.cid.codec(), 0x71);
        assert!(!block.bytes.is_empty());
    }

    #[test]
    fn mst_subtree_and_car_encode_multiple_records() {
        let mut records = vec![
            repo_record_block(
                "app.bsky.actor.profile/self".to_string(),
                serde_json::json!({
                    "$type": "app.bsky.actor.profile",
                    "displayName": "dais"
                }),
            )
            .expect("profile block"),
            repo_record_block(
                "app.bsky.feed.post/aaa".to_string(),
                serde_json::json!({
                    "$type": "app.bsky.feed.post",
                    "text": "one",
                    "createdAt": "2026-07-04T12:00:00.000Z"
                }),
            )
            .expect("post block"),
            repo_record_block(
                "app.bsky.graph.follow/ccc".to_string(),
                serde_json::json!({
                    "$type": "app.bsky.graph.follow",
                    "subject": "did:plc:example",
                    "createdAt": "2026-07-04T12:00:01.000Z"
                }),
            )
            .expect("follow block"),
        ];
        records.sort_by(|left, right| left.path.cmp(&right.path));
        let min_depth = records
            .iter()
            .map(|record| repo_key_depth(record.path.as_bytes()))
            .min()
            .expect("min depth");
        let (root, blocks) = mst_subtree(&records, 0..records.len(), min_depth).expect("mst");
        let car = encode_car(root, &blocks).expect("car");

        assert!(!blocks.is_empty());
        assert!(car.len() > 8);
    }

    #[test]
    fn repo_snapshot_from_records_signs_commit_and_returns_car() {
        let identity = identity();
        let snapshot = repo_snapshot_from_records(
            &identity,
            "2026-07-04T12:00:00.000Z",
            "owner-token",
            vec![
                RepoRecord {
                    path: "app.bsky.actor.profile/self".to_string(),
                    value: serde_json::json!({
                        "$type": "app.bsky.actor.profile",
                        "displayName": "dais"
                    }),
                },
                RepoRecord {
                    path: "app.bsky.feed.post/aaa".to_string(),
                    value: serde_json::json!({
                        "$type": "app.bsky.feed.post",
                        "text": "one",
                        "createdAt": "2026-07-04T12:00:00.000Z"
                    }),
                },
            ],
        )
        .expect("snapshot");

        assert!(snapshot.commit_cid.starts_with("baf"));
        assert_eq!(snapshot.rev, "2026-07-04T12:00:00.000Z");
        assert!(snapshot.car_bytes.len() > 16);
        assert_eq!(
            repo_stats(&snapshot),
            RepoStats {
                head: snapshot.commit_cid.clone(),
                rev: snapshot.rev.clone()
            }
        );
    }
}
