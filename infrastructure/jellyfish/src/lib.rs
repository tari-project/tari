//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

//! Jellyfish Merkle Tree for Tari.
//!
//! # Hash schemes
//!
//! Node hashing is selected by [`JmtHashScheme`], passed explicitly to [`JellyfishMerkleTree::new`] and to every
//! `SparseMerkleProofExt::verify_*` call. There is no default. A verifier takes the scheme from its authenticated
//! context (e.g. the protocol version in a signed header), never from a proof, which carries no scheme tag.
//!
//! | Scheme | Domain (name, version) | Leaf label | Internal label | First crate version |
//! |--------|------------------------|------------|----------------|---------------------|
//! | [`JmtHashScheme::V1`] | `com.tari.jmt`, 0 | `"Leaf"` | `"Internal"` | 6.0.1-pre.2 |
//!
//! Earlier crate versions hashed leaf and internal nodes with the single label `"Node"`. That scheme is not
//! supported: no live root uses it. [`jmt_node_hash`] and [`jmt_node_hash2`] still use the `"Node"` label but are
//! general-purpose helpers (e.g. key derivation), independent of the scheme.
//!
//! A scheme never changes once released. A new scheme is a new variant with its own domain version, and callers gate
//! it on their own protocol version.
//!
//! # Version pinning
//!
//! `command_merkle_root` in a sidechain block header is built by Ootle and verified by `tari_sidechain` in this
//! repository, so both must pin the same `tari_jellyfish` version and map their protocol versions to the same scheme.

#![forbid(unsafe_code)]

mod hash;
pub use hash::*;

mod tree;
pub use tree::*;

mod types;
pub use types::*;

mod error;
pub use error::*;

mod store;

pub use store::*;

mod bit_iter;
#[cfg(any(test, feature = "memory-store"))]
pub mod memory_store;
