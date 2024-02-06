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

use crate::transactions::tari_amount::MicroMinotari;

pub trait Emission {
    fn block_reward(&self, height: u64) -> MicroMinotari;
    fn supply_at_block(&self, height: u64) -> MicroMinotari;
}

/// The Minotari emission schedule with constant tail emission. The emission schedule determines how much Minotari is
/// mined as a block reward at every block.
#[derive(Debug, Clone)]
pub struct EmissionSchedule {
    initial: MicroMinotari,
    decay: &'static [u64],
    #[cfg(not(tari_feature_mainnet_emission))]
    tail: MicroMinotari,
    #[cfg(tari_feature_mainnet_emission)]
    inflation_bips: u64,
    #[cfg(tari_feature_mainnet_emission)]
    epoch_length: u64, // The number of blocks in an inflation epoch
    #[cfg(tari_feature_mainnet_emission)]
    initial_supply: MicroMinotari, // The supply at block 0, from faucets or premine
}

impl EmissionSchedule {
    /// Create a new emission schedule instance.
    ///
    /// ## Primary emission schedule
    ///
    /// The Emission schedule follows a similar pattern to Monero; with an initially exponentially decaying emission
    /// rate and a tail emission.
    ///
    ///
    /// The decay portion is given by
    ///  $$ r_n = \lfloor r_{n-1} * (1 - \epsilon) \rfloor, n > 0 $$
    ///  $$ r_0 = A_0 $$
    ///
    /// where
    ///  * $$A_0$$ is the genesis block reward
    ///  * $$1 - \epsilon$$ is the decay rate
    ///
    /// The decay parameters are determined as described in [#decay_parameters].
    ///
    ///  ## Tail emission
    ///
    ///  If the feature `mainnet_emission` is not enabled, the tail emission is constant. It is triggered if the reward
    ///  would fall below the `tail` value.
    ///
    /// If the feature `mainnet_emission` is enabled, the tail emission is calculated as follows:
    ///
    ///  At each block, the reward is multiplied by `EPOCH_LENGTH` (approximately a year's worth of blocks) to
    /// calculate `annual_supply`.
    ///  If `annual_supply/current_supply` is less than `0.01*inflation_bips`% then we enter tail emission mode.
    ///
    ///  Every `EPOCH_LENGTH` blocks, the inflation rate is recalculated based on the current supply.
    ///
    /// ## Decay parameters
    ///
    /// The `intfloor` function is an integer-math-based multiplication of an integer by a fraction that's very close
    /// to one (e.g. 0.998,987,123,432)` that
    ///  1. provides the same result regardless of the CPU architecture (e.g. x86, ARM, etc.)
    ///  2. Has minimal rounding error given the very high precision of the decay factor.
    ///
    /// Firstly, the decay factor is represented in an array of its binary coefficients. In the same way that 65.4 in
    /// decimal can be represented as `6 x 10 + 5 x 1 + 4 x 0.1`, we can write 0.75 in binary as `2^(-1) + 2^(-2)`.
    /// The decay function is always less than one, so we dispense with signs and just represent the array as the set
    /// of negative powers of 2 that most closely represent the decay factor.
    ///
    /// We can then apply a very fast multiplication using bitwise operations. If the decay factor, ϵ, is represented
    /// in the array `**k**` then
    /// ```ignore
    /// intfloor(x, (1 - ϵ)) = x - sum(x >> k_i)
    /// ```
    ///
    /// Now, why use (1 - ϵ), and not the decay rate, `f` directly?
    ///
    /// The reason is to reduce rounding error. Every SHR operation is a "round down" operation. E.g. `7 >> 2` is 1,
    /// whereas 7 / 4 = 1.75. So, we lose 0.75 there due to rounding. In general, the maximum error due to rounding
    /// when doing integer division, `a / b` is `a % b`, which has a maximum of `b-1`. In binary terms, the maximum
    /// error of the operation ` a >> b` is `2^-(b+1)`.
    ///
    /// Now compare the operation `x.f` where `f ~ 1` vs. `x.(1 - ϵ) = x - x.ϵ`, where `ϵ ~ 0`.
    /// In both cases, the maximum error is $$ \sum_i 2^{k_i} = 1 - 2^{-(n+1)} $$
    ///
    /// Since `f` is close to one, `k` is something like 0.9989013671875, or `[1,2,3,4,5,6,7,8,9,11,12,13]`, with a
    /// maximum error of 0.49945 μT per block. Too high.
    ///
    /// However, using the ϵ representation (1 - `f`) is `[10,14,15,...,64]`, which has a maximum error of
    /// 0.0005493 μT per block, which is more than accurate enough for our purposes (1 μT difference over 2,000
    /// blocks).
    ///
    /// **Note:** The word "error" has been used here, since this is technically what it is compared to an infinite
    /// precision floating point operation. However, to be clear, the results given by `intfloor` are, by
    /// **definition**, the correct and official emission values.
    ///
    /// ## Panics
    ///
    /// The shift right operation will overflow if shifting more than 63 bits. `new` will panic if any of the decay
    /// values are greater than or equal to 64.
    pub fn new(
        initial: MicroMinotari,
        decay: &'static [u64],
        #[cfg(not(tari_feature_mainnet_emission))] tail: MicroMinotari,
        #[cfg(tari_feature_mainnet_emission)] inflation_bips: u64,
        #[cfg(tari_feature_mainnet_emission)] epoch_length: u64,
        #[cfg(tari_feature_mainnet_emission)] initial_supply: MicroMinotari,
    ) -> EmissionSchedule {
        assert!(
            decay.iter().all(|i| *i < 64),
            "Decay value would overflow. All `decay` values must be less than 64"
        );
        #[cfg(not(tari_feature_mainnet_emission))]
        let params = EmissionSchedule { initial, decay, tail };
        #[cfg(tari_feature_mainnet_emission)]
        let params = EmissionSchedule {
            initial,
            decay,
            inflation_bips,
            epoch_length,
            initial_supply,
        };
        params
    }

    /// Utility function to calculate the decay parameters that are provided in [EmissionSchedule::new]. This function
    /// is provided as a convenience and for the record, but is kept as a separate step. For performance reasons the
    /// parameters are 'hard-coded' as a static array rather than a heap allocation.
    ///
    /// See [`EmissionSchedule::new`] for more details on how the parameters are derived.
    ///
    /// Input : `k`: A string representing a floating point number of (nearly) arbitrary precision, and less than one.
    ///
    /// Returns: An array of powers of negative two when applied as a shift right and sum operation is very
    /// close to (1-k)*n.
    ///
    /// None - If k is not a valid floating point number less than one.
    pub fn decay_params(k: &str) -> Option<Vec<u64>> {
        // Convert string into a vector of digits. e.g. 0.9635 -> [9,6,3,5]
        fn frac_vec(n: &str) -> Option<Vec<u8>> {
            if !n.starts_with("0.") {
                return None;
            }
            if !n.chars().skip(2).all(|i| i.is_ascii_digit()) {
                return None;
            }
            let arr = n.chars().skip(2).map(|i| i as u8 - 48).collect::<Vec<u8>>();
            Some(arr)
        }
        // Multiply a vector of decimal fractional digits by 2. The bool indicates whether the result was greater than
        // one
        fn times_two(num: &mut [u8]) -> bool {
            let len = num.len();
            let mut carry_last = 0u8;
            for i in 0..len {
                let index = len - 1 - i;
                let carry = (num[index] >= 5).into();
                num[index] = (2 * num[index]) % 10 + carry_last;
                carry_last = carry;
            }
            carry_last > 0
        }

        fn is_zero(v: &[u8]) -> bool {
            v.iter().all(|i| *i == 0u8)
        }

        let mut next = frac_vec(k)?;
        let mut result = Vec::with_capacity(32);
        let mut index = 1u8;
        let mut exact = true;
        while !is_zero(&next) {
            let overflow = times_two(&mut next);
            if !overflow {
                result.push(index);
            }
            if index >= 63 {
                exact = false;
                break;
            }
            index += 1;
        }
        if exact {
            result.push(index - 1);
        }
        let result = result.into_iter().map(u64::from).collect();
        Some(result)
    }

    /// Return an iterator over the block reward and total supply. This is the most efficient way to iterate through
    /// the emission curve if you're interested in the supply as well as the reward.
    ///
    /// This is an infinite iterator, and each value returned is a tuple of (block number, reward, and total supply)
    pub fn iter(&self) -> EmissionRate {
        EmissionRate::new(self)
    }

    fn inner_schedule(&self, height: u64) -> EmissionRate {
        let mut iterator = self.iter();
        while iterator.block_height() < height {
            iterator.next();
        }
        iterator
    }
}

pub struct EmissionRate<'a> {
    block_num: u64,
    supply: MicroMinotari,
    reward: MicroMinotari,
    schedule: &'a EmissionSchedule,
    #[cfg(tari_feature_mainnet_emission)]
    epoch: u64,
    #[cfg(tari_feature_mainnet_emission)]
    epoch_counter: u64,
}

impl<'a> EmissionRate<'a> {
    fn new(schedule: &'a EmissionSchedule) -> EmissionRate<'a> {
        EmissionRate {
            block_num: 0,
            #[cfg(not(tari_feature_mainnet_emission))]
            supply: MicroMinotari(0),
            #[cfg(tari_feature_mainnet_emission)]
            supply: schedule.initial_supply,
            reward: MicroMinotari(0),
            schedule,
            #[cfg(tari_feature_mainnet_emission)]
            epoch: 0,
            #[cfg(tari_feature_mainnet_emission)]
            epoch_counter: 0,
        }
    }

    pub fn supply(&self) -> MicroMinotari {
        self.supply
    }

    pub fn block_height(&self) -> u64 {
        self.block_num
    }

    pub fn block_reward(&self) -> MicroMinotari {
        self.reward
    }

    fn next_decay_reward(&self) -> MicroMinotari {
        let r = self.reward.as_u64();
        self.schedule
            .decay
            .iter()
            .fold(self.reward, |sum, i| sum - MicroMinotari::from(r >> *i))
    }

    /// Calculates the next reward by multiplying the decay factor by the previous block reward using integer math.
    ///
    /// We write the decay factor, 1 - k, as a sum of fraction powers of two. e.g. if we wanted 0.25 as our k, then
    /// (1-k) would be 0.75 = 1/2 plus 1/4 (1/2^2).
    ///
    /// Then we calculate k.R = (1 - e).R = R - e.R = R - (0.5 * R + 0.25 * R) = R - R >> 1 - R >> 2
    #[cfg(not(tari_feature_mainnet_emission))]
    fn next_reward(&mut self) {
        let reward = std::cmp::max(self.next_decay_reward(), self.schedule.tail);
        self.reward = reward;
    }

    #[cfg(tari_feature_mainnet_emission)]
    fn new_tail_emission(&self) -> MicroMinotari {
        let epoch_issuance = (self.supply.as_u64() / 100).saturating_mul(self.schedule.inflation_bips);
        let reward = epoch_issuance / self.schedule.epoch_length; // in uT
        MicroMinotari::from((reward / 1_000_000) * 1_000_000) // truncate to nearest whole XTR
    }

    #[cfg(tari_feature_mainnet_emission)]
    fn next_reward(&mut self) {
        // Inflation phase
        if self.epoch > 0 {
            self.epoch_counter += 1;
            if self.epoch_counter >= self.schedule.epoch_length {
                self.epoch_counter = 0;
                self.epoch += 1;
                self.reward = self.new_tail_emission();
            }
        } else {
            // Decay phase
            let cutoff = self.new_tail_emission();
            let next_decay_reward = self.next_decay_reward();
            if self.epoch == 0 && next_decay_reward > cutoff {
                self.reward = next_decay_reward;
            } else {
                self.epoch = 1;
                self.reward = cutoff;
            }
        }
    }
}

impl Iterator for EmissionRate<'_> {
    type Item = (u64, MicroMinotari, MicroMinotari);

    fn next(&mut self) -> Option<Self::Item> {
        self.block_num += 1;
        if self.block_num == 1 {
            self.reward = self.schedule.initial;
            self.supply = self.supply.checked_add(self.reward)?;
            return Some((self.block_num, self.reward, self.supply));
        }
        self.next_reward(); // Has side effect
                            // Once we've reached max supply, the iterator is done
        self.supply = self.supply.checked_add(self.reward)?;
        Some((self.block_num, self.reward, self.supply))
    }
}

impl Emission for EmissionSchedule {
    /// Calculate the block reward for the given block height, in µMinotari
    fn block_reward(&self, height: u64) -> MicroMinotari {
        let iterator = self.inner_schedule(height);
        iterator.block_reward()
    }

    /// Calculate the exact emitted supply after the given block, in µMinotari. The value is calculated by summing up
    /// the block reward for each block, making this a very inefficient function if you wanted to call it from a
    /// loop for example. For those cases, use the `iter` function instead.
    fn supply_at_block(&self, height: u64) -> MicroMinotari {
        let iterator = self.inner_schedule(height);
        iterator.supply()
    }
}

#[cfg(test)]
mod test {
    use crate::consensus::emission::EmissionSchedule;

    #[test]
    fn calc_array() {
        assert_eq!(EmissionSchedule::decay_params("1.00"), None);
        assert_eq!(EmissionSchedule::decay_params("56345"), None);
        assert_eq!(EmissionSchedule::decay_params("0.75").unwrap(), vec![2]);
        assert_eq!(EmissionSchedule::decay_params("0.25").unwrap(), vec![1, 2]);
        assert_eq!(EmissionSchedule::decay_params("0.5").unwrap(), vec![1]);
        assert_eq!(EmissionSchedule::decay_params("0.875").unwrap(), vec![3]);
        assert_eq!(EmissionSchedule::decay_params("0.125").unwrap(), vec![1, 2, 3]);
        assert_eq!(EmissionSchedule::decay_params("0.64732").unwrap(), vec![
            2, 4, 5, 7, 10, 13, 16, 19, 20, 21, 22, 25, 29, 32, 33, 34, 35, 36, 38, 45, 47, 51, 53, 58, 59, 60, 62, 63
        ]);
        assert_eq!(EmissionSchedule::decay_params("0.9999991208182701").unwrap(), vec![
            21, 22, 23, 25, 26, 37, 38, 39, 41, 45, 49, 50, 51, 52, 55, 57, 59, 60, 63
        ]);
        assert_eq!(EmissionSchedule::decay_params("0.0").unwrap(), vec![0]);
    }

    #[cfg(tari_feature_mainnet_emission)]
    mod inflating_tail {
        use crate::{
            consensus::emission::EmissionSchedule,
            transactions::tari_amount::{MicroMinotari, T},
        };

        #[test]
        #[allow(clippy::cast_possible_truncation)]
        fn mainnet_emission() {
            let epoch_length = 30 * 24 * 366;
            let halflife = 3 * 30 * 24 * 365;
            let a0 = MicroMinotari::from(12_923_971_428);
            let decay = &[21u64, 22, 23, 25, 26, 37, 38, 40];
            let premine = 6_300_000_000 * T;
            let schedule = EmissionSchedule::new(a0, decay, 1, epoch_length, premine);
            let mut iter = schedule.iter();
            assert_eq!(iter.block_num, 0);
            assert_eq!(iter.reward, MicroMinotari::from(0));
            assert_eq!(iter.supply, premine);
            let (num, reward, supply) = iter.next().unwrap();
            // Block 1
            assert_eq!(num, 1);
            assert_eq!(reward, MicroMinotari::from(12_923_971_428));
            assert_eq!(supply, MicroMinotari::from(6_300_012_923_971_428));
            // Block 2
            let (num, reward, supply) = iter.next().unwrap();
            assert_eq!(num, 2);
            assert_eq!(reward, MicroMinotari::from(12_923_960_068));
            assert_eq!(supply, MicroMinotari::from(6_300_025_847_931_496));

            // Block 788,400. 50% Mined
            let mut iter = iter.skip_while(|(num, _, _)| *num < halflife);
            let (num, reward, supply) = iter.next().unwrap();
            assert_eq!(num, halflife);
            assert_eq!(reward.as_u64(), 6_463_480_936);
            let total_supply = 21_000_000_000 * T - premine;
            let residual = (supply - premine) * 2 - total_supply;
            // Within 0.01% of mining half the total supply
            assert!(residual < total_supply / 10000, "Residual: {}", residual);
            // Head to tail emission
            let mut iter = iter.skip_while(|(num, _, _)| *num < 3_220_980);
            let (num, reward, supply) = iter.next().unwrap();
            assert_eq!(num, 3_220_980);
            assert_eq!(reward, MicroMinotari::from(764_000_449));
            assert_eq!(supply, MicroMinotari::from(20_140_382_328_948_420));
            let (num, reward, _) = iter.next().unwrap();
            assert_eq!(num, 3_220_981);
            assert_eq!(reward, 764 * T);
            let (num, reward, _) = iter.next().unwrap();
            assert_eq!(num, 3_220_982);
            assert_eq!(reward, 764 * T);
            // Next boosting
            let mut iter = iter.skip((epoch_length - 3) as usize);
            let (num, reward, supply) = iter.next().unwrap();
            assert_eq!(num, 3_484_500);
            assert_eq!(reward, 764 * T);
            assert_eq!(supply, MicroMinotari::from(20_341_711_608_948_420));
            let (num, reward, _) = iter.next().unwrap();
            assert_eq!(num, 3_484_501);
            assert_eq!(reward, 771 * T);
            let (num, reward, supply) = iter.next().unwrap();
            assert_eq!(num, 3_484_502);
            assert_eq!(reward, 771 * T);
            // Check supply inflation. Because of rounding, it could be between 98 and 100 bips
            let epoch_supply = 771 * T * epoch_length;
            let inflation = (10000 * epoch_supply / supply).as_u64(); // 1 bip => 100
            assert!(inflation < 100 && inflation > 98, "Inflation: {} bips", inflation);
        }
    }

    #[cfg(not(tari_feature_mainnet_emission))]
    mod constant_tail {
        use std::convert::TryFrom;

        use crate::{
            consensus::{
                emission::{Emission, EmissionSchedule},
                ConsensusConstants,
            },
            transactions::tari_amount::{uT, MicroMinotari, T},
        };

        #[test]
        fn schedule() {
            let schedule = EmissionSchedule::new(
                MicroMinotari::from(10_000_100),
                &[22, 23, 24, 26, 27],
                MicroMinotari::from(100),
            );
            assert_eq!(schedule.block_reward(0), MicroMinotari::from(0));
            assert_eq!(schedule.supply_at_block(0), MicroMinotari::from(0));
            assert_eq!(schedule.block_reward(1), MicroMinotari::from(10_000_100));
            assert_eq!(schedule.supply_at_block(1), MicroMinotari::from(10_000_100));
            // These values have been independently calculated
            assert_eq!(schedule.block_reward(100 + 1), MicroMinotari::from(9_999_800));
            assert_eq!(schedule.supply_at_block(100 + 1), MicroMinotari::from(1_009_994_950));
        }

        #[test]
        fn huge_block_number() {
            // let mut n = (std::i32::MAX - 1) as u64;
            let height = 262_800_000; // 1000 years' problem
            let schedule = EmissionSchedule::new(
                MicroMinotari::from(10000000u64),
                &[22, 23, 24, 26, 27],
                MicroMinotari::from(100),
            );
            // Slow but does not overflow
            assert_eq!(schedule.block_reward(height + 1), MicroMinotari::from(4_194_303));
        }

        #[test]
        fn generate_emission_schedule_as_iterator() {
            const INITIAL: u64 = 10_000_100;
            let schedule = EmissionSchedule::new(
                MicroMinotari::from(INITIAL),
                &[2], // 0.25 decay
                MicroMinotari::from(100),
            );
            assert_eq!(schedule.block_reward(0), MicroMinotari(0));
            assert_eq!(schedule.supply_at_block(0), MicroMinotari(0));
            let values = schedule.iter().take(101).collect::<Vec<_>>();
            let (height, reward, supply) = values[0];
            assert_eq!(height, 1);
            assert_eq!(reward, MicroMinotari::from(INITIAL));
            assert_eq!(supply, MicroMinotari::from(INITIAL));
            let (height, reward, supply) = values[1];
            assert_eq!(height, 2);
            assert_eq!(reward, MicroMinotari::from(7_500_075));
            assert_eq!(supply, MicroMinotari::from(17_500_175));
            let (height, reward, supply) = values[2];
            assert_eq!(height, 3);
            assert_eq!(reward, MicroMinotari::from(5_625_057));
            assert_eq!(supply, MicroMinotari::from(23_125_232));
            let (height, reward, supply) = values[10];
            assert_eq!(height, 11);
            assert_eq!(reward, MicroMinotari::from(563_142));
            assert_eq!(supply, MicroMinotari::from(38_310_986));
            let (height, reward, supply) = values[41];
            assert_eq!(height, 42);
            assert_eq!(reward, MicroMinotari::from(100));
            assert_eq!(supply, MicroMinotari::from(40_000_252));

            let mut tot_supply = MicroMinotari::from(0);
            for (_, reward, supply) in schedule.iter().take(1000) {
                tot_supply += reward;
                assert_eq!(tot_supply, supply);
            }
        }

        #[test]
        #[allow(clippy::identity_op)]
        fn emission() {
            use crate::transactions::tari_amount::uT;
            let emission = EmissionSchedule::new(1 * T, &[1, 2], 100 * uT);
            let mut emission = emission.iter();
            // decay is 1 - 0.25 - 0.125 = 0.625
            assert_eq!(emission.block_height(), 0);
            assert_eq!(emission.block_reward(), MicroMinotari(0));
            assert_eq!(emission.supply(), MicroMinotari(0));

            assert_eq!(emission.next(), Some((1, 1_000_000 * uT, 1_000_000 * uT)));
            assert_eq!(emission.next(), Some((2, 250_000 * uT, 1_250_000 * uT)));
            assert_eq!(emission.next(), Some((3, 62_500 * uT, 1_312_500 * uT)));
            assert_eq!(emission.next(), Some((4, 15_625 * uT, 1_328_125 * uT)));
            assert_eq!(emission.next(), Some((5, 3_907 * uT, 1_332_032 * uT)));
            assert_eq!(emission.next(), Some((6, 978 * uT, 1_333_010 * uT)));
            assert_eq!(emission.next(), Some((7, 245 * uT, 1_333_255 * uT)));
            // Tail emission kicks in
            assert_eq!(emission.next(), Some((8, 100 * uT, 1_333_355 * uT)));
            assert_eq!(emission.next(), Some((9, 100 * uT, 1_333_455 * uT)));

            assert_eq!(emission.block_height(), 9);
            assert_eq!(emission.block_reward(), 100 * uT);
            assert_eq!(emission.supply(), 1333455 * uT);
            let schedule = EmissionSchedule::new(1 * T, &[1, 2], 100 * uT);
            assert_eq!(emission.block_reward(), schedule.block_reward(9));
            assert_eq!(emission.supply(), schedule.supply_at_block(9))
        }

        #[test]
        fn stagenet_schedule() {
            let stagenet = ConsensusConstants::stagenet();
            let schedule = EmissionSchedule::new(
                stagenet[0].emission_initial,
                stagenet[0].emission_decay,
                stagenet[0].emission_tail,
            );
            // No genesis block coinbase
            assert_eq!(schedule.block_reward(0), MicroMinotari(0));
            // Coinbases starts at block 1
            let coinbase_offset = 1;
            let first_reward = schedule.block_reward(coinbase_offset);
            assert_eq!(first_reward, stagenet[0].emission_initial * uT);
            assert_eq!(schedule.supply_at_block(coinbase_offset), first_reward);
            // 'half_life_block' at approximately '(total supply - faucet value) / 2'
            #[allow(clippy::cast_possible_truncation)]
            let half_life_block = (365.0 * 24.0 * 30.0 * 2.76) as u64;
            assert_eq!(
                schedule.supply_at_block(half_life_block + coinbase_offset),
                7_483_280_506_356_578 * uT
            );
            // Tail emission starts after block 3,255,552 + coinbase_offset
            let mut rewards = schedule
                .iter()
                .skip(3255552 + usize::try_from(coinbase_offset).unwrap());
            let (block_num, reward, supply) = rewards.next().unwrap();
            assert_eq!(block_num, 3255553 + coinbase_offset);
            assert_eq!(reward, 800_000_415 * uT);
            let total_supply_up_to_tail_emission = supply + stagenet[0].faucet_value();
            assert_eq!(total_supply_up_to_tail_emission, 20_999_999_999_819_869 * uT);
            let (_, reward, _) = rewards.next().unwrap();
            assert_eq!(reward, stagenet[0].emission_tail);
        }
    }
}
