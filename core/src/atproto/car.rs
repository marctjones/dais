//! CAR v1 (Content Addressable aRchive) container decode.
//!
//! Read side of the format `repo::encode_car` writes: a varint-length-prefixed
//! DAG-CBOR header (`{version, roots}`) followed by a sequence of
//! varint-length-prefixed `(CID, block bytes)` sections.
//! <https://ipld.io/specs/transport/car/carv1/>
//!
//! `decode_car` only unpacks the container into a root CID and a
//! `(CID -> bytes)` block map — it does not interpret what those blocks mean.
//! An AT Protocol repo commit is a DAG-CBOR object among those blocks, and
//! walking the MST it points to is a distinct step layered on top of the map
//! this produces, scoped separately (dais issue #50).
//!
//! Every CAR file dais currently has to decode is one it received over the
//! wire from a peer, so every read here is defensive: undersized inputs,
//! truncated sections, or a header that isn't a CBOR map with the expected
//! shape all return `CoreError` rather than panicking.

use std::collections::BTreeMap;

use cid::Cid;
use ipld_core::ipld::Ipld;

use crate::error::{CoreError, CoreResult};

/// A decoded CAR v1 file: the root CID(s) declared in the header, and every
/// block the archive carries, keyed by CID for the MST walk to look up.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct CarFile {
    pub roots: Vec<Cid>,
    pub blocks: BTreeMap<Cid, Vec<u8>>,
}

impl CarFile {
    pub fn block(&self, cid: &Cid) -> Option<&[u8]> {
        self.blocks.get(cid).map(Vec::as_slice)
    }
}

pub fn decode_car(bytes: &[u8]) -> CoreResult<CarFile> {
    let mut cursor = bytes;

    let header_len = read_uvarint(&mut cursor)?;
    let header_bytes = take(&mut cursor, header_len)?;
    let roots = decode_header_roots(header_bytes)?;

    let mut blocks = BTreeMap::new();
    while !cursor.is_empty() {
        let section_len = read_uvarint(&mut cursor)?;
        let section = take(&mut cursor, section_len)?;
        let (cid, block_bytes) = decode_section(section)?;
        blocks.insert(cid, block_bytes.to_vec());
    }

    Ok(CarFile { roots, blocks })
}

fn decode_header_roots(header_bytes: &[u8]) -> CoreResult<Vec<Cid>> {
    let header: Ipld = serde_ipld_dagcbor::from_slice(header_bytes).map_err(|error| {
        CoreError::InvalidAtProto(format!("CAR header is not DAG-CBOR: {error}"))
    })?;
    let Ipld::Map(fields) = header else {
        return Err(CoreError::InvalidAtProto(
            "CAR header must be a CBOR map".to_string(),
        ));
    };
    match fields.get("version") {
        Some(Ipld::Integer(version)) if *version == 1 => {}
        Some(other) => {
            return Err(CoreError::InvalidAtProto(format!(
                "unsupported CAR version: {other:?}"
            )))
        }
        None => {
            return Err(CoreError::InvalidAtProto(
                "CAR header is missing version".to_string(),
            ))
        }
    }
    let Some(Ipld::List(roots)) = fields.get("roots") else {
        return Err(CoreError::InvalidAtProto(
            "CAR header is missing roots".to_string(),
        ));
    };
    roots
        .iter()
        .map(|root| match root {
            Ipld::Link(cid) => Ok(*cid),
            other => Err(CoreError::InvalidAtProto(format!(
                "CAR header root is not a CID link: {other:?}"
            ))),
        })
        .collect()
}

/// A section is `CID bytes || block bytes` with no length prefix of its own
/// on the CID — `Cid::read_bytes` consumes exactly the CID's own encoding
/// (self-describing: multibase-less binary CID) and leaves the remainder as
/// the block payload, whatever codec that block turns out to use.
fn decode_section(section: &[u8]) -> CoreResult<(Cid, &[u8])> {
    let mut reader = section;
    let cid = Cid::read_bytes(&mut reader)
        .map_err(|error| CoreError::InvalidAtProto(format!("invalid CAR block CID: {error}")))?;
    Ok((cid, reader))
}

fn read_uvarint(cursor: &mut &[u8]) -> CoreResult<usize> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    loop {
        let Some((&byte, rest)) = cursor.split_first() else {
            return Err(CoreError::InvalidAtProto(
                "CAR file truncated inside a varint".to_string(),
            ));
        };
        *cursor = rest;
        // CAR section/header lengths fit comfortably under 2^32 in practice;
        // this bound exists to reject corrupt/hostile input rather than to
        // accommodate any real archive, so 9 continuation bytes (63 value
        // bits) is deliberately generous rather than tight.
        if shift >= 63 {
            return Err(CoreError::InvalidAtProto(
                "CAR varint is too large".to_string(),
            ));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    usize::try_from(value)
        .map_err(|_| CoreError::InvalidAtProto("CAR varint overflows usize".to_string()))
}

fn take<'a>(cursor: &mut &'a [u8], len: usize) -> CoreResult<&'a [u8]> {
    if cursor.len() < len {
        return Err(CoreError::InvalidAtProto(
            "CAR file truncated: declared length exceeds remaining bytes".to_string(),
        ));
    }
    let (head, rest) = cursor.split_at(len);
    *cursor = rest;
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atproto::repo::{encode_car, CarBlock};
    use multihash_codetable::{Code, MultihashDigest};

    fn cbor_block(value: &Ipld) -> CarBlock {
        let bytes = serde_ipld_dagcbor::to_vec(value).expect("encode fixture block");
        let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(&bytes));
        CarBlock { cid, bytes }
    }

    #[test]
    fn round_trips_a_car_file_written_by_encode_car() {
        // decode_car is the read side of the exact writer this codebase
        // already ships (repo::encode_car, used for the owner's own repo
        // export/backup) — round-tripping through it is a real correctness
        // check against a trusted, independently-tested encoder, not just
        // "my decoder agrees with itself."
        let leaf = cbor_block(&Ipld::String("hello from the MST".to_string()));
        let parent = cbor_block(&Ipld::List(vec![Ipld::Link(leaf.cid), Ipld::Integer(7)]));
        let blocks = vec![leaf.clone(), parent.clone()];

        let car_bytes = encode_car(parent.cid, &blocks).expect("encode CAR fixture");
        let decoded = decode_car(&car_bytes).expect("decode CAR fixture");

        assert_eq!(decoded.roots, vec![parent.cid]);
        assert_eq!(decoded.blocks.len(), 2);
        assert_eq!(decoded.block(&leaf.cid), Some(leaf.bytes.as_slice()));
        assert_eq!(decoded.block(&parent.cid), Some(parent.bytes.as_slice()));
    }

    #[test]
    fn round_trips_many_blocks_and_preserves_byte_identical_payloads() {
        let blocks: Vec<CarBlock> = (0..25)
            .map(|index| cbor_block(&Ipld::Integer(index)))
            .collect();
        let root = blocks[0].cid;
        let car_bytes = encode_car(root, &blocks).expect("encode CAR fixture");

        let decoded = decode_car(&car_bytes).expect("decode CAR fixture");

        assert_eq!(decoded.blocks.len(), blocks.len());
        for block in &blocks {
            assert_eq!(decoded.block(&block.cid), Some(block.bytes.as_slice()));
        }
    }

    #[test]
    fn round_trips_a_car_with_no_blocks() {
        let root = cbor_block(&Ipld::Null).cid;
        let car_bytes = encode_car(root, &[]).expect("encode empty CAR fixture");

        let decoded = decode_car(&car_bytes).expect("decode empty CAR fixture");

        assert_eq!(decoded.roots, vec![root]);
        assert!(decoded.blocks.is_empty());
    }

    #[test]
    fn rejects_a_car_with_an_unsupported_version() {
        let header = Ipld::Map(BTreeMap::from([
            ("version".to_string(), Ipld::Integer(2)),
            ("roots".to_string(), Ipld::List(vec![])),
        ]));
        let header_bytes = serde_ipld_dagcbor::to_vec(&header).unwrap();
        let mut car_bytes = Vec::new();
        write_test_uvarint(header_bytes.len() as u64, &mut car_bytes);
        car_bytes.extend(header_bytes);

        let error = decode_car(&car_bytes).expect_err("version 2 must be rejected");
        assert!(error.to_string().contains("unsupported CAR version"));
    }

    #[test]
    fn rejects_a_truncated_section() {
        let root = cbor_block(&Ipld::Null).cid;
        let mut car_bytes = encode_car(root, &[cbor_block(&Ipld::Integer(1))]).unwrap();
        car_bytes.truncate(car_bytes.len() - 1);

        let error = decode_car(&car_bytes).expect_err("truncated section must be rejected");
        assert!(error.to_string().contains("truncated"));
    }

    #[test]
    fn rejects_a_header_that_is_not_a_cbor_map() {
        let header_bytes = serde_ipld_dagcbor::to_vec(&Ipld::Integer(1)).unwrap();
        let mut car_bytes = Vec::new();
        write_test_uvarint(header_bytes.len() as u64, &mut car_bytes);
        car_bytes.extend(header_bytes);

        let error = decode_car(&car_bytes).expect_err("non-map header must be rejected");
        assert!(error.to_string().contains("must be a CBOR map"));
    }

    fn write_test_uvarint(mut value: u64, output: &mut Vec<u8>) {
        while value >= 0x80 {
            output.push((value as u8) | 0x80);
            value >>= 7;
        }
        output.push(value as u8);
    }
}
