// Copyright 2019. The Taiji Project
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

use std::cmp;

use crate::transactions::taiji_amount::MicroMinotaiji;

pub trait Emission {
    fn block_reward(&self, height: u64) -> MicroMinotaiji;
    fn supply_at_block(&self, height: u64) -> MicroMinotaiji;
}

/// The Minotaiji emission schedule. The emission schedule determines how much Minotaiji is mined as a block reward at
/// every block.
///
/// NB: We don't know what the final emission schedule will be on Minotaiji yet, so do not give any weight to values or
/// formulae provided in this file, they will almost certainly change ahead of main-net release.
#[derive(Debug, Clone)]
pub struct EmissionSchedule {
    initial: MicroMinotaiji,
    decay: &'static [u64],
    tail: MicroMinotaiji,
}

impl EmissionSchedule {
    /// Create a new emission schedule instance.
    ///
    /// The Emission schedule follows a similar pattern to Monero; with an exponentially decaying emission rate with
    /// a constant tail emission rate.
    ///
    /// The block reward is given by
    ///  $$ r_n = \mathrm{MAX}(\mathrm(intfloor(r_{n-1} * (1 - \epsilon)), t) n > 0 $$
    ///  $$ r_0 = A_0 $$
    ///
    /// where
    ///  * $$A_0$$ is the genesis block reward
    ///  * $$1 - \epsilon$$ is the decay rate
    ///  * $$t$$ is the constant tail emission rate
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
    pub fn new(initial: MicroMinotaiji, decay: &'static [u64], tail: MicroMinotaiji) -> EmissionSchedule {
        assert!(
            decay.iter().all(|i| *i < 64),
            "Decay value would overflow. All `decay` values must be less than 64"
        );
        EmissionSchedule { initial, decay, tail }
    }

    /// Utility function to calculate the decay parameters that are provided in [EmissionSchedule::new]. This function
    /// is provided as a convenience and for the record, but is kept as a separate step. For performance reasons the
    /// parameters are 'hard-coded' as a static array rather than a heap allocation.
    ///
    /// See [`EmissionSchedule::new`] for more details on how the parameters are derived.
    ///
    /// Input : `k`: A string representing a floating point number of (nearly) arbitrary precision, and less than one.
    ///
    /// Returns: An array of powers of negative two when when applied as a shift right and sum operation is very
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
    ///
    /// ```edition2018
    /// use taiji_core::{
    ///     consensus::emission::EmissionSchedule,
    ///     transactions::taiji_amount::MicroMinotaiji,
    /// };
    /// // Print the reward and supply for first 100 blocks
    /// let schedule = EmissionSchedule::new(10.into(), &[3], 1.into());
    /// for (n, reward, supply) in schedule.iter().take(100) {
    ///     println!("{:3} {:9} {:9}", n, reward, supply);
    /// }
    /// ```
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
    supply: MicroMinotaiji,
    reward: MicroMinotaiji,
    schedule: &'a EmissionSchedule,
}

impl<'a> EmissionRate<'a> {
    fn new(schedule: &'a EmissionSchedule) -> EmissionRate<'a> {
        EmissionRate {
            block_num: 0,
            supply: MicroMinotaiji(0),
            reward: MicroMinotaiji(0),
            schedule,
        }
    }

    pub fn supply(&self) -> MicroMinotaiji {
        self.supply
    }

    pub fn block_height(&self) -> u64 {
        self.block_num
    }

    pub fn block_reward(&self) -> MicroMinotaiji {
        self.reward
    }

    /// Calculates the next reward by multiplying the decay factor by the previous block reward using integer math.
    ///
    /// We write the decay factor, 1 - k, as a sum of fraction powers of two. e.g. if we wanted 0.25 as our k, then
    /// (1-k) would be 0.75 = 1/2 plus 1/4 (1/2^2).
    ///
    /// Then we calculate k.R = (1 - e).R = R - e.R = R - (0.5 * R + 0.25 * R) = R - R >> 1 - R >> 2
    fn next_reward(&self) -> MicroMinotaiji {
        let r = self.reward.as_u64();
        let next = self
            .schedule
            .decay
            .iter()
            .fold(self.reward, |sum, i| sum - MicroMinotaiji::from(r >> *i));

        cmp::max(next, self.schedule.tail)
    }
}

impl Iterator for EmissionRate<'_> {
    type Item = (u64, MicroMinotaiji, MicroMinotaiji);

    fn next(&mut self) -> Option<Self::Item> {
        self.block_num += 1;
        if self.block_num == 1 {
            self.reward = self.schedule.initial;
            self.supply = self.schedule.initial;
            return Some((self.block_num, self.reward, self.supply));
        }
        self.reward = self.next_reward();
        // Once we've reached max supply, the iterator is done
        self.supply = self.supply.checked_add(self.reward)?;
        Some((self.block_num, self.reward, self.supply))
    }
}

impl Emission for EmissionSchedule {
    /// Calculate the block reward for the given block height, in µMinotaiji
    fn block_reward(&self, height: u64) -> MicroMinotaiji {
        let iterator = self.inner_schedule(height);
        iterator.block_reward()
    }

    /// Calculate the exact emitted supply after the given block, in µMinotaiji. The value is calculated by summing up
    /// the block reward for each block, making this a very inefficient function if you wanted to call it from a
    /// loop for example. For those cases, use the `iter` function instead.
    fn supply_at_block(&self, height: u64) -> MicroMinotaiji {
        let iterator = self.inner_schedule(height);
        iterator.supply()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        consensus::emission::{Emission, EmissionSchedule},
        transactions::taiji_amount::{uT, MicroMinotaiji, T},
    };

    #[test]
    fn schedule() {
        let schedule = EmissionSchedule::new(
            MicroMinotaiji::from(10_000_100),
            &[22, 23, 24, 26, 27],
            MicroMinotaiji::from(100),
        );
        assert_eq!(schedule.block_reward(0), MicroMinotaiji::from(0));
        assert_eq!(schedule.supply_at_block(0), MicroMinotaiji::from(0));
        assert_eq!(schedule.block_reward(1), MicroMinotaiji::from(10_000_100));
        assert_eq!(schedule.supply_at_block(1), MicroMinotaiji::from(10_000_100));
        // These values have been independently calculated
        assert_eq!(schedule.block_reward(100 + 1), MicroMinotaiji::from(9_999_800));
        assert_eq!(schedule.supply_at_block(100 + 1), MicroMinotaiji::from(1_009_994_950));
    }

    #[test]
    fn huge_block_number() {
        // let mut n = (std::i32::MAX - 1) as u64;
        let height = 262_800_000; // 1000 years' problem
        let schedule = EmissionSchedule::new(
            MicroMinotaiji::from(10000000u64),
            &[22, 23, 24, 26, 27],
            MicroMinotaiji::from(100),
        );
        // Slow but does not overflow
        assert_eq!(schedule.block_reward(height + 1), MicroMinotaiji::from(4_194_303));
    }

    #[test]
    fn generate_emission_schedule_as_iterator() {
        const INITIAL: u64 = 10_000_100;
        let schedule = EmissionSchedule::new(
            MicroMinotaiji::from(INITIAL),
            &[2], // 0.25 decay
            MicroMinotaiji::from(100),
        );
        assert_eq!(schedule.block_reward(0), MicroMinotaiji(0));
        assert_eq!(schedule.supply_at_block(0), MicroMinotaiji(0));
        let values = schedule.iter().take(101).collect::<Vec<_>>();
        let (height, reward, supply) = values[0];
        assert_eq!(height, 1);
        assert_eq!(reward, MicroMinotaiji::from(INITIAL));
        assert_eq!(supply, MicroMinotaiji::from(INITIAL));
        let (height, reward, supply) = values[1];
        assert_eq!(height, 2);
        assert_eq!(reward, MicroMinotaiji::from(7_500_075));
        assert_eq!(supply, MicroMinotaiji::from(17_500_175));
        let (height, reward, supply) = values[2];
        assert_eq!(height, 3);
        assert_eq!(reward, MicroMinotaiji::from(5_625_057));
        assert_eq!(supply, MicroMinotaiji::from(23_125_232));
        let (height, reward, supply) = values[10];
        assert_eq!(height, 11);
        assert_eq!(reward, MicroMinotaiji::from(563_142));
        assert_eq!(supply, MicroMinotaiji::from(38_310_986));
        let (height, reward, supply) = values[41];
        assert_eq!(height, 42);
        assert_eq!(reward, MicroMinotaiji::from(100));
        assert_eq!(supply, MicroMinotaiji::from(40_000_252));

        let mut tot_supply = MicroMinotaiji::from(0);
        for (_, reward, supply) in schedule.iter().take(1000) {
            tot_supply += reward;
            assert_eq!(tot_supply, supply);
        }
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn emission() {
        let emission = EmissionSchedule::new(1 * T, &[1, 2], 100 * uT);
        let mut emission = emission.iter();
        // decay is 1 - 0.25 - 0.125 = 0.625
        assert_eq!(emission.block_height(), 0);
        assert_eq!(emission.block_reward(), MicroMinotaiji(0));
        assert_eq!(emission.supply(), MicroMinotaiji(0));

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
}
