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

use std::convert::TryFrom;

use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_core::blocks::genesis_block::get_genesis_block;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Birthday {
    zero_point_time: u64,
    birthday: u16,
    version: u8,
}

impl Birthday {
    pub fn new() -> Self {
        let network = Network::Dibbler;

        Self::new_from_network(network)
    }

    pub fn new_from_network(network: Network) -> Self {
        let current_time = Self::current_time_in_seconds();
        Self::new_from_network_and_current_time(network, current_time)
    }

    fn new_from_network_and_current_time(network: Network, current_time: u64) -> Self {
        const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
        const PERIOD_LENGTH: u64 = u16::MAX as u64 + 1; // 2^16

        let mut zero_point_time = Self::get_network_genesis_time(network);

        let days = (current_time - zero_point_time) / SECONDS_PER_DAY;
        let birthday = u16::try_from(days % PERIOD_LENGTH).unwrap();
        let version = u8::try_from(days / PERIOD_LENGTH).unwrap();

        zero_point_time += PERIOD_LENGTH * u64::from(version);

        Self {
            birthday,
            version,
            zero_point_time,
        }
    }

    pub fn birthday(&self) -> u16 {
        self.birthday
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn zero_point_time(&self) -> u64 {
        self.zero_point_time
    }

    pub fn current_time_in_seconds() -> u64 {
        u64::try_from(chrono::Utc::now().timestamp()).unwrap()
    }

    pub fn get_network_genesis_time(network: Network) -> u64 {
        get_genesis_block(network).block().header.timestamp.as_u64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

    #[test]
    fn correct_version() {
        let network = Network::Dibbler;
        let birthday = Birthday::new_from_network(network);
        assert_eq!(birthday.version(), 0u8);
    }

    #[test]
    fn correct_zero_point_time() {
        let network = Network::Dibbler;
        let dibbler_genesis_block_time = get_genesis_block(network).block().header.timestamp.as_u64();
        let birthday = Birthday::new_from_network(network);
        assert_eq!(birthday.zero_point_time(), dibbler_genesis_block_time); // admit at most 5 seconds difference
    }

    #[test]
    fn birthday_is_correctly_computed() {
        let network = Network::Dibbler;

        let dibbler_genesis_block_time = Birthday::get_network_genesis_time(network);

        let now = u64::try_from(chrono::Utc::now().timestamp()).unwrap();
        let current = (now - dibbler_genesis_block_time) / (24 * 60 * 60);

        let birthday = Birthday::new_from_network(network).birthday();
        let suite_birthday = u16::try_from(current % (2u64.pow(16))).unwrap();

        assert_eq!(suite_birthday, birthday);
    }

    #[test]
    fn works_after_successful_versions() {
        let genesis_timestamp = Birthday::get_network_genesis_time(Network::Dibbler);

        for vrsn in 1..10u64 {
            let lapse_period = vrsn * (u64::from(u16::MAX) + 1) + vrsn;
            let current_time = genesis_timestamp + lapse_period * SECONDS_PER_DAY;
            let birthday_data = Birthday::new_from_network_and_current_time(Network::Dibbler, current_time);

            assert_eq!(birthday_data.version, u8::try_from(vrsn).unwrap());
            assert_eq!(birthday_data.birthday, u16::try_from(vrsn).unwrap());
            assert_eq!(
                birthday_data.zero_point_time,
                genesis_timestamp + vrsn * (u64::from(u16::MAX) + 1)
            );
        }
    }
}
