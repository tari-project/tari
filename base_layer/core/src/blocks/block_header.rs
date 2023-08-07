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
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};

use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{BlockHash, FixedHash, PrivateKey};
use tari_utilities::{epoch_time::EpochTime, hex::Hex};
use thiserror::Error;

#[cfg(feature = "base_node")]
use crate::blocks::{BlockBuilder, NewBlockHeaderTemplate};
use crate::{
    blocks::BlocksHashDomain,
    consensus::DomainSeparatedConsensusHasher,
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
    #[error("Monero blocks must have a nonce of 0")]
    InvalidNonce,
    #[error("Incorrect height: Expected {expected} but got {actual}")]
    InvalidHeight { expected: u64, actual: u64 },
    #[error("Incorrect previous hash: Expected {expected} but got {actual}")]
    InvalidPreviousHash { expected: BlockHash, actual: BlockHash },
}

/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Serialize, Deserialize, Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BlockHeader {
    /// Version of the block
    pub version: u16,
    /// Height of this block since the genesis block (height 0)
    pub height: u64,
    /// Hash of the block previous to this in the chain.
    pub prev_hash: BlockHash,
    /// Timestamp at which the block was built.
    pub timestamp: EpochTime,
    /// This is the UTXO merkle root of the outputs
    /// This is calculated as Hash (txo MMR root  || roaring bitmap hash of UTXO indices)
    pub output_mr: FixedHash,
    /// The size (number  of leaves) of the output and range proof MMRs at the time of this header
    pub output_mmr_size: u64,
    /// This is the MMR root of the kernels
    pub kernel_mr: FixedHash,
    /// The number of MMR leaves in the kernel MMR
    pub kernel_mmr_size: u64,
    /// This is the Merkle root of the inputs in this block
    pub input_mr: FixedHash,
    /// Sum of kernel offsets for all kernels in this block.
    pub total_kernel_offset: PrivateKey,
    /// Sum of script offsets for all kernels in this block.
    pub total_script_offset: PrivateKey,
    /// Nonce increment used to mine this block.
    pub nonce: u64,
    /// Proof of work summary
    pub pow: ProofOfWork,
    /// Merkle root of all active validator node.
    pub validator_node_mr: FixedHash,
}

impl BlockHeader {
    /// Create a new, default header with the given version.
    pub fn new(blockchain_version: u16) -> BlockHeader {
        BlockHeader {
            version: blockchain_version,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: PrivateKey::default(),
            total_script_offset: PrivateKey::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
        }
    }

    pub fn hash(&self) -> FixedHash {
        DomainSeparatedConsensusHasher::<BlocksHashDomain>::new("block_header")
            .chain(&self.mining_hash())
            .chain(&self.pow)
            .chain(&self.nonce)
            .finalize()
            .into()
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
            output_mr: FixedHash::zero(),
            output_mmr_size: prev.output_mmr_size,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: prev.kernel_mmr_size,
            input_mr: FixedHash::zero(),
            total_kernel_offset: PrivateKey::default(),
            total_script_offset: PrivateKey::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
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

            let (max, min) = headers.windows(2).fold((0u64, u64::MAX), |(max, min), next| {
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

    /// Provides a mining hash of the header, used for the mining.
    /// This differs from the normal hash by not hashing the nonce and kernel pow.
    pub fn mining_hash(&self) -> FixedHash {
        DomainSeparatedConsensusHasher::<BlocksHashDomain>::new("block_header")
            .chain(&self.version)
            .chain(&self.height)
            .chain(&self.prev_hash)
            .chain(&self.timestamp)
            .chain(&self.input_mr)
            .chain(&self.output_mr)
            .chain(&self.output_mmr_size)
            .chain(&self.kernel_mr)
            .chain(&self.kernel_mmr_size)
            .chain(&self.total_kernel_offset)
            .chain(&self.total_script_offset)
            .chain(&self.validator_node_mr)
            .finalize()
            .into()
    }

    pub fn merge_mining_hash(&self) -> FixedHash {
        let mut mining_hash = self.mining_hash();
        mining_hash[0..4].copy_from_slice(b"TARI"); // Maybe put this in a `const`
        mining_hash
    }

    #[inline]
    pub fn timestamp(&self) -> EpochTime {
        self.timestamp
    }

    pub fn to_chrono_datetime(&self) -> DateTime<Utc> {
        let dt = NaiveDateTime::from_timestamp_opt(i64::try_from(self.timestamp.as_u64()).unwrap_or(i64::MAX), 0)
            .unwrap_or(NaiveDateTime::MAX);
        DateTime::from_utc(dt, Utc)
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
            output_mr: FixedHash::zero(),
            output_mmr_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: header_template.total_kernel_offset,
            total_script_offset: header_template.total_script_offset,
            nonce: 0,
            pow: header_template.pow,
            validator_node_mr: FixedHash::zero(),
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
        writeln!(
            fmt,
            "Version: {}\nBlock height: {}\nPrevious block hash: {}\nTimestamp: {}",
            self.version,
            self.height,
            self.prev_hash.to_hex(),
            self.to_chrono_datetime().to_rfc2822()
        )?;
        writeln!(
            fmt,
            "Merkle roots:\nInputs: {},\nOutputs: {} ({})\n\nKernels: {} ({})",
            self.input_mr.to_hex(),
            self.output_mr.to_hex(),
            self.output_mmr_size,
            self.kernel_mr.to_hex(),
            self.kernel_mmr_size
        )?;
        writeln!(fmt, "ValidatorNode: {}\n", self.validator_node_mr.to_hex())?;
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn from_previous() {
        let mut h1 = crate::proof_of_work::sha3x_test::get_header();
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
                ..BlockHeader::new(0)
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
                ..BlockHeader::new(0)
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
            ..BlockHeader::new(0)
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
                ..BlockHeader::new(0)
            })
            .collect::<Vec<_>>();
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
                ..BlockHeader::new(0)
            })
            .collect::<Vec<_>>();
        let (max, min, avg) = BlockHeader::timing_stats(&headers);
        assert_eq!(max, 60);
        assert_eq!(min, 60);
        let error_margin = f64::EPSILON; // Use machine epsilon for comparison of floats
        assert!((avg - 60f64).abs() < error_margin);
    }
}
