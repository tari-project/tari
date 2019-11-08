// Copyright 2019. The Tari Project
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

use crate::emission::EmissionSchedule;
use chrono::{DateTime, Duration, Utc};
use std::ops::Add;

// This is the our target time in seconds between blocks
pub const TARGET_BLOCK_INTERVAL: u64 = 60;
// When doing difficulty adjustments and FTL calculations this is the amount of blocks we look at
pub const DIFFICULTY_BLOCK_WINDOW: u64 = 150;

/// This is used to control all consensus values.
pub struct ConsensusRules {
    /// The min height maturity a coinbase utxo must have
    coinbase_lock_height: u64,
    /// The emission schedule to use for coinbase rewards
    emission_schedule: EmissionSchedule,
    /// Current version of the blockchain
    blockchain_version: u16,
    /// The Future Time Limit (FTL) of the blockchain in seconds. This is the max allowable timestamp that is excepted.
    /// We use TxN/20 where T = target time = 60 seconds, and N = block_window = 150
    future_time_limit: u64,
}

impl ConsensusRules {
    pub fn current() -> Self {
        //        CONSENSUS_RULES
        ConsensusRules {
            coinbase_lock_height: 1,
            emission_schedule: EmissionSchedule::new(10_000_000.into(), 0.999, 100.into()),
            blockchain_version: 1,
            future_time_limit: TARGET_BLOCK_INTERVAL * DIFFICULTY_BLOCK_WINDOW / 20,
        }
    }

    /// The min height maturity a coinbase utxo must have
    pub fn coinbase_lock_height(&self) -> u64 {
        self.coinbase_lock_height
    }

    /// Current version of the blockchain
    pub fn blockchain_version(&self) -> u16 {
        self.blockchain_version
    }

    /// The emission schedule to use for coinbase rewards
    pub fn emission_schedule(&self) -> &EmissionSchedule {
        &self.emission_schedule
    }

    /// This returns the FTL(Future Time Limit) for blocks
    /// Any block with a timestamp greater than this is rejected.
    pub fn ftl(&self) -> DateTime<Utc> {
        Utc::now().add(Duration::seconds(self.future_time_limit as i64))
    }
}
