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
//
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct SolutionStatistics {
    /// Total found
    pub found: u32,
    /// Total rejected
    pub rejected: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MiningStatistics {
    /// Solutions per second
    sols: Vec<f64>,
    /// Hashes per second
    hashes: u64,
    /// Number Solvers
    pub solvers: usize,
    /// Solution statistics
    pub solution_stats: SolutionStatistics,
}

impl MiningStatistics {
    pub fn add_sols(&mut self, val: f64) {
        self.sols.insert(0, val);
        self.sols.truncate(60);
    }

    pub fn sols(&self) -> f64 {
        if self.sols.is_empty() {
            0.0
        } else {
            let sum: f64 = self.sols.iter().sum();
            sum / (self.sols.len() as f64)
        }
    }

    pub fn add_hash(&mut self) {
        self.hashes += 1;
    }

    pub fn hash_rate(&mut self, elapsed: Duration) -> f64 {
        let hash_rate = self.hashes as f64 / elapsed.as_micros() as f64;
        // reset the total number of hashes for this interval
        self.hashes = 0;
        hash_rate
    }
}

#[derive(Clone, Debug, Default)]
pub struct Statistics {
    pub mining_stats: MiningStatistics,
}
