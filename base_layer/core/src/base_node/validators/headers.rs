//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    blocks::{chain_header::ChainHeader, BlockHeader, BlockHeaderValidationError},
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    validation::{helpers, helpers::check_header_timestamp_greater_than_median, Validation, ValidationError},
};
use log::*;
use tari_crypto::tari_utilities::{hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::horizon_state_sync::headers";

pub struct HeaderValidator<B> {
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> HeaderValidator<B> {
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        Self { db }
    }
}

impl<B: BlockchainBackend> Validation<BlockHeader> for HeaderValidator<B> {
    fn validate(&self, header: &BlockHeader) -> Result<(), ValidationError> {
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        self.check_median_timestamp(header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Median timestamp is ok for {} ",
            &header_id
        );
        Ok(())
    }
}

impl<B: BlockchainBackend> ValidationConvert<BlockHeader, ChainHeader, B> for HeaderValidator<B> {
    fn validate_and_convert(&self, header: BlockHeader, _db: &B) -> Result<ChainHeader, ValidationError> {
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        trace!(
            target: LOG_TARGET,
            "Calculating and verifying target and achieved difficulty {} ",
            &header_id
        );
        let chain_header = self.check_achieved_and_target_difficulty(header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            &header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", &header_id
        );
        Ok(chain_header)
    }
}

impl<B: BlockchainBackend> HeaderValidator<B> {
    /// Calculates the achieved and target difficulties at the specified height and compares them.
    pub fn check_achieved_and_target_difficulty(
        &self,
        block_header: BlockHeader,
    ) -> Result<ChainHeader, ValidationError>
    {
        // We cant use the actual tip, as this is used by the header sync, the tip has not yet been downloaded, but we
        // can assume that the previous header was added, so we use that as the tip.
        let tip_height = block_header.height.saturating_sub(1);
        let db = self.db.db_read_access()?;
        let chain_header = check_achieved_and_target_difficulty(&*db, block_header, self.rules.clone())?;
        Ok(chain_header)
    }

    // /// Returns the set of target difficulties for the given `BlockHeader`
    // fn fetch_target_difficulties(
    //     &self,
    //     block_header: BlockHeader,
    // ) -> Result<Vec<(EpochTime, Difficulty)>, ValidationError>
    // {
    //     let block_window = self
    //         .rules
    //         .consensus_constants(block_header.height)
    //         .get_difficulty_block_window();
    //     let start_height = block_header.height.saturating_sub(block_window);
    //     if start_height == block_header.height {
    //         return Ok(vec![]);
    //     }

    //     trace!(
    //         target: LOG_TARGET,
    //         "fetch_target_difficulties: new header height = {}, block window = {}",
    //         block_header.height,
    //         block_window
    //     );

    //     let block_window = block_window as usize;
    //     // TODO: create custom iterator for chunks that does not require a large number of u64s to exist in memory
    //     let heights = (0..block_header.height).rev().collect::<Vec<_>>();
    //     let mut target_difficulties = Vec::with_capacity(block_window);
    //     for block_nums in heights.chunks(block_window) {
    //         let start = *block_nums.first().unwrap();
    //         let end = *block_nums.last().unwrap();
    //         let headers = self.db.fetch_headers(start, end)?;

    //         let max_remaining = block_window.saturating_sub(target_difficulties.len());
    //         trace!(
    //             target: LOG_TARGET,
    //             "fetch_target_difficulties: max_remaining = {}",
    //             max_remaining
    //         );
    //         target_difficulties.extend(
    //             headers
    //                 .into_iter()
    //                 .filter(|h| h.pow.pow_algo == block_header.pow.pow_algo)
    //                 .take(max_remaining)
    //                 .map(|h| (h.timestamp, h.pow.target_difficulty)),
    //         );

    //         assert!(
    //             target_difficulties.len() <= block_window,
    //             "target_difficulties can never contain more elements than the block window"
    //         );
    //         if target_difficulties.len() == block_window {
    //             break;
    //         }
    //     }

    //     trace!(
    //         target: LOG_TARGET,
    //         "fetch_target_difficulties: #returned = {}",
    //         target_difficulties.len()
    //     );
    //     Ok(target_difficulties.into_iter().rev().collect())
    // }

    /// This function tests that the block timestamp is greater than the median timestamp at the specified height.
    pub fn check_median_timestamp(&self, block_header: &BlockHeader) -> Result<(), ValidationError> {
        let timestamps = self.db.fetch_block_timestamps(block_header.hash())?;
        check_header_timestamp_greater_than_median(block_header, &timestamps)
    }
}
