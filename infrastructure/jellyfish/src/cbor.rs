//   Copyright 2026 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

//! Native minicbor encoding, enabled by the `minicbor` feature.
//!
//! Every container is a definite-length array, and decoding rejects any other length. Enums are flat arrays led by the
//! variant index in declaration order. The format:
//!
//! | Type | Encoding |
//! |------|----------|
//! | [`TreeHash`] | `bytes(32)` |
//! | [`LeafKey`] | as [`TreeHash`] |
//! | [`Nibble`] | `uint` (`0..=15`) |
//! | [`NibblePath`] | `[num_nibbles, bytes]` |
//! | [`NodeKey`] | `[version, nibble_path]` |
//! | [`NodeType`] | `[0]` leaf, `[1]` null, `[2, leaf_count]` internal |
//! | [`Child`] | `[hash, version, node_type]` |
//! | [`InternalNode`] | `{nibble => child}`, 1 to 16 entries in strictly ascending nibble order |
//! | [`LeafNode`] | `[leaf_key, value_hash, payload, version]` |
//! | [`Node`] | `[0, internal]`, `[1, leaf]`, `[2]` null |
//! | [`StaleTreeNode`] | `[0, node_key]` node, `[1, node_key]` subtree |
//! | [`TreeNode`] | `[0, node]` v1 |
//! | [`SparseMerkleLeafNode`] | `[key, value_hash]` |
//! | [`NodeInProof`] | `[0, leaf]`, `[1, hash]` |
//! | [`SparseMerkleProofExt`] | `[leaf / null, [sibling, ...]]`, at most [`MAX_PROOF_SIBLINGS`] siblings |
//!
//! An [`InternalNode`]'s leaf count is recomputed from its children rather than stored. Decoding applies the same
//! checks as the serde and borsh impls (nibble range, nibble path shape, internal node children, sibling count).

use indexmap::IndexMap;
use minicbor::{
    CborLen,
    Decode,
    Decoder,
    Encode,
    Encoder,
    decode,
    encode::{self, Write},
};

use crate::{
    Child,
    InternalNode,
    LeafKey,
    LeafNode,
    MAX_PROOF_SIBLINGS,
    Nibble,
    NibblePath,
    Node,
    NodeInProof,
    NodeKey,
    NodeType,
    SparseMerkleLeafNode,
    SparseMerkleProofExt,
    StaleTreeNode,
    TreeHash,
    TreeNode,
};

/// The number of children in a full [`InternalNode`].
const MAX_CHILDREN: usize = 16;

/// Reads a definite-length array header, returning its length.
fn array_len(d: &mut Decoder<'_>, what: &'static str) -> Result<u64, decode::Error> {
    let pos = d.position();
    d.array()?
        .ok_or_else(|| decode::Error::message(format!("{what}: expected a definite-length array")).at(pos))
}

/// Reads a definite-length array header and checks its length.
fn expect_array(d: &mut Decoder<'_>, len: u64, what: &'static str) -> Result<(), decode::Error> {
    let pos = d.position();
    let actual = array_len(d, what)?;
    if actual != len {
        return Err(decode::Error::message(format!("{what}: expected an array of {len}, found {actual}")).at(pos));
    }
    Ok(())
}

/// Reads a flat enum's array header and variant index, returning `(array length, index)`.
fn variant(d: &mut Decoder<'_>, what: &'static str) -> Result<(u64, u32), decode::Error> {
    let len = array_len(d, what)?;
    if len == 0 {
        return Err(decode::Error::message(format!("{what}: missing variant index")).at(d.position()));
    }
    Ok((len, d.u32()?))
}

fn check_variant_len(actual: u64, expected: u64, what: &'static str, pos: usize) -> Result<(), decode::Error> {
    if actual != expected {
        return Err(decode::Error::message(format!(
            "{what}: expected a variant array of {expected}, found {actual}"
        ))
        .at(pos));
    }
    Ok(())
}

fn unknown_variant(index: u32, pos: usize) -> decode::Error {
    decode::Error::unknown_variant(i64::from(index)).at(pos)
}

/// The encoded length of an array (or map) header, which is that of a `uint` of the same value.
fn header_len(len: u64) -> usize {
    CborLen::<()>::cbor_len(&len, &mut ())
}

fn sum<const N: usize>(parts: [usize; N]) -> usize {
    parts.into_iter().fold(0, usize::saturating_add)
}

// ---------------------------------------------------------------------------------------------------------------------
// TreeHash, LeafKey

impl<C> Encode<C> for TreeHash {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, _: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.bytes(self.as_slice())?;
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for TreeHash {
    fn decode(d: &mut Decoder<'b>, _: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        TreeHash::try_from_bytes(d.bytes()?).map_err(|_| decode::Error::message("TreeHash must be 32 bytes").at(pos))
    }
}

impl<C> CborLen<C> for TreeHash {
    fn cbor_len(&self, _: &mut C) -> usize {
        sum([header_len(32), 32])
    }
}

impl<C> Encode<C> for LeafKey {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        self.bytes.encode(e, ctx)
    }
}

impl<'b, C> Decode<'b, C> for LeafKey {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        Ok(LeafKey::new(TreeHash::decode(d, ctx)?))
    }
}

impl<C> CborLen<C> for LeafKey {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        self.bytes.cbor_len(ctx)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Nibble, NibblePath, NodeKey

impl<C> Encode<C> for Nibble {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, _: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.u8(u8::from(*self))?;
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for Nibble {
    fn decode(d: &mut Decoder<'b>, _: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        Nibble::try_from(d.u8()?).map_err(|e| decode::Error::message(e).at(pos))
    }
}

impl<C> CborLen<C> for Nibble {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        u8::from(*self).cbor_len(ctx)
    }
}

impl<C> Encode<C> for NibblePath {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.array(2)?;
        self.num_nibbles().encode(e, ctx)?;
        e.bytes(self.bytes())?;
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for NibblePath {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        expect_array(d, 2, "NibblePath")?;
        let pos = d.position();
        let num_nibbles = usize::decode(d, ctx)?;
        let bytes = d.bytes()?.to_vec();
        NibblePath::try_from_parts(num_nibbles, bytes).map_err(|e| decode::Error::message(e).at(pos))
    }
}

impl<C> CborLen<C> for NibblePath {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        let num_bytes = self.bytes().len();
        sum([
            header_len(2),
            self.num_nibbles().cbor_len(ctx),
            num_bytes.cbor_len(ctx),
            num_bytes,
        ])
    }
}

impl<C> Encode<C> for NodeKey {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.array(2)?;
        self.version().encode(e, ctx)?;
        self.nibble_path().encode(e, ctx)
    }
}

impl<'b, C> Decode<'b, C> for NodeKey {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        expect_array(d, 2, "NodeKey")?;
        let version = d.u64()?;
        let nibble_path = NibblePath::decode(d, ctx)?;
        Ok(NodeKey::new(version, nibble_path))
    }
}

impl<C> CborLen<C> for NodeKey {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        sum([
            header_len(2),
            self.version().cbor_len(ctx),
            self.nibble_path().cbor_len(ctx),
        ])
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// NodeType, Child, InternalNode

impl<C> Encode<C> for NodeType {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        match self {
            NodeType::Leaf => {
                e.array(1)?.u32(0)?;
            },
            NodeType::Null => {
                e.array(1)?.u32(1)?;
            },
            NodeType::Internal { leaf_count } => {
                e.array(2)?.u32(2)?;
                leaf_count.encode(e, ctx)?;
            },
        }
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for NodeType {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        let (len, index) = variant(d, "NodeType")?;
        match index {
            0 => {
                check_variant_len(len, 1, "NodeType::Leaf", pos)?;
                Ok(NodeType::Leaf)
            },
            1 => {
                check_variant_len(len, 1, "NodeType::Null", pos)?;
                Ok(NodeType::Null)
            },
            2 => {
                check_variant_len(len, 2, "NodeType::Internal", pos)?;
                Ok(NodeType::Internal {
                    leaf_count: usize::decode(d, ctx)?,
                })
            },
            i => Err(unknown_variant(i, pos)),
        }
    }
}

impl<C> CborLen<C> for NodeType {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        match self {
            // array header and a one-byte index
            NodeType::Leaf | NodeType::Null => 2,
            NodeType::Internal { leaf_count } => sum([2, leaf_count.cbor_len(ctx)]),
        }
    }
}

impl<C> Encode<C> for Child {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.array(3)?;
        self.hash.encode(e, ctx)?;
        self.version.encode(e, ctx)?;
        self.node_type.encode(e, ctx)
    }
}

impl<'b, C> Decode<'b, C> for Child {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        expect_array(d, 3, "Child")?;
        let pos = d.position();
        let hash = TreeHash::decode(d, ctx)?;
        let version = d.u64()?;
        let node_type = NodeType::decode(d, ctx)?;
        Child::try_new(hash, version, node_type).map_err(|e| decode::Error::message(e).at(pos))
    }
}

impl<C> CborLen<C> for Child {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        sum([
            header_len(3),
            self.hash.cbor_len(ctx),
            self.version.cbor_len(ctx),
            self.node_type.cbor_len(ctx),
        ])
    }
}

impl<C> Encode<C> for InternalNode {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        // `InternalNode::try_new` sorts the children, so this map is in ascending nibble order.
        e.map(self.num_children() as u64)?;
        for (nibble, child) in self.children_sorted() {
            nibble.encode(e, ctx)?;
            child.encode(e, ctx)?;
        }
        Ok(())
    }
}

impl<'b, C> Decode<'b, C> for InternalNode {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        let len = d
            .map()?
            .ok_or_else(|| decode::Error::message("InternalNode: expected a definite-length map").at(pos))?;
        if len > MAX_CHILDREN as u64 {
            return Err(
                decode::Error::message(format!("InternalNode has {len} children, max is {MAX_CHILDREN}")).at(pos),
            );
        }
        let mut children = IndexMap::with_capacity(MAX_CHILDREN);
        let mut prev = None;
        for _ in 0..len {
            let nibble_pos = d.position();
            let nibble = Nibble::decode(d, ctx)?;
            if prev.is_some_and(|p| nibble <= p) {
                return Err(
                    decode::Error::message("InternalNode children must be in strictly ascending nibble order")
                        .at(nibble_pos),
                );
            }
            prev = Some(nibble);
            children.insert(nibble, Child::decode(d, ctx)?);
        }
        InternalNode::try_new(children).map_err(|e| decode::Error::message(e).at(pos))
    }
}

impl<C> CborLen<C> for InternalNode {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        self.children_sorted()
            .fold(header_len(self.num_children() as u64), |acc, (nibble, child)| {
                sum([acc, nibble.cbor_len(ctx), child.cbor_len(ctx)])
            })
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// LeafNode, Node, StaleTreeNode, TreeNode

impl<C, P: Encode<C>> Encode<C> for LeafNode<P> {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.array(4)?;
        self.leaf_key().encode(e, ctx)?;
        self.value_hash().encode(e, ctx)?;
        self.payload().encode(e, ctx)?;
        self.version().encode(e, ctx)
    }
}

impl<'b, C, P: Decode<'b, C>> Decode<'b, C> for LeafNode<P> {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        expect_array(d, 4, "LeafNode")?;
        let leaf_key = LeafKey::decode(d, ctx)?;
        let value_hash = TreeHash::decode(d, ctx)?;
        let payload = P::decode(d, ctx)?;
        let version = d.u64()?;
        Ok(LeafNode::new(leaf_key, value_hash, payload, version))
    }
}

impl<C, P: CborLen<C>> CborLen<C> for LeafNode<P> {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        sum([
            header_len(4),
            self.leaf_key().cbor_len(ctx),
            self.value_hash().cbor_len(ctx),
            self.payload().cbor_len(ctx),
            self.version().cbor_len(ctx),
        ])
    }
}

impl<C, P: Encode<C>> Encode<C> for Node<P> {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        match self {
            Node::Internal(node) => {
                e.array(2)?.u32(0)?;
                node.encode(e, ctx)
            },
            Node::Leaf(leaf) => {
                e.array(2)?.u32(1)?;
                leaf.encode(e, ctx)
            },
            Node::Null => {
                e.array(1)?.u32(2)?;
                Ok(())
            },
        }
    }
}

impl<'b, C, P: Decode<'b, C>> Decode<'b, C> for Node<P> {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        let (len, index) = variant(d, "Node")?;
        match index {
            0 => {
                check_variant_len(len, 2, "Node::Internal", pos)?;
                Ok(Node::Internal(InternalNode::decode(d, ctx)?))
            },
            1 => {
                check_variant_len(len, 2, "Node::Leaf", pos)?;
                Ok(Node::Leaf(LeafNode::decode(d, ctx)?))
            },
            2 => {
                check_variant_len(len, 1, "Node::Null", pos)?;
                Ok(Node::Null)
            },
            i => Err(unknown_variant(i, pos)),
        }
    }
}

impl<C, P: CborLen<C>> CborLen<C> for Node<P> {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        match self {
            // array header and a one-byte index
            Node::Internal(node) => sum([2, node.cbor_len(ctx)]),
            Node::Leaf(leaf) => sum([2, leaf.cbor_len(ctx)]),
            Node::Null => 2,
        }
    }
}

impl<C> Encode<C> for StaleTreeNode {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        let index = match self {
            StaleTreeNode::Node(_) => 0,
            StaleTreeNode::Subtree(_) => 1,
        };
        e.array(2)?.u32(index)?;
        self.as_node_key().encode(e, ctx)
    }
}

impl<'b, C> Decode<'b, C> for StaleTreeNode {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        let (len, index) = variant(d, "StaleTreeNode")?;
        check_variant_len(len, 2, "StaleTreeNode", pos)?;
        match index {
            0 => Ok(StaleTreeNode::Node(NodeKey::decode(d, ctx)?)),
            1 => Ok(StaleTreeNode::Subtree(NodeKey::decode(d, ctx)?)),
            i => Err(unknown_variant(i, pos)),
        }
    }
}

impl<C> CborLen<C> for StaleTreeNode {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        sum([2, self.as_node_key().cbor_len(ctx)])
    }
}

impl<C, P: Encode<C>> Encode<C> for TreeNode<P> {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        match self {
            TreeNode::V1(node) => {
                e.array(2)?.u32(0)?;
                node.encode(e, ctx)
            },
        }
    }
}

impl<'b, C, P: Decode<'b, C>> Decode<'b, C> for TreeNode<P> {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        let (len, index) = variant(d, "TreeNode")?;
        match index {
            0 => {
                check_variant_len(len, 2, "TreeNode::V1", pos)?;
                Ok(TreeNode::V1(Node::decode(d, ctx)?))
            },
            i => Err(unknown_variant(i, pos)),
        }
    }
}

impl<C, P: CborLen<C>> CborLen<C> for TreeNode<P> {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        sum([2, self.as_node().cbor_len(ctx)])
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Proofs

impl<C> Encode<C> for SparseMerkleLeafNode {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.array(2)?;
        self.key().encode(e, ctx)?;
        self.value_hash().encode(e, ctx)
    }
}

impl<'b, C> Decode<'b, C> for SparseMerkleLeafNode {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        expect_array(d, 2, "SparseMerkleLeafNode")?;
        let key = LeafKey::decode(d, ctx)?;
        let value_hash = TreeHash::decode(d, ctx)?;
        Ok(SparseMerkleLeafNode::new(key, value_hash))
    }
}

impl<C> CborLen<C> for SparseMerkleLeafNode {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        sum([header_len(2), self.key().cbor_len(ctx), self.value_hash().cbor_len(ctx)])
    }
}

impl<C> Encode<C> for NodeInProof {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        match self {
            NodeInProof::Leaf(leaf) => {
                e.array(2)?.u32(0)?;
                leaf.encode(e, ctx)
            },
            NodeInProof::Other(hash) => {
                e.array(2)?.u32(1)?;
                hash.encode(e, ctx)
            },
        }
    }
}

impl<'b, C> Decode<'b, C> for NodeInProof {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        let pos = d.position();
        let (len, index) = variant(d, "NodeInProof")?;
        check_variant_len(len, 2, "NodeInProof", pos)?;
        match index {
            0 => Ok(NodeInProof::Leaf(SparseMerkleLeafNode::decode(d, ctx)?)),
            1 => Ok(NodeInProof::Other(TreeHash::decode(d, ctx)?)),
            i => Err(unknown_variant(i, pos)),
        }
    }
}

impl<C> CborLen<C> for NodeInProof {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        match self {
            NodeInProof::Leaf(leaf) => sum([2, leaf.cbor_len(ctx)]),
            NodeInProof::Other(hash) => sum([2, hash.cbor_len(ctx)]),
        }
    }
}

impl<C> Encode<C> for SparseMerkleProofExt {
    fn encode<W: Write>(&self, e: &mut Encoder<W>, ctx: &mut C) -> Result<(), encode::Error<W::Error>> {
        e.array(2)?;
        self.leaf_ref().encode(e, ctx)?;
        e.array(self.siblings().len() as u64)?;
        for sibling in self.siblings() {
            sibling.encode(e, ctx)?;
        }
        Ok(())
    }
}

// Rejects oversized proofs as soon as the length is read, rather than decoding every sibling and failing in `verify`.
impl<'b, C> Decode<'b, C> for SparseMerkleProofExt {
    fn decode(d: &mut Decoder<'b>, ctx: &mut C) -> Result<Self, decode::Error> {
        expect_array(d, 2, "SparseMerkleProofExt")?;
        let leaf = Option::<SparseMerkleLeafNode>::decode(d, ctx)?;
        let pos = d.position();
        let len = array_len(d, "SparseMerkleProofExt siblings")?;
        if len > MAX_PROOF_SIBLINGS as u64 {
            return Err(decode::Error::message(format!(
                "SparseMerkleProofExt has {len} siblings, max is {MAX_PROOF_SIBLINGS}"
            ))
            .at(pos));
        }
        let mut siblings = Vec::with_capacity(MAX_PROOF_SIBLINGS);
        for _ in 0..len {
            siblings.push(NodeInProof::decode(d, ctx)?);
        }
        Ok(SparseMerkleProofExt::new(leaf, siblings))
    }
}

impl<C> CborLen<C> for SparseMerkleProofExt {
    fn cbor_len(&self, ctx: &mut C) -> usize {
        self.siblings().iter().fold(
            sum([
                header_len(2),
                self.leaf_ref().cbor_len(ctx),
                header_len(self.siblings().len() as u64),
            ]),
            |acc, sibling| sum([acc, sibling.cbor_len(ctx)]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_NIBBLE_PATH_LEN;

    fn hash(b: u8) -> TreeHash {
        TreeHash::new([b; 32])
    }

    fn nibble(n: u8) -> Nibble {
        Nibble::try_from(n).unwrap()
    }

    /// Encodes `value`, checks `CborLen` agrees, decodes it back and checks it round-trips.
    fn round_trip<T>(value: &T) -> Vec<u8>
    where T: Encode<()> + for<'b> Decode<'b, ()> + CborLen<()> + PartialEq + std::fmt::Debug {
        let bytes = minicbor::to_vec(value).unwrap();
        assert_eq!(minicbor::len(value), bytes.len(), "CborLen mismatch for {value:?}");
        let decoded: T = minicbor::decode(&bytes).unwrap();
        assert_eq!(&decoded, value);
        bytes
    }

    fn decode_err<T: for<'b> Decode<'b, ()>>(bytes: &[u8]) -> String {
        match minicbor::decode::<T>(bytes) {
            Ok(_) => panic!("decode unexpectedly succeeded"),
            Err(e) => e.to_string(),
        }
    }

    fn full_internal_node() -> InternalNode {
        let children = (0..16u8)
            .map(|i| {
                let node_type = if i.is_multiple_of(2) {
                    NodeType::Leaf
                } else {
                    NodeType::Internal { leaf_count: 1000 }
                };
                (nibble(i), Child::try_new(hash(i), 1_000_000, node_type).unwrap())
            })
            .collect();
        InternalNode::try_new(children).unwrap()
    }

    #[test]
    fn primitives_round_trip() {
        assert_eq!(round_trip(&hash(7)).len(), 34);
        round_trip(&LeafKey::new(hash(1)));
        for n in 0..16 {
            round_trip(&nibble(n));
        }
        round_trip(&NibblePath::new_even(vec![]));
        round_trip(&NibblePath::new_even(vec![0x12, 0x34]));
        round_trip(&NibblePath::new_odd(vec![0x12, 0x30]).unwrap());
        round_trip(&NodeKey::new(u64::MAX, NibblePath::new_odd(vec![0xa0]).unwrap()));
        round_trip(&NodeType::Leaf);
        round_trip(&NodeType::Null);
        round_trip(&NodeType::Internal { leaf_count: usize::MAX });
    }

    #[test]
    fn nodes_round_trip() {
        let internal = full_internal_node();
        round_trip(&internal);
        let leaf = LeafNode::new(LeafKey::new(hash(1)), hash(2), 42u32, 9);
        round_trip(&leaf);
        round_trip(&Node::<u32>::Internal(internal.clone()));
        round_trip(&Node::Leaf(leaf.clone()));
        round_trip(&Node::<u32>::Null);
        round_trip(&StaleTreeNode::Node(NodeKey::new_empty_path(3)));
        round_trip(&StaleTreeNode::Subtree(NodeKey::new(
            3,
            NibblePath::new_even(vec![0x12]),
        )));

        let bytes = minicbor::to_vec(TreeNode::new_v1(Node::Leaf(leaf.clone()))).unwrap();
        assert_eq!(minicbor::len(TreeNode::new_v1(Node::Leaf(leaf.clone()))), bytes.len());
        let decoded: TreeNode<u32> = minicbor::decode(&bytes).unwrap();
        assert_eq!(decoded.into_node(), Node::Leaf(leaf));
    }

    #[test]
    fn full_internal_node_is_compact() {
        // 16 children of [hash(34), version(5), node_type(2..5)] keyed by a one-byte nibble.
        let bytes = round_trip(&full_internal_node());
        assert!(bytes.len() < 720, "{} bytes", bytes.len());
    }

    #[test]
    fn proofs_round_trip() {
        let leaf = SparseMerkleLeafNode::new(LeafKey::new(hash(1)), hash(2));
        round_trip(&leaf);
        round_trip(&NodeInProof::Leaf(leaf.clone()));
        round_trip(&NodeInProof::Other(hash(3)));
        round_trip(&SparseMerkleProofExt::new(None, vec![]));
        let siblings = vec![NodeInProof::Other(hash(4)); MAX_PROOF_SIBLINGS];
        let proof = SparseMerkleProofExt::new(Some(leaf), siblings);
        round_trip(&proof);
    }

    #[test]
    fn rejects_bad_tree_hash() {
        let bytes = minicbor::to_vec(minicbor::bytes::ByteVec::from(vec![0u8; 31])).unwrap();
        assert!(decode_err::<TreeHash>(&bytes).contains("32 bytes"));
    }

    #[test]
    fn rejects_out_of_range_nibble() {
        let bytes = minicbor::to_vec(16u8).unwrap();
        decode_err::<Nibble>(&bytes);
    }

    #[test]
    fn rejects_malformed_nibble_path() {
        let encode = |num_nibbles: usize, bytes: Vec<u8>| {
            let mut buf = Vec::new();
            let mut e = Encoder::new(&mut buf);
            e.array(2)
                .unwrap()
                .u64(num_nibbles as u64)
                .unwrap()
                .bytes(&bytes)
                .unwrap();
            buf
        };
        decode_err::<NibblePath>(&encode(3, vec![0x12]));
        decode_err::<NibblePath>(&encode(3, vec![0x12, 0x34]));
        let too_long = MAX_NIBBLE_PATH_LEN + 1;
        decode_err::<NibblePath>(&encode(too_long, vec![0; too_long.div_ceil(2)]));
        assert_eq!(
            minicbor::decode::<NibblePath>(&encode(MAX_NIBBLE_PATH_LEN, vec![0x11; 32]))
                .unwrap()
                .num_nibbles(),
            MAX_NIBBLE_PATH_LEN
        );
    }

    #[test]
    fn rejects_wrong_array_length() {
        let mut buf = Vec::new();
        let mut e = Encoder::new(&mut buf);
        e.array(3).unwrap().u64(1).unwrap();
        NibblePath::new_even(vec![]).encode(&mut e, &mut ()).unwrap();
        e.u8(0).unwrap();
        assert!(decode_err::<NodeKey>(&buf).contains("expected an array of 2"));

        let mut buf = Vec::new();
        Encoder::new(&mut buf).begin_array().unwrap().u64(1).unwrap();
        assert!(decode_err::<NodeKey>(&buf).contains("definite-length"));
    }

    #[test]
    fn rejects_invalid_internal_node() {
        let child = |node_type| Child {
            hash: hash(1),
            version: 1,
            node_type,
        };
        let encode = |entries: &[(u8, Child)]| {
            let mut buf = Vec::new();
            let mut e = Encoder::new(&mut buf);
            e.map(entries.len() as u64).unwrap();
            for (n, c) in entries {
                e.u8(*n).unwrap();
                c.encode(&mut e, &mut ()).unwrap();
            }
            buf
        };

        // No children, a single leaf child, a Null child
        decode_err::<InternalNode>(&encode(&[]));
        decode_err::<InternalNode>(&encode(&[(0, child(NodeType::Leaf))]));
        decode_err::<InternalNode>(&encode(&[(0, child(NodeType::Leaf)), (1, child(NodeType::Null))]));
        // Unsorted and duplicate nibbles
        let err = decode_err::<InternalNode>(&encode(&[(1, child(NodeType::Leaf)), (0, child(NodeType::Leaf))]));
        assert!(err.contains("ascending"), "{err}");
        decode_err::<InternalNode>(&encode(&[(1, child(NodeType::Leaf)), (1, child(NodeType::Leaf))]));
        // Leaf count overflow
        decode_err::<InternalNode>(&encode(&[
            (0, child(NodeType::Internal { leaf_count: usize::MAX })),
            (1, child(NodeType::Leaf)),
        ]));
        // Too many children is rejected from the header alone
        let mut buf = Vec::new();
        Encoder::new(&mut buf).map(17).unwrap();
        assert!(decode_err::<InternalNode>(&buf).contains("max is 16"));

        // Leaf count is recomputed
        let node: InternalNode = minicbor::decode(&encode(&[
            (2, child(NodeType::Internal { leaf_count: 5 })),
            (9, child(NodeType::Leaf)),
        ]))
        .unwrap();
        assert_eq!(node.leaf_count(), 6);
    }

    #[test]
    fn rejects_too_many_siblings() {
        let mut buf = Vec::new();
        let mut e = Encoder::new(&mut buf);
        e.array(2).unwrap().null().unwrap();
        e.array(MAX_PROOF_SIBLINGS as u64 + 1).unwrap();
        // The header alone is rejected, before any sibling is read
        assert!(decode_err::<SparseMerkleProofExt>(&buf).contains("max is 256"));
    }

    #[test]
    fn rejects_unknown_variant() {
        let mut buf = Vec::new();
        Encoder::new(&mut buf).array(1).unwrap().u32(3).unwrap();
        decode_err::<Node<u32>>(&buf);
        decode_err::<NodeType>(&buf);
    }
}
