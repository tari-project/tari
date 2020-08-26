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

use crate::{
    base_node::{comms_interface::CommsInterfaceError, LocalNodeCommsInterface},
    blocks::{BlockBuilder, NewBlockHeaderTemplate},
    proof_of_work::{Difficulty, PowError, ProofOfWork},
    transactions::types::{BlindingFactor, HashDigest},
};
use chrono::{DateTime, Utc};
use digest::Digest;
use serde::{
    de::{self, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::{
    fmt,
    fmt::{Display, Error, Formatter},
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, ByteArray, Hashable};
use thiserror::Error;

pub const BLOCK_HASH_LENGTH: usize = 32;
pub type BlockHash = Vec<u8>;

#[derive(Clone, Debug, Error)]
pub enum BlockHeaderValidationError {
    #[error("The Genesis block header is incorrectly chained")]
    ChainedGenesisBlockHeader,
    #[error("Incorrect Genesis block header")]
    IncorrectGenesisBlockHeader,
    #[error("Header does not form a valid chain")]
    InvalidChaining,
    #[error("Invalid timestamp received on the header")]
    InvalidTimestamp,
    #[error("Invalid timestamp future time limit received on the header")]
    InvalidTimestampFutureTimeLimit,
    #[error("Invalid Proof of work for the header: {0}")]
    ProofOfWorkError(#[from] PowError),
    #[error("Mismatched MMR roots")]
    MismatchedMmrRoots,
    #[error("Monero seed hash too old")]
    OldSeedHash,
}

/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BlockHeader {
    /// Version of the block
    pub version: u16,
    /// Height of this block since the genesis block (height 0)
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    #[serde(with = "hash_serializer")]
    pub prev_hash: BlockHash,
    /// Timestamp at which the block was built.
    pub timestamp: EpochTime,
    /// This is the UTXO merkle root of the outputs
    /// This is calculated as Hash (txo MMR root  || roaring bitmap hash of UTXO indices)
    #[serde(with = "hash_serializer")]
    pub output_mr: BlockHash,
    /// This is the MMR root of the range proofs
    #[serde(with = "hash_serializer")]
    pub range_proof_mr: BlockHash,
    /// This is the MMR root of the kernels
    #[serde(with = "hash_serializer")]
    pub kernel_mr: BlockHash,
    /// Sum of kernel offsets for all kernels in this block.
    pub total_kernel_offset: BlindingFactor,
    /// Nonce increment used to mine this block.
    pub nonce: u64,
    /// Proof of work summary
    pub pow: ProofOfWork,
}

impl BlockHeader {
    /// Create a new, default header with the given version.
    pub fn new(blockchain_version: u16) -> BlockHeader {
        BlockHeader {
            version: blockchain_version,
            height: 0,
            prev_hash: vec![0; BLOCK_HASH_LENGTH],
            timestamp: EpochTime::now(),
            output_mr: vec![0; BLOCK_HASH_LENGTH],
            range_proof_mr: vec![0; BLOCK_HASH_LENGTH],
            kernel_mr: vec![0; BLOCK_HASH_LENGTH],
            total_kernel_offset: BlindingFactor::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
        }
    }

    /// Create a new block header using relevant data from the previous block. The height is incremented by one, the
    /// previous block hash is set, and the timestamp is set to the current time and the proof of work is partially
    /// initialized, although the `accumulated_difficulty_<algo>` stats are updated using the previous block's proof
    /// of work information.
    pub fn from_previous(prev: &BlockHeader) -> Result<BlockHeader, BlockHeaderValidationError> {
        let prev_hash = prev.hash();
        let mut pow = ProofOfWork::default();
        pow.add_difficulty(&prev.pow, prev.achieved_difficulty()?);
        Ok(BlockHeader {
            version: prev.version,
            height: prev.height + 1,
            prev_hash,
            timestamp: EpochTime::now(),
            output_mr: vec![0; BLOCK_HASH_LENGTH],
            range_proof_mr: vec![0; BLOCK_HASH_LENGTH],
            kernel_mr: vec![0; BLOCK_HASH_LENGTH],
            total_kernel_offset: BlindingFactor::default(),
            nonce: 0,
            pow,
        })
    }

    /// Calculates and returns the achieved difficulty for this header and associated proof of work.
    pub fn achieved_difficulty(&self) -> Result<Difficulty, PowError> {
        ProofOfWork::achieved_difficulty(self)
    }

    /// Calculates the total accumulated difficulty for the blockchain from the genesis block up until (and including)
    /// this block.
    pub fn total_accumulated_difficulty_inclusive(&self) -> Result<Difficulty, PowError> {
        let mut prev_pow = self.pow.clone();
        prev_pow.add_difficulty(&self.pow, self.achieved_difficulty()?);
        Ok(prev_pow.total_accumulated_difficulty())
    }

    pub fn into_builder(self) -> BlockBuilder {
        BlockBuilder::new(self.version).with_header(self)
    }

    pub async fn get_heights_from_tip(
        mut handler: LocalNodeCommsInterface,
        height_from_tip: u64,
    ) -> Result<Vec<u64>, CommsInterfaceError>
    {
        let metadata = handler.get_metadata().await?;
        let tip = metadata.height_of_longest_chain.unwrap_or(0);
        // Avoid overflow
        let height_from_tip = std::cmp::min(tip, height_from_tip);
        let start = std::cmp::max(tip - height_from_tip, 0);
        Ok(BlockHeader::get_height_range(start, tip))
    }

    /// Returns a height range in descending order
    pub fn get_height_range(start: u64, end_inclusive: u64) -> Vec<u64> {
        let mut heights: Vec<u64> =
            (std::cmp::min(start, end_inclusive)..=std::cmp::max(start, end_inclusive)).collect();
        heights.reverse();
        heights
    }

    /// Given a slice of headers (in reverse order), calculate the maximum, minimum and average periods between them
    pub fn timing_stats(headers: &[BlockHeader]) -> (u64, u64, f64) {
        let (max, min) = headers.windows(2).fold((0u64, std::u64::MAX), |(max, min), next| {
            let delta_t = match next[0].timestamp.checked_sub(next[1].timestamp) {
                Some(delta) => delta.as_u64(),
                None => 0u64,
            };
            let min = min.min(delta_t);
            let max = max.max(delta_t);
            (max, min)
        });
        let avg = if headers.len() >= 2 {
            let dt = headers.first().unwrap().timestamp - headers.last().unwrap().timestamp;
            let n = headers.len() - 1;
            dt.as_u64() as f64 / n as f64
        } else {
            0.0
        };
        (max, min, avg)
    }

    /// Provides a hash of the header, used for the merge mining.
    /// This differs from the normal hash by not hashing the nonce and kernel pow.
    pub fn merged_mining_hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.version.to_le_bytes())
            .chain(self.height.to_le_bytes())
            .chain(self.prev_hash.as_bytes())
            .chain(self.timestamp.as_u64().to_le_bytes())
            .chain(self.output_mr.as_bytes())
            .chain(self.range_proof_mr.as_bytes())
            .chain(self.kernel_mr.as_bytes())
            .chain(self.total_kernel_offset.as_bytes())
            .result()
            .to_vec()
    }
}

impl From<NewBlockHeaderTemplate> for BlockHeader {
    fn from(header_template: NewBlockHeaderTemplate) -> Self {
        Self {
            version: header_template.version,
            height: header_template.height,
            prev_hash: header_template.prev_hash,
            timestamp: EpochTime::now(),
            output_mr: vec![0; 32],
            range_proof_mr: vec![0; 32],
            kernel_mr: vec![0; 32],
            total_kernel_offset: header_template.total_kernel_offset,
            nonce: 0,
            pow: header_template.pow,
        }
    }
}

impl Hashable for BlockHeader {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.version.to_le_bytes())
            .chain(self.height.to_le_bytes())
            .chain(self.prev_hash.as_bytes())
            .chain(self.timestamp.as_u64().to_le_bytes())
            .chain(self.output_mr.as_bytes())
            .chain(self.range_proof_mr.as_bytes())
            .chain(self.kernel_mr.as_bytes())
            .chain(self.total_kernel_offset.as_bytes())
            .chain(self.nonce.to_le_bytes())
            .chain(self.pow.to_bytes())
            .result()
            .to_vec()
    }
}

impl PartialEq for BlockHeader {
    fn eq(&self, other: &Self) -> bool {
        self.hash() == other.hash()
    }
}

impl Eq for BlockHeader {}

impl Display for BlockHeader {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let datetime: DateTime<Utc> = self.timestamp.into();
        let msg = format!(
            "Version: {}\nBlock height: {}\nPrevious block hash: {}\nTimestamp: {}\n",
            self.version,
            self.height,
            self.prev_hash.to_hex(),
            datetime.to_rfc2822()
        );
        fmt.write_str(&msg)?;
        let msg = format!(
            "Merkle roots:\nOutputs: {}\nRange proofs: {}\nKernels: {}\n",
            self.output_mr.to_hex(),
            self.range_proof_mr.to_hex(),
            self.kernel_mr.to_hex()
        );
        fmt.write_str(&msg)?;
        fmt.write_str(&format!(
            "Total offset: {}\nNonce: {}\nProof of work:\n{}",
            self.total_kernel_offset.to_hex(),
            self.nonce,
            self.pow
        ))
    }
}

pub(crate) mod hash_serializer {
    use super::*;
    use tari_crypto::tari_utilities::hex::Hex;

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
                formatter.write_str("A block header hash in binary format")
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

#[cfg(test)]
mod test {
    use crate::{blocks::BlockHeader, tari_utilities::epoch_time::EpochTime};
    use tari_crypto::tari_utilities::Hashable;
    #[test]
    fn from_previous() {
        let mut h1 = crate::proof_of_work::blake_test::get_header();
        h1.nonce = 7600; // Achieved difficulty is 18,138;
        assert_eq!(h1.height, 0, "Default block height");
        let hash1 = h1.hash();
        let diff1 = h1.achieved_difficulty();
        assert_eq!(diff1, 18138.into());
        let h2 = BlockHeader::from_previous(&h1);
        assert_eq!(h2.height, h1.height + 1, "Incrementing block height");
        assert!(h2.timestamp > h1.timestamp, "Timestamp");
        assert_eq!(h2.prev_hash, hash1, "Previous hash");
        // default pow is blake, so monero diff should stay the same
        assert_eq!(
            h2.pow.accumulated_monero_difficulty, h1.pow.accumulated_monero_difficulty,
            "Monero difficulty"
        );
        assert_eq!(
            h2.pow.accumulated_blake_difficulty,
            h1.pow.accumulated_blake_difficulty + diff1,
            "Blake difficulty"
        );
    }

    #[test]
    fn test_timing_stats() {
        let headers = vec![500, 350, 300, 210, 100u64]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: EpochTime::from(t),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 150);
        assert_eq!(min, 50);
        assert_eq!(avg, 100f64);
    }

    #[test]
    fn timing_negative_blocks() {
        let headers = vec![150, 90, 100u64]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: EpochTime::from(t),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 60);
        assert_eq!(min, 0);
        assert_eq!(avg, 25f64);
    }

    #[test]
    fn timing_empty_list() {
        let (max, min, avg) = BlockHeader::timing_stats(&[]);
        assert_eq!(max, 0);
        assert_eq!(min, std::u64::MAX);
        assert_eq!(avg, 0f64);
    }
}
