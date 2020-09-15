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

use crate::transactions::tari_amount::MicroTari;
use num::pow;

/// The Tari emission schedule. The emission schedule determines how much Tari is mined as a block reward at every
/// block.
///
/// NB: We don't know what the final emission schedule will be on Tari yet, so do not give any weight to values or
/// formulae provided in this file, they will almost certainly change ahead of main-net release.
#[derive(Clone)]
pub struct Emission {
    initial: MicroTari,
    decay: &'static [u64],
    tail: MicroTari,
}

impl Emission {
    /// Create a new emission schedule instance.
    ///
    /// The Emission schedule follows a similar pattern to Monero; with an exponentially decaying emission rate with
    /// a constant tail emission rate.
    ///
    /// The block reward is given by
    ///  $$ r_n = r_{n-1} * (1 - \epsilon) + t, n > 0 $$
    ///  $$ r_0 = A_0 $$
    ///
    /// where
    ///  * $$A_0$$ is the genesis block reward
    ///  * $$1 - \epsilon$$ is the decay rate
    ///  * $$t$$ is the constant tail emission rate
    ///
    /// The decay in this constructor is calculated as follows:
    /// $$ \epsilon = \sum 2^{-k} \foreach k \in decay $$
    ///
    /// So for example, if the decay rate is 0.25, then $$\epsilon$$ is 0.75 or 1/2 + 1/4 i.e. `1 >> 1 + 1 >> 2`
    /// and the decay array is `&[1, 2]`
    pub fn new(initial: MicroTari, decay: &'static [u64], tail: MicroTari) -> Emission {
        Emission { initial, decay, tail }
    }

    /// Return an iterator over the block reward and total supply. This is the most efficient way to iterate through
    /// the emission curve if you're interested in the supply as well as the reward.
    ///
    /// This is an infinite iterator, and each value returned is a tuple of (block number, reward, and total supply)
    ///
    /// ```edition2018
    /// use tari_core::consensus::emission::Emission;
    /// use tari_core::transactions::tari_amount::MicroTari;
    /// // Print the reward and supply for first 100 blocks
    /// let schedule = Emission::new(10.into(), &[3], 1.into());
    /// for (n, reward, supply) in schedule.iter().take(100) {
    ///     println!("{:3} {:9} {:9}", n, reward, supply);
    /// }
    /// ```
    pub fn iter(&self) -> EmissionRate {
        EmissionRate::new(self)
    }
}

pub struct EmissionRate<'a> {
    block_num: u64,
    supply: MicroTari,
    reward: MicroTari,
    schedule: &'a Emission,
}

impl<'a> EmissionRate<'a> {
    fn new(schedule: &'a Emission) -> EmissionRate<'a> {
        EmissionRate {
            block_num: 0,
            supply: schedule.initial,
            reward: schedule.initial,
            schedule,
        }
    }

    pub fn supply(&self) -> MicroTari {
        self.supply
    }

    pub fn block_height(&self) -> u64 {
        self.block_num
    }

    pub fn block_reward(&self) -> MicroTari {
        self.reward
    }

    /// Calculates the next reward by multiplying the decay factor by the previous block reward using integer math.
    ///
    /// We write the decay factor, 1 - k, as a sum of fraction powers of two. e.g. if we wanted 0.25 as our k, then
    /// (1-k) would be 0.75 = 1/2 plus 1/4 (1/2^2).
    ///
    /// Then we calculate k.R = (1 - e).R = R - e.R = R - (0.5 * R + 0.25 * R) = R - R >> 1 - R >> 2
    fn next_reward(&self) -> MicroTari {
        let r: u64 = self.reward.into();
        let next = self
            .schedule
            .decay
            .iter()
            .fold(self.reward, |sum, i| sum - MicroTari::from(r >> *i));
        if next > self.schedule.tail {
            next
        } else {
            self.schedule.tail
        }
    }
}

impl<'a> Iterator for EmissionRate<'a> {
    type Item = (u64, MicroTari, MicroTari);

    fn next(&mut self) -> Option<Self::Item> {
        self.reward = self.next_reward();
        self.supply += self.reward;
        self.block_num += 1;
        Some((self.block_num, self.reward, self.supply))
    }
}

/// The Tari emission schedule. The emission schedule determines how much Tari is mined as a block reward at every
/// block.
///
/// NB: We don't know what the final emission schedule will be on Tari yet, so do not give any weight to values or
/// formulae provided in this file, they will almost certainly change ahead of main-net release.
#[derive(Debug, Clone)]
// #[deprecated(note = "Use Emission instead")]
pub struct EmissionSchedule {
    initial: MicroTari,
    decay: f64,
    tail: MicroTari,
}

impl EmissionSchedule {
    /// Create a new emission schedule instance.
    ///
    /// The Emission schedule follows a similar pattern to Monero; with an exponentially decaying emission rate with
    /// a constant tail emission rate.
    ///
    /// The block reward is given by
    ///  $$ r_n = A_0 r^n + t $$
    ///
    /// where
    ///  * $$A_0$$ is the genesis block reward
    ///  * $$1-r$$ is the decay rate
    ///  * $$t$$ is the constant tail emission rate
    pub fn new(initial: MicroTari, decay: f64, tail: MicroTari) -> EmissionSchedule {
        EmissionSchedule { initial, decay, tail }
    }

    /// Calculate the block reward for the given block height, in µTari
    pub fn block_reward(&self, block: u64) -> MicroTari {
        let base = if block < std::i32::MAX as u64 {
            let base_f = (f64::from(self.initial) * pow(self.decay, block as usize)).trunc();
            MicroTari::from(base_f as u64)
        } else {
            MicroTari::from(0)
        };
        base + self.tail
    }

    /// Calculate the exact emitted supply after the given block, in µTari. The value is calculated by summing up the
    /// block reward for each block, making this a very inefficient function if you wanted to call it from a loop for
    /// example. For those cases, use the `iter` function instead.
    pub fn supply_at_block(&self, block: u64) -> MicroTari {
        let mut total = MicroTari::from(0u64);
        for i in 0..=block {
            total += self.block_reward(i);
        }
        total
    }

    /// Return an iterator over the block reward and total supply. This is the most efficient way to iterate through
    /// the emission curve if you're interested in the supply as well as the reward.
    ///
    /// This is an infinite iterator, and each value returned is a tuple of (block number, reward, and total supply)
    ///
    /// ```edition2018
    /// use tari_core::consensus::emission::EmissionSchedule;
    /// use tari_core::transactions::tari_amount::MicroTari;
    /// // Print the reward and supply for first 100 blocks
    /// let schedule = EmissionSchedule::new(10.into(), 0.9, 1.into());
    /// for (n, reward, supply) in schedule.iter().take(100) {
    ///     println!("{:3} {:9} {:9}", n, reward, supply);
    /// }
    /// ```
    pub fn iter(&self) -> EmissionValues {
        EmissionValues::new(self)
    }
}

pub struct EmissionValues<'a> {
    block_num: u64,
    supply: MicroTari,
    reward: MicroTari,
    schedule: &'a EmissionSchedule,
}

impl<'a> EmissionValues<'a> {
    fn new(schedule: &'a EmissionSchedule) -> EmissionValues<'a> {
        EmissionValues {
            block_num: 0,
            supply: MicroTari::default(),
            reward: MicroTari::default(),
            schedule,
        }
    }
}

impl<'a> Iterator for EmissionValues<'a> {
    type Item = (u64, MicroTari, MicroTari);

    fn next(&mut self) -> Option<Self::Item> {
        let n = self.block_num;
        self.reward = self.schedule.block_reward(n);
        self.supply += self.reward;
        self.block_num += 1;
        Some((n, self.reward, self.supply))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        consensus::emission::{Emission, EmissionSchedule},
        transactions::tari_amount::{uT, MicroTari, T},
    };

    /// Commit df95cee73812689bbae77bfb547c1d73a49635d4 introduced a bug in Windows builds that resulted in certain
    /// blocks failing validation tests. The cause was traced to an erroneous implementation of the std::f64::powi
    /// function in Rust toolchain nightly-2020-06-10, where Windows would give a slightly different floating point
    /// result than Linux. This affected the EmissionSchedule::block_reward calculation.
    #[test]
    fn block_reward_edge_cases() {
        const EMISSION_INITIAL: u64 = 5_538_846_115;
        const EMISSION_DECAY: f64 = 0.999_999_560_409_038_5;
        const EMISSION_TAIL: u64 = 1;

        let schedule = EmissionSchedule::new(
            MicroTari::from(EMISSION_INITIAL * uT),
            EMISSION_DECAY,
            MicroTari::from(EMISSION_TAIL * T),
        );

        // Block numbers in these tests represent the edge cases of the pow function.
        assert_eq!(schedule.block_reward(9182), MicroTari::from(5517534590));
        assert_eq!(schedule.block_reward(9430), MicroTari::from(5516933218));
        assert_eq!(schedule.block_reward(10856), MicroTari::from(5513476601));
        assert_eq!(schedule.block_reward(11708), MicroTari::from(5511412391));
        assert_eq!(schedule.block_reward(30335), MicroTari::from(5466475914));
        assert_eq!(schedule.block_reward(33923), MicroTari::from(5457862272));
        assert_eq!(schedule.block_reward(34947), MicroTari::from(5455406466));
    }

    #[test]
    fn schedule() {
        let schedule = EmissionSchedule::new(MicroTari::from(10_000_000), 0.999, MicroTari::from(100));
        let r0 = schedule.block_reward(0);
        assert_eq!(r0, MicroTari::from(10_000_100));
        let s0 = schedule.supply_at_block(0);
        assert_eq!(s0, MicroTari::from(10_000_100));
        assert_eq!(schedule.block_reward(100), MicroTari::from(9_048_021));
        assert_eq!(schedule.supply_at_block(100), MicroTari::from(961_136_499));
    }

    #[test]
    fn huge_block_number() {
        let mut n = (std::i32::MAX - 1) as u64;
        let schedule = EmissionSchedule::new(MicroTari::from(1e21 as u64), 0.999_9999, MicroTari::from(100));
        for _ in 0..3 {
            assert_eq!(schedule.block_reward(n), MicroTari::from(100));
            n += 1;
        }
    }

    #[test]
    fn generate_emission_schedule_as_iterator() {
        let schedule = EmissionSchedule::new(MicroTari::from(10_000_000), 0.999, MicroTari::from(100));
        let values: Vec<(u64, MicroTari, MicroTari)> = schedule.iter().take(101).collect();
        assert_eq!(values[0].0, 0);
        assert_eq!(values[0].1, MicroTari::from(10_000_100));
        assert_eq!(values[0].2, MicroTari::from(10_000_100));
        assert_eq!(values[100].0, 100);
        assert_eq!(values[100].1, MicroTari::from(9_048_021));
        assert_eq!(values[100].2, MicroTari::from(961_136_499));

        let mut tot_supply = MicroTari::default();
        for (_, reward, supply) in schedule.iter().take(1000) {
            tot_supply += reward;
            assert_eq!(tot_supply, supply);
        }
    }

    #[test]
    fn emission() {
        let emission = Emission::new(1 * T, &[1, 2], 100 * uT);
        let mut emission = emission.iter();
        // decay is 1 - 0.25 - 0.125 = 0.625
        assert_eq!(emission.block_height(), 0);
        assert_eq!(emission.block_reward(), 1 * T);
        assert_eq!(emission.supply(), 1 * T);

        assert_eq!(emission.next(), Some((1, 250_000 * uT, 1_250_000 * uT)));
        assert_eq!(emission.next(), Some((2, 62_500 * uT, 1_312_500 * uT)));
        assert_eq!(emission.next(), Some((3, 15_625 * uT, 1_328_125 * uT)));
        assert_eq!(emission.next(), Some((4, 3_907 * uT, 1_332_032 * uT)));
        assert_eq!(emission.next(), Some((5, 978 * uT, 1_333_010 * uT)));
        assert_eq!(emission.next(), Some((6, 245 * uT, 1_333_255 * uT)));
        // Tail emission kicks in
        assert_eq!(emission.next(), Some((7, 100 * uT, 1_333_355 * uT)));
        assert_eq!(emission.next(), Some((8, 100 * uT, 1_333_455 * uT)));

        assert_eq!(emission.block_height(), 8);
        assert_eq!(emission.block_reward(), 100 * uT);
        assert_eq!(emission.supply(), 1333455 * uT);
    }
}
