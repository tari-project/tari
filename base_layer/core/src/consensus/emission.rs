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
use std::cmp;

/// The Tari emission schedule. The emission schedule determines how much Tari is mined as a block reward at every
/// block.
///
/// NB: We don't know what the final emission schedule will be on Tari yet, so do not give any weight to values or
/// formulae provided in this file, they will almost certainly change ahead of main-net release.
#[derive(Debug, Clone)]
pub struct EmissionSchedule {
    initial: MicroTari,
    decay: &'static [u64],
    tail: MicroTari,
}

impl EmissionSchedule {
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
    /// and the decay array is `&[1, 2]`.
    ///
    /// ## Panics
    ///
    /// The shift right operation will overflow if shifting more than 63 bits. `new` will panic if any of the decay
    /// values are greater than or equal to 64.
    pub fn new(initial: MicroTari, decay: &'static [u64], tail: MicroTari) -> EmissionSchedule {
        assert!(
            decay.iter().all(|i| *i < 64),
            "Decay value would overflow. All `decay` values must be less than 64"
        );
        EmissionSchedule { initial, decay, tail }
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
    /// let schedule = EmissionSchedule::new(10.into(), &[3], 1.into());
    /// for (n, reward, supply) in schedule.iter().take(100) {
    ///     println!("{:3} {:9} {:9}", n, reward, supply);
    /// }
    /// ```
    pub fn iter(&self) -> EmissionRate {
        EmissionRate::new(self)
    }

    /// Calculate the block reward for the given block height, in µTari
    pub fn block_reward(&self, height: u64) -> MicroTari {
        self.iter()
            .nth(height)
            .map(|(_, reward, _)| reward)
            .unwrap_or(self.tail)
    }

    /// Calculate the exact emitted supply after the given block, in µTari. The value is calculated by summing up the
    /// block reward for each block, making this a very inefficient function if you wanted to call it from a loop for
    /// example. For those cases, use the `iter` function instead.
    ///
    /// This may return None once the supply > u64::MAX (in practice, billions of years).
    pub fn supply_at_block(&self, height: u64) -> Option<MicroTari> {
        self.iter().nth(height).map(|(_, _, supply)| supply)
    }
}

pub struct EmissionRate<'a> {
    block_num: u64,
    supply: MicroTari,
    reward: MicroTari,
    schedule: &'a EmissionSchedule,
}

impl<'a> EmissionRate<'a> {
    fn new(schedule: &'a EmissionSchedule) -> EmissionRate<'a> {
        EmissionRate {
            block_num: 0,
            supply: schedule.initial,
            reward: schedule.initial,
            schedule,
        }
    }

    /// Calculates the next reward by multiplying the decay factor by the previous block reward using integer math.
    ///
    /// We write the decay factor, 1 - k, as a sum of fraction powers of two. e.g. if we wanted 0.25 as our k, then
    /// (1-k) would be 0.75 = 1/2 plus 1/4 (1/2^2).
    ///
    /// Then we calculate k.R = (1 - e).R = R - e.R = R - (0.5 * R + 0.25 * R) = R - R >> 1 - R >> 2
    fn next_reward(&self) -> MicroTari {
        let r = self.reward.as_u64();
        let next = self
            .schedule
            .decay
            .iter()
            .fold(self.reward, |sum, i| sum - MicroTari::from(r >> *i));

        cmp::max(next, self.schedule.tail)
    }

    /// Returns the nth element of the iterator.
    /// This differs from Iterator::nth in that it takes a u64 to avoid having to handle the failure case when
    /// converting to usize on 32-bit architectures, other than that the implementation is identical.
    fn nth(&mut self, mut n: u64) -> Option<(u64, MicroTari, MicroTari)> {
        for x in self {
            if n == 0 {
                return Some(x);
            }
            n -= 1;
        }
        None
    }
}

impl Iterator for EmissionRate<'_> {
    type Item = (u64, MicroTari, MicroTari);

    fn next(&mut self) -> Option<Self::Item> {
        let emission = (self.block_num, self.reward, self.supply);

        let reward = self.next_reward();
        // Once max supply (as limited by u64) has been reached, the iterator is complete.
        let supply = self.supply.checked_add(reward)?;
        // Once a height of u64::MAX is reached, the iterator is complete.
        let block_num = self.block_num.checked_add(1)?;
        // Only update internal state if the iterator can iterate
        self.reward = reward;
        self.supply = supply;
        self.block_num = block_num;

        Some(emission)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        consensus::emission::EmissionSchedule,
        transactions::tari_amount::{uT, MicroTari, T},
    };

    #[test]
    fn schedule() {
        let schedule = EmissionSchedule::new(MicroTari::from(10_000_100), &[22, 23, 24, 26, 27], MicroTari::from(100));
        let r0 = schedule.block_reward(0);
        assert_eq!(r0, MicroTari::from(10_000_100));
        let s0 = schedule.supply_at_block(0).unwrap();
        assert_eq!(s0, MicroTari::from(10_000_100));
        // These values have been independently calculated
        assert_eq!(schedule.block_reward(100), MicroTari::from(9_999_800));
        assert_eq!(schedule.supply_at_block(100).unwrap(), MicroTari::from(1_009_994_950));
    }

    #[test]
    fn huge_block_number() {
        let schedule = EmissionSchedule::new(MicroTari::from(1e7 as u64), &[22, 23, 24, 26, 27], MicroTari::from(100));
        // 1000 years worth of blocks.
        let height = 262_800_000;
        // Slow but does not overflow
        assert_eq!(schedule.block_reward(height), MicroTari::from(4_194_303));
    }

    #[test]
    fn generate_emission_schedule_as_iterator() {
        const INITIAL: u64 = 10_000_100;
        let schedule = EmissionSchedule::new(
            MicroTari::from(INITIAL),
            &[2], // 0.25 decay
            MicroTari::from(100),
        );
        let values = schedule.iter().take(101).collect::<Vec<_>>();
        let (height, reward, supply) = values[0];
        assert_eq!(height, 0);
        assert_eq!(reward, MicroTari::from(INITIAL));
        assert_eq!(supply, MicroTari::from(INITIAL));
        let (height, reward, supply) = values[1];
        assert_eq!(height, 1);
        assert_eq!(reward, MicroTari::from(7_500_075));
        assert_eq!(supply, MicroTari::from(17_500_175));
        let (height, reward, supply) = values[2];
        assert_eq!(height, 2);
        assert_eq!(reward, MicroTari::from(5_625_057));
        assert_eq!(supply, MicroTari::from(23_125_232));
        let (height, reward, supply) = values[10];
        assert_eq!(height, 10);
        assert_eq!(reward, MicroTari::from(563_142));
        assert_eq!(supply, MicroTari::from(38_310_986));
        let (height, reward, supply) = values[41];
        assert_eq!(height, 41);
        assert_eq!(reward, MicroTari::from(100));
        assert_eq!(supply, MicroTari::from(40_000_252));

        let mut tot_supply = MicroTari::from(0);
        for (_, reward, supply) in schedule.iter().take(1000) {
            tot_supply += reward;
            assert_eq!(tot_supply, supply);
        }
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn emission() {
        let schedule = EmissionSchedule::new(1 * T, &[1, 2], 100 * uT);
        let mut emission = schedule.iter();
        // decay is 1 - 0.25 - 0.125 = 0.625
        assert_eq!(emission.block_num, 0);
        assert_eq!(emission.reward, 1 * T);
        assert_eq!(emission.supply, 1 * T);

        assert_eq!(emission.next(), Some((0, 1 * T, 1 * T)));
        assert_eq!(emission.next(), Some((1, 250_000 * uT, 1_250_000 * uT)));
        assert_eq!(emission.next(), Some((2, 62_500 * uT, 1_312_500 * uT)));
        assert_eq!(emission.next(), Some((3, 15_625 * uT, 1_328_125 * uT)));
        assert_eq!(emission.next(), Some((4, 3_907 * uT, 1_332_032 * uT)));
        assert_eq!(emission.next(), Some((5, 978 * uT, 1_333_010 * uT)));
        assert_eq!(emission.next(), Some((6, 245 * uT, 1_333_255 * uT)));
        // Tail emission kicks in
        assert_eq!(emission.next(), Some((7, 100 * uT, 1_333_355 * uT)));
        assert_eq!(emission.next(), Some((8, 100 * uT, 1_333_455 * uT)));

        let (height, reward, supply) = emission.next().unwrap();
        assert_eq!(height, 9);
        assert_eq!(reward, schedule.block_reward(9));
        assert_eq!(supply, schedule.supply_at_block(9).unwrap());
    }
}
