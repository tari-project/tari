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

use std::{
    cmp::Ordering,
    fmt,
    fmt::{Display, Error, Formatter},
    io,
    io::{Read, Write},
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
use tari_common_types::{
    array::copy_into_fixed_array,
    types::{BlindingFactor, BlockHash, HashDigest, BLOCK_HASH_LENGTH},
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, ByteArray, Hashable};
use thiserror::Error;

#[cfg(feature = "base_node")]
use crate::blocks::{BlockBuilder, NewBlockHeaderTemplate};
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusHashWriter},
    proof_of_work::{PowAlgorithm, PowError, ProofOfWork},
};

#[derive(Debug, Error)]
pub enum BlockHeaderValidationError {
    #[error("The Genesis block header is incorrectly chained")]
    ChainedGenesisBlockHeader,
    #[error("Incorrect Genesis block header")]
    IncorrectGenesisBlockHeader,
    #[error("Header does not form a valid chain")]
    InvalidChaining,
    #[error("Invalid timestamp received on the header: {0}")]
    InvalidTimestamp(String),
    #[error("Invalid timestamp future time limit received on the header")]
    InvalidTimestampFutureTimeLimit,
    #[error("Invalid Proof of work for the header: {0}")]
    ProofOfWorkError(#[from] PowError),
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
    /// This is the MMR root of the witness proofs
    #[serde(with = "hash_serializer")]
    pub witness_mr: BlockHash,
    /// The size (number  of leaves) of the output and range proof MMRs at the time of this header
    pub output_mmr_size: u64,
    /// This is the MMR root of the kernels
    #[serde(with = "hash_serializer")]
    pub kernel_mr: BlockHash,
    /// The number of MMR leaves in the kernel MMR
    pub kernel_mmr_size: u64,
    /// This is the Merkle root of the inputs in this block
    #[serde(with = "hash_serializer")]
    pub input_mr: BlockHash,
    /// Sum of kernel offsets for all kernels in this block.
    pub total_kernel_offset: BlindingFactor,
    /// Sum of script offsets for all kernels in this block.
    pub total_script_offset: BlindingFactor,
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
            witness_mr: vec![0; BLOCK_HASH_LENGTH],
            output_mmr_size: 0,
            kernel_mr: vec![0; BLOCK_HASH_LENGTH],
            kernel_mmr_size: 0,
            input_mr: vec![0; BLOCK_HASH_LENGTH],
            total_kernel_offset: BlindingFactor::default(),
            total_script_offset: BlindingFactor::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
        }
    }

    /// Create a new block header using relevant data from the previous block. The height is incremented by one, the
    /// previous block hash is set, the timestamp is set to the current time, and the kernel/output mmr sizes are set to
    /// the previous block. All other fields, including proof of work are set to defaults.
    pub fn from_previous(prev: &BlockHeader) -> BlockHeader {
        let prev_hash = prev.hash();
        BlockHeader {
            version: prev.version,
            height: prev.height + 1,
            prev_hash,
            timestamp: EpochTime::now(),
            output_mr: vec![0; BLOCK_HASH_LENGTH],
            witness_mr: vec![0; BLOCK_HASH_LENGTH],
            output_mmr_size: prev.output_mmr_size,
            kernel_mr: vec![0; BLOCK_HASH_LENGTH],
            kernel_mmr_size: prev.kernel_mmr_size,
            input_mr: vec![0; BLOCK_HASH_LENGTH],
            total_kernel_offset: BlindingFactor::default(),
            total_script_offset: BlindingFactor::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
        }
    }

    #[cfg(feature = "base_node")]
    pub fn into_builder(self) -> BlockBuilder {
        BlockBuilder::new(self.version).with_header(self)
    }

    /// Given a slice of headers, calculate the maximum, minimum and average periods between them.
    /// Expects the slice of headers to be ordered from youngest to oldest, but will reverse them if not.
    /// This function always allocates a vec of the slice length. This is in case it needs to reverse the list.
    pub fn timing_stats(headers: &[BlockHeader]) -> (u64, u64, f64) {
        if headers.len() < 2 {
            (0, 0, 0.0)
        } else {
            let mut headers = headers.to_vec();

            // ensure the slice is in reverse order
            let ordering = headers[0].timestamp.cmp(&headers[headers.len() - 1].timestamp);
            if ordering == Ordering::Less {
                headers.reverse();
            }

            // unwraps: length already checked
            let last_ts = headers.first().unwrap().timestamp;
            let first_ts = headers.last().unwrap().timestamp;

            let (max, min) = headers.windows(2).fold((0u64, std::u64::MAX), |(max, min), next| {
                let dt = match next[0].timestamp.checked_sub(next[1].timestamp) {
                    Some(delta) => delta.as_u64(),
                    None => 0u64,
                };
                (max.max(dt), min.min(dt))
            });

            let dt = match last_ts.checked_sub(first_ts) {
                Some(t) => t,
                None => 0.into(),
            };
            let n = headers.len() - 1;
            let avg = dt.as_u64() as f64 / n as f64;

            (max, min, avg)
        }
    }

    /// Provides a hash of the header, used for the merge mining.
    /// This differs from the normal hash by not hashing the nonce and kernel pow.
    pub fn merged_mining_hash(&self) -> Vec<u8> {
        if self.version <= 2 {
            // TODO: Remove deprecated header hashing #testnetreset
            HashDigest::new()
                .chain(self.version.to_le_bytes())
                .chain(self.height.to_le_bytes())
                .chain(self.prev_hash.as_bytes())
                .chain(self.timestamp.as_u64().to_le_bytes())
                .chain(self.input_mr.as_bytes())
                .chain(self.output_mr.as_bytes())
                .chain(self.output_mmr_size.to_le_bytes())
                .chain(self.witness_mr.as_bytes())
                .chain(self.kernel_mr.as_bytes())
                .chain(self.kernel_mmr_size.to_le_bytes())
                .chain(self.total_kernel_offset.as_bytes())
                .chain(self.total_script_offset.as_bytes())
                .finalize()
                .to_vec()
        } else {
            ConsensusHashWriter::default()
                .chain(&self.version)
                .chain(&self.height)
                .chain(&self.prev_hash)
                .chain(&self.timestamp)
                .chain(&self.input_mr)
                // TODO: Cleanup if/when we migrate to fixed 32-byte array type for hashes
                .chain(&copy_into_fixed_array::<_, 32>(&self.output_mr).unwrap())
                .chain(&self.output_mmr_size)
                .chain(& copy_into_fixed_array::<_, 32>(&self.witness_mr).unwrap())
                .chain(&copy_into_fixed_array::<_, 32>(&self.kernel_mr).unwrap())
                .chain(&self.kernel_mmr_size)
                .chain(&self.total_kernel_offset)
                .chain(&self.total_script_offset)
                .finalize().to_vec()
        }
    }

    #[inline]
    pub fn timestamp(&self) -> EpochTime {
        self.timestamp
    }

    #[inline]
    pub fn pow_algo(&self) -> PowAlgorithm {
        self.pow.pow_algo
    }
}

#[cfg(feature = "base_node")]
impl From<NewBlockHeaderTemplate> for BlockHeader {
    fn from(header_template: NewBlockHeaderTemplate) -> Self {
        Self {
            version: header_template.version,
            height: header_template.height,
            prev_hash: header_template.prev_hash,
            timestamp: EpochTime::now(),
            output_mr: vec![],
            witness_mr: vec![],
            // TODO: put  mmr sizes in template
            output_mmr_size: 0,
            kernel_mr: vec![],
            kernel_mmr_size: 0,
            input_mr: vec![],
            total_kernel_offset: header_template.total_kernel_offset,
            total_script_offset: header_template.total_script_offset,
            nonce: 0,
            pow: header_template.pow,
        }
    }
}

impl Hashable for BlockHeader {
    fn hash(&self) -> Vec<u8> {
        if self.version <= 2 {
            HashDigest::new()
                .chain(self.merged_mining_hash())
                .chain(self.pow.to_bytes())
                .chain(self.nonce.to_le_bytes())
                .finalize()
                .to_vec()
        } else {
            ConsensusHashWriter::default()
                // TODO: this excludes extraneous length varint used for Vec<u8> since a hash is always 32-bytes. Clean this
                //       up if we decide to migrate to a fixed 32-byte type
                .chain(&copy_into_fixed_array::<_, 32>(&self.merged_mining_hash()).unwrap())
                .chain(&self.pow)
                .chain(& self.nonce)
                .finalize().to_vec()
        }
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
        writeln!(
            fmt,
            "Version: {}\nBlock height: {}\nPrevious block hash: {}\nTimestamp: {}",
            self.version,
            self.height,
            self.prev_hash.to_hex(),
            datetime.to_rfc2822()
        )?;
        writeln!(
            fmt,
            "Merkle roots:\nInputs: {},\nOutputs: {} ({})\nWitness: {}\nKernels: {} ({})\n",
            self.input_mr.to_hex(),
            self.output_mr.to_hex(),
            self.output_mmr_size,
            self.witness_mr.to_hex(),
            self.kernel_mr.to_hex(),
            self.kernel_mmr_size
        )?;
        writeln!(
            fmt,
            "Total offset: {}\nTotal script offset: {}\nNonce: {}\nProof of work:\n{}",
            self.total_kernel_offset.to_hex(),
            self.total_script_offset.to_hex(),
            self.nonce,
            self.pow
        )
    }
}

pub(crate) mod hash_serializer {
    use tari_crypto::tari_utilities::hex::Hex;

    use super::*;

    #[allow(clippy::ptr_arg)]
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

impl ConsensusEncoding for BlockHeader {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = self.version.consensus_encode(writer)?;
        written += self.height.consensus_encode(writer)?;
        written += copy_into_fixed_array::<_, 32>(&self.prev_hash)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Could not copy vec to 32 byte array: {}", e),
                )
            })?
            .consensus_encode(writer)?;
        written += self.timestamp.as_u64().consensus_encode(writer)?;
        written += copy_into_fixed_array::<_, 32>(&self.output_mr)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Could not copy vec to 32 byte array: {}", e),
                )
            })?
            .consensus_encode(writer)?;
        written += copy_into_fixed_array::<_, 32>(&self.witness_mr)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Could not copy vec to 32 byte array: {}", e),
                )
            })?
            .consensus_encode(writer)?;
        written += self.output_mmr_size.consensus_encode(writer)?;
        written += copy_into_fixed_array::<_, 32>(&self.kernel_mr)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Could not copy vec to 32 byte array: {}", e),
                )
            })?
            .consensus_encode(writer)?;
        written += self.kernel_mmr_size.consensus_encode(writer)?;
        written += copy_into_fixed_array::<_, 32>(&self.input_mr)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Could not copy vec to 32 byte array: {}", e),
                )
            })?
            .consensus_encode(writer)?;
        written += self.total_kernel_offset.consensus_encode(writer)?;
        written += self.total_script_offset.consensus_encode(writer)?;
        written += self.nonce.consensus_encode(writer)?;
        written += self.pow.consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusDecoding for BlockHeader {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let version = u16::consensus_decode(reader)?;
        let mut header = BlockHeader::new(version);
        header.height = u64::consensus_decode(reader)?;
        header.prev_hash = <[u8; 32] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        header.timestamp = EpochTime::from(u64::consensus_decode(reader)?);
        header.output_mr = <[u8; 32] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        header.witness_mr = <[u8; 32] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        header.output_mmr_size = u64::consensus_decode(reader)?;
        header.kernel_mr = <[u8; 32] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        header.kernel_mmr_size = u64::consensus_decode(reader)?;
        header.input_mr = <[u8; 32] as ConsensusDecoding>::consensus_decode(reader)?.to_vec();
        header.total_kernel_offset = BlindingFactor::consensus_decode(reader)?;
        header.total_script_offset = BlindingFactor::consensus_decode(reader)?;
        header.nonce = u64::consensus_decode(reader)?;
        header.pow = ProofOfWork::consensus_decode(reader)?;
        Ok(header)
    }
}

#[cfg(test)]
mod test {
    use std::cmp::Ordering;

    use tari_crypto::tari_utilities::Hashable;

    use crate::blocks::BlockHeader;
    #[test]
    fn from_previous() {
        let mut h1 = crate::proof_of_work::sha3_test::get_header();
        h1.nonce = 7600;
        assert_eq!(h1.height, 0, "Default block height");
        let hash1 = h1.hash();
        let h2 = BlockHeader::from_previous(&h1);
        assert_eq!(h2.height, h1.height + 1, "Incrementing block height");
        assert!(h2.timestamp > h1.timestamp, "Timestamp");
        assert_eq!(h2.prev_hash, hash1, "Previous hash");
    }

    #[test]
    fn test_timing_stats() {
        let headers = vec![500, 350, 300, 210, 100u64]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: t.into(),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 150);
        assert_eq!(min, 50);
        let error_margin = f64::EPSILON; // Use an epsilon for comparison of floats
        assert!((avg - 100f64).abs() < error_margin);
    }

    #[test]
    fn timing_negative_blocks() {
        let headers = vec![150, 90, 100u64]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: t.into(),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 60);
        assert_eq!(min, 0);
        let error_margin = f64::EPSILON; // Use machine epsilon for comparison of floats
        assert!((avg - 25f64).abs() < error_margin);
    }

    #[test]
    fn timing_empty_list() {
        let (max, min, avg) = BlockHeader::timing_stats(&[]);
        assert_eq!(max, 0);
        assert_eq!(min, 0);
        let error_margin = f64::EPSILON; // Use machine epsilon for comparison of floats
        assert!((avg - 0f64).abs() < error_margin);
    }

    #[test]
    fn timing_one_block() {
        let header = BlockHeader {
            timestamp: 0.into(),
            ..BlockHeader::default()
        };

        let (max, min, avg) = BlockHeader::timing_stats(&[header]);
        assert_eq!((max, min), (0, 0));
        assert!((avg - 0f64).abs() < f64::EPSILON);
    }

    #[test]
    fn timing_two_blocks() {
        let headers = vec![150, 90]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: t.into(),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 60);
        assert_eq!(min, 60);
        let error_margin = f64::EPSILON; // Use machine epsilon for comparison of floats
        assert!((avg - 60f64).abs() < error_margin);
    }

    #[test]
    fn timing_wrong_order() {
        let headers = vec![90, 150]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: t.into(),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 60);
        assert_eq!(min, 60);
        let error_margin = f64::EPSILON; // Use machine epsilon for comparison of floats
        assert!((avg - 60f64).abs() < error_margin);
    }

    #[test]
    fn compare_timestamps() {
        let headers = vec![90, 90, 150]
            .into_iter()
            .map(|t| BlockHeader {
                timestamp: t.into(),
                ..BlockHeader::default()
            })
            .collect::<Vec<BlockHeader>>();

        let ordering = headers[0].timestamp.cmp(&headers[1].timestamp);
        assert_eq!(ordering, Ordering::Equal);

        let ordering = headers[1].timestamp.cmp(&headers[2].timestamp);
        assert_eq!(ordering, Ordering::Less);

        let ordering = headers[2].timestamp.cmp(&headers[0].timestamp);
        assert_eq!(ordering, Ordering::Greater);
    }
}
