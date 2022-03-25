//  Copyright 2022, The Tari Project
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

use std::{convert::TryFrom, time::Duration};

use crate::common::rolling_vec::RollingVec;

#[derive(Debug, Clone)]
pub struct RollingAverageTime {
    samples: RollingVec<Duration>,
}

impl RollingAverageTime {
    pub fn new(num_samples: usize) -> Self {
        Self {
            samples: RollingVec::new(num_samples),
        }
    }

    pub fn add_sample(&mut self, sample: Duration) {
        self.samples.push(sample);
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn calc_samples_per_second(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }

        let total_time = self.samples.iter().sum::<Duration>();
        Some((self.samples.len() as f64 / total_time.as_micros() as f64) * 1_000_000.0)
    }

    pub fn calculate_average(&self) -> Option<Duration> {
        if self.samples.is_empty() {
            return None;
        }

        let total_time = self.samples.iter().sum::<Duration>();
        Some(Duration::from_nanos(
            u64::try_from(total_time.as_nanos()).unwrap_or(u64::MAX) / self.samples.len() as u64,
        ))
    }
}
