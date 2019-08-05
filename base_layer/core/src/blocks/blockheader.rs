// Copyright 2018 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

//! Blockchain state
//!
//! For [technical reasons](https://www.tari.com/2019/07/15/tari-protocol-discussion-42.html), the commitment in
//! block headers commits to the entire TXO set, rather than just UTXOs using a merkle mountain range.
//! However, it's really important to commit to the actual UTXO set at a given height and have the ability to
//! distinguish between spent and unspent outputs in the MMR.
//!
//! To solve this we commit to the MMR root in the header of each block. This will give as an immutable state at the
//! given height. But this does not provide us with a UTXO, only TXO set. To identify UTXOs we create a roaring bit map
//! of all the UTXO's positions inside of the MMR leaves. We hash this, and combine it with the MMR root, to provide us
//! with a TXO set that will represent the UTXO state of the chain at the given height:
//! state = Hash(Hash(mmr_root)|| Hash(roaring_bitmap))
//! This hash is called the UTXO merkle root, and is used as the output_mr

use crate::{pow::*, types::*};
use chrono::{DateTime, NaiveDate, Utc};
use digest::Input;
use serde::{
    de::{self, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use tari_crypto::ristretto::*;
use tari_utilities::{ByteArray, Hashable};
type BlockHash = [u8; 32];
use std::fmt;
use tari_utilities::hex::*;

/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BlockHeader {
    /// Version of the block
    pub version: u16,
    /// Height of this block since the genesis block (height 0)
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    #[serde(with = "hash_serializer")]
    pub prev_hash: BlockHash,
    /// Timestamp at which the block was built.
    pub timestamp: DateTime<Utc>,
    /// This is the UTXO merkle root of the outputs
    /// This is calculated as Hash (txo MMR root  || roaring bitmap hash of UTXO indices)
    #[serde(with = "hash_serializer")]
    pub output_mr: BlockHash,
    /// This is the MMRR root of the range proofs
    #[serde(with = "hash_serializer")]
    pub range_proof_mr: BlockHash,
    /// This is the MMRR root of the kernels
    #[serde(with = "hash_serializer")]
    pub kernel_mr: BlockHash,
    /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
    /// block from the total kernel offset of the previous block header.
    pub total_kernel_offset: BlindingFactor,
    /// Nonce used
    /// Proof of work summary
    pub pow: ProofOfWork,
}

impl BlockHeader {
    /// This function will validate the proof of work in the header
    pub fn validate_pow(&self) -> bool {
        unimplemented!();
    }
}

impl Default for BlockHeader {
    fn default() -> Self {
        BlockHeader {
            version: BLOCKCHAIN_VERSION,
            height: 0,
            prev_hash: [0; 32],
            timestamp: DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc),
            output_mr: [0; 32],
            range_proof_mr: [0; 32],
            kernel_mr: [0; 32],
            total_kernel_offset: RistrettoSecretKey::from(0),
            pow: ProofOfWork::default(),
        }
    }
}

impl Hashable for BlockHeader {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.version.to_le_bytes())
            .chain(self.height.to_le_bytes())
            .chain(self.prev_hash.as_bytes())
            .chain(self.timestamp.timestamp().to_le_bytes())
            .chain(self.output_mr.as_bytes())
            .chain(self.range_proof_mr.as_bytes())
            .chain(self.kernel_mr.as_bytes())
            .chain(self.total_kernel_offset.as_bytes())
            .chain(self.pow.proof_as_bytes())
            .result()
            .to_vec()
    }
}
mod hash_serializer {
    use super::*;
    pub fn serialize<S>(bytes: &BlockHash, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        if serializer.is_human_readable() {
            bytes.to_hex().serialize(serializer)
        } else {
            serializer.serialize_bytes(bytes.as_bytes())
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockHash, D::Error>
    where D: Deserializer<'de> {
        struct BlockHashVisitor;

        impl<'de> Visitor<'de> for BlockHashVisitor {
            type Value = BlockHash;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a bulletproof range proof in binary format")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<BlockHash, E>
            where E: de::Error {
                BlockHash::from_bytes(v).map_err(E::custom)
            }
        }

        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            BlockHash::from_hex(&s).map_err(de::Error::custom)
        } else {
            deserializer.deserialize_bytes(BlockHashVisitor)
        }
    }
}
