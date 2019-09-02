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

use crate::tari_amount::MicroTari;

/// The Tari emission schedule. The emission schedule determines how much Tari is mined as a block reward at every
/// block.
///
/// NB: We don't know what the final emission schedule will be on Tari yet, so do not give any weight to values or
/// formulae provided in this file, they will almost certainly change ahead of main-net release.
#[derive(Clone)]
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
            let base_f = (f64::from(self.initial) * self.decay.powi(block as i32)).trunc();
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
    /// use tari_core::emission::EmissionSchedule;
    /// use tari_core::tari_amount::MicroTari;
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
    use crate::{emission::EmissionSchedule, tari_amount::MicroTari};

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
}
