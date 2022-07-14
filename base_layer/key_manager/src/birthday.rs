// Copyright 2021. The Tari Project
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

use std::{
    convert::TryFrom,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;

/// Implementation of a [`Birthday`] type. The goal of the current logic is to define a birthday date dependent on
/// a fixed genesis time. There are two subfields, `birthday` and `version`. Whereas `birthday` keeps track of the
/// numbers of days between the time of runtime instantiation from genesis time, `version` tracks an epoch counter.
/// The idea behind adding a versioning to the logic permits to extend the birthday definition beyond the u16::MAX.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Birthday {
    birthday: u16,
    version: u8,
}

impl Birthday {
    pub fn new(network: Network) -> Self {
        let current_time = Self::current_time_in_seconds();
        Self::new_from_current_time(network, current_time)
    }
    use tari_core::blocks::genesis_block::get_genesis_block;

    fn new_from_current_time(network: Network, current_time: u64) -> Self {
        const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
        const EPOCH_LENGTH: u64 = u16::MAX as u64 + 1; // 2^16

        let genesis_time = Self::get_genesis_time(network);

        let days = (Instant::now() - genesis_time).as_secs() / SECONDS_PER_DAY;
        let birthday = (days % EPOCH_LENGTH) as u16;
        let version = u8::try_from(days / EPOCH_LENGTH).unwrap();

        Self {
            birthday,
            version,
        }
    }

    pub fn birthday(&self) -> u16 {
        self.birthday
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn zero_point_time(&self) -> u64 {
        self.get_genesis_time() + Duration::seconds(EPOCH_LENGTH * (version as u64) * SECONDS_PER_DAY)
    }

    pub fn current_time_in_seconds() -> u64 {
        u64::try_from(chrono::Utc::now().timestamp()).unwrap()
    }

    pub fn get_genesis_time(network: Network) -> Instant {
        Instant::at(network.block().header.timestamp.as_u64())
    }
}

impl Default for Birthday {
    fn default() -> Self {
        let network = Network::Dibbler;
        Self::new(network)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

    #[test]
    fn correct_version() {
        let network = Network::Dibbler;
        let birthday = Birthday::new(network);
        assert_eq!(birthday.version(), 0u8);
    }

    #[test]
    fn correct_zero_point_time() {
        let network = Network::Dibbler;
        let dibbler_genesis_block_time = get_genesis_block(network).block().header.timestamp.as_u64();
        let birthday = Birthday::new(network);
        assert_eq!(birthday.zero_point_time(), dibbler_genesis_block_time); // admit at most 5 seconds difference
    }

    #[test]
    fn birthday_is_correctly_computed() {
        let network = Network::Dibbler;

        let dibbler_genesis_block_time = Birthday::get_genesis_time(network);

        let now = u64::try_from(chrono::Utc::now().timestamp()).unwrap();
        let current = (now - dibbler_genesis_block_time) / (24 * 60 * 60);

        let birthday = Birthday::new(network).birthday();
        let suite_birthday = u16::try_from(current % (2u64.pow(16))).unwrap();

        assert_eq!(suite_birthday, birthday);
    }

    #[test]
    fn works_after_successful_versions() {
        let genesis_timestamp = Birthday::get_genesis_time(Network::Dibbler);

        for vrsn in 1..10u64 {
            let lapse_period = vrsn * (u64::from(u16::MAX) + 1) + vrsn;
            let current_time = genesis_timestamp + lapse_period * SECONDS_PER_DAY;
            let birthday_data = Birthday::new_from_current_time(Network::Dibbler, current_time);

            assert_eq!(birthday_data.version, u8::try_from(vrsn).unwrap());
            assert_eq!(birthday_data.birthday, u16::try_from(vrsn).unwrap());
            assert_eq!(
                birthday_data.zero_point_time,
                genesis_timestamp + vrsn * (u64::from(u16::MAX) + 1)
            );
        }
    }
}
