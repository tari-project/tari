// Copyright 2020 The Tari Project
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

//! Header row object is used to store the blockheader as well as extra detail such as the target_difficulty and the
//! total achieved difficulty.

use crate::{
    blocks::{BlockBuilder, BlockHeader, BlockHeaderValidationError},
    proof_of_work::{Difficulty, PowAlgorithm, PowError},
};
use chrono::{DateTime, Utc};
use digest::Digest;
use log::*;
use serde::{
    de::{self, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::{
    collections::HashMap,
    fmt,
    fmt::{Display, Error, Formatter},
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, ByteArray, Hashable};
use thiserror::Error;
const LOG_TARGET: &str = "c::bn::blocks::chain_header";

/// The BlockHeader contains all the metadata for the block, including proof of work, a link to the previous block
/// and the transaction kernels.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ChainHeader {
    /// Header of the block
    header: BlockHeader,
    /// The target difficulty for solving the current block using the specified proof of work algorithm.
    pub target_difficulty: Difficulty,
    /// This tracks the achieved difficulty per algorithm. Allowed algorithms is controlled by the consensus
    /// constants.
    pub achieved_difficulty: HashMap<PowAlgorithm, Difficulty>,
}

impl ChainHeader {
    /// Create a new, default header row with the given version.
    pub fn new(blockchain_version: u16) -> ChainHeader {
        ChainHeader {
            header: BlockHeader::new(blockchain_version),
            target_difficulty: Difficulty::default(),
            achieved_difficulty: HashMap::new(),
        }
    }

    pub fn new_blank_from_header(header: BlockHeader) -> ChainHeader{
        ChainHeader {
            header: header,
            target_difficulty: Difficulty::default(),
            achieved_difficulty: HashMap::new(),
        }
    }

    /// Create a new header_row using a previous header_row. Internally it uses blockHeader::from_previous() to create a
    /// new block header. The achieved difficulty is also calculated for the header and kept track of. The target
    /// difficulty is initialised as default and an initial achieved difficulty is calculated. Although this is later
    /// replaced by a new one.
    pub fn from_previous(prev: &ChainHeader) -> Result<ChainHeader, BlockHeaderValidationError> {
        let header = BlockHeader::from_previous(&prev.header)?;
        ChainHeader::from_previous_with_header(prev, header, Difficulty::default())
    }

    /// Create a new header_row using a previous header_row and the provided header. The achieved difficulty is also
    /// calculated for the header and kept track of. The Achieved difficulty is calculated and verified with the
    /// provided target.
    pub fn from_previous_with_header(
        prev: &ChainHeader,
        header: BlockHeader,
        target: Difficulty,
    ) -> Result<ChainHeader, BlockHeaderValidationError>
    {
        let header = BlockHeader::from_previous(&prev.header)?;
        let mut total_achieved_difficulty = prev.achieved_difficulty.clone();
        let achieved_difficulty = total_achieved_difficulty
            .get(&header.pow.pow_algo)
            .unwrap_or(&Difficulty::default());
        let header_diff = header.achieved_difficulty()?;
        if header_diff < target {
            warn!(
                target: LOG_TARGET,
                "Proof of work for {} was below the target difficulty. Achieved: {}, Target:{}",
                header.hash().to_hex(),
                header_diff,
                target
            );
            return Err(BlockHeaderValidationError::ProofOfWorkError(
                PowError::AchievedDifficultyTooLow {
                    achieved: header_diff,
                    target,
                },
            ));
        }
        let achieved_difficulty = achieved_difficulty + &header_diff;
        total_achieved_difficulty.insert(header.pow.pow_algo, achieved_difficulty);
        Ok(ChainHeader {
            header,
            target_difficulty: target,
            achieved_difficulty: total_achieved_difficulty,
        })
    }

    /// This gets a ref to the header inside of the header_row
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    /// This deconstructs the header row into just the header
    pub fn to_header(self) -> BlockHeader {
        self.header
    }

    /// Calculates the total accumulated difficulty for the blockchain from the genesis block up until (and including)
    /// this block.
    pub fn total_accumulated_difficulty_inclusive_squared(&self) -> Result<Difficulty, PowError> {
        let mut result = 1.into();
        for (_, val) in self.achieved_difficulty.iter() {
            result = result * *val;
        }
        Ok(result)
    }

    pub fn into_builder(self) -> BlockBuilder {
        BlockBuilder::new(self.header.version).with_chain_header(self)
    }

    /// Given a slice of headers (in reverse order), calculate the maximum, minimum and average periods between them
    pub fn timing_stats(headers: &[ChainHeader]) -> (u64, u64, f64) {
        let (max, min) = headers.windows(2).fold((0u64, std::u64::MAX), |(max, min), next| {
            let delta_t = match next[0].header.timestamp.checked_sub(next[1].header.timestamp) {
                Some(delta) => delta.as_u64(),
                None => 0u64,
            };
            let min = min.min(delta_t);
            let max = max.max(delta_t);
            (max, min)
        });
        let avg = if headers.len() >= 2 {
            let dt = headers.first().unwrap().header.timestamp - headers.last().unwrap().header.timestamp;
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
        self.header.merged_mining_hash()
    }
}

// impl From<NewBlockHeaderTemplate> for ChainHeader {
//     fn from(header_template: NewBlockHeaderTemplate) -> Self {
//         let header = BlockHeader::from(header_template);
//     }
// }

impl Hashable for ChainHeader {
    fn hash(&self) -> Vec<u8> {
        self.header.hash()
    }
}

impl PartialEq for ChainHeader {
    fn eq(&self, other: &Self) -> bool {
        self.hash() == other.hash()
    }
}

impl Eq for ChainHeader {}

impl Display for ChainHeader {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let datetime: DateTime<Utc> = self.header.timestamp.into();
        let msg = format!(
            "Version: {}\nBlock height: {}\nPrevious block hash: {}\nTimestamp: {}\n",
            self.header.version,
            self.header.height,
            self.header.prev_hash.to_hex(),
            datetime.to_rfc2822()
        );
        fmt.write_str(&msg)?;
        let msg = format!(
            "Merkle roots:\nOutputs: {}\nRange proofs: {}\nKernels: {}\n",
            self.header.output_mr.to_hex(),
            self.header.range_proof_mr.to_hex(),
            self.header.kernel_mr.to_hex()
        );
        fmt.write_str(&msg)?;
        fmt.write_str(&format!(
            "Total offset: {}\nNonce: {}\nProof of work:\n{}",
            self.header.total_kernel_offset.to_hex(),
            self.header.nonce,
            self.header.pow
        ))
    }
}

// pub(crate) mod hash_serializer {
//     use super::*;
//     use tari_crypto::tari_utilities::hex::Hex;

//     #[allow(clippy::ptr_arg)]
//     pub fn serialize<S>(bytes: &BlockHash, serializer: S) -> Result<S::Ok, S::Error>
//     where S: Serializer {
//         if serializer.is_human_readable() {
//             bytes.to_hex().serialize(serializer)
//         } else {
//             serializer.serialize_bytes(bytes.as_bytes())
//         }
//     }

//     pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockHash, D::Error>
//     where D: Deserializer<'de> {
//         struct BlockHashVisitor;

//         impl<'de> Visitor<'de> for BlockHashVisitor {
//             type Value = BlockHash;

//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("A block header hash in binary format")
//             }

//             fn visit_bytes<E>(self, v: &[u8]) -> Result<BlockHash, E>
//             where E: de::Error {
//                 BlockHash::from_bytes(v).map_err(E::custom)
//             }
//         }

//         if deserializer.is_human_readable() {
//             let s = String::deserialize(deserializer)?;
//             BlockHash::from_hex(&s).map_err(de::Error::custom)
//         } else {
//             deserializer.deserialize_bytes(BlockHashVisitor)
//         }
//     }
// }

// #[cfg(test)]
// mod test {
//     use crate::{blocks::BlockHeader, tari_utilities::epoch_time::EpochTime};
//     use tari_crypto::tari_utilities::Hashable;
//     #[test]
//     fn from_previous() {
//         let mut h1 = crate::proof_of_work::blake_test::get_header();
//         h1.nonce = 7600; // Achieved difficulty is 18,138;
//         assert_eq!(h1.height, 0, "Default block height");
//         let hash1 = h1.hash();
//         let diff1 = h1.achieved_difficulty().unwrap();
//         assert_eq!(diff1, 18138.into());
//         let h2 = BlockHeader::from_previous(&h1).unwrap();
//         assert_eq!(h2.height, h1.height + 1, "Incrementing block height");
//         assert!(h2.timestamp > h1.timestamp, "Timestamp");
//         assert_eq!(h2.prev_hash, hash1, "Previous hash");
//         // default pow is blake, so monero diff should stay the same
//         assert_eq!(
//             h2.pow.accumulated_monero_difficulty, h1.pow.accumulated_monero_difficulty,
//             "Monero difficulty"
//         );
//         assert_eq!(
//             h2.pow.accumulated_blake_difficulty,
//             h1.pow.accumulated_blake_difficulty + diff1,
//             "Blake difficulty"
//         );
//     }

//     #[test]
//     fn test_timing_stats() {
//         let headers = vec![500, 350, 300, 210, 100u64]
//             .into_iter()
//             .map(|t| BlockHeader {
//                 timestamp: EpochTime::from(t),
//                 ..BlockHeader::default()
//             })
//             .collect::<Vec<BlockHeader>>();
//         let (max, min, avg) = BlockHeader::timing_stats(&headers);
//         assert_eq!(max, 150);
//         assert_eq!(min, 50);
//         assert_eq!(avg, 100f64);
//     }

//     #[test]
//     fn timing_negative_blocks() {
//         let headers = vec![150, 90, 100u64]
//             .into_iter()
//             .map(|t| BlockHeader {
//                 timestamp: EpochTime::from(t),
//                 ..BlockHeader::default()
//             })
//             .collect::<Vec<BlockHeader>>();
//         let (max, min, avg) = BlockHeader::timing_stats(&headers);
//         assert_eq!(max, 60);
//         assert_eq!(min, 0);
//         assert_eq!(avg, 25f64);
//     }

//     #[test]
//     fn timing_empty_list() {
//         let (max, min, avg) = BlockHeader::timing_stats(&[]);
//         assert_eq!(max, 0);
//         assert_eq!(min, std::u64::MAX);
//         assert_eq!(avg, 0f64);
//     }
// }
