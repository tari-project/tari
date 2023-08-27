//  Copyright 2022, The Taiji Project
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

    pub fn calculate_average_with_min_samples(&self, min_samples: usize) -> Option<Duration> {
        if self.samples.len() < min_samples {
            return None;
        }
        self.calculate_average()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn empty_average_is_none() {
        // zero capacity RollingAverageTime
        let mut subject = RollingAverageTime::new(0);

        // average should assert to None (as there are no elements to compute average from)
        assert_eq!(subject.calculate_average(), None);
        assert_eq!(subject.calculate_average_with_min_samples(0), None);
        assert_eq!(subject.samples.len(), 0);

        // re-test logic after adding sample
        subject.add_sample(Duration::new(0, 999_999_999_u32));
        assert_eq!(subject.calculate_average(), None);
        assert_eq!(subject.calculate_average_with_min_samples(0), None);
        assert_eq!(subject.samples.len(), 0);
    }

    #[test]
    fn calculate_correct_average_with_single_duration() {
        // test case with a single case
        let mut subject = RollingAverageTime::new(1);
        subject.add_sample(Duration::new(1, 0));

        assert_eq!(subject.calculate_average(), Some(Duration::new(1, 0)));

        // insert new element pos full capacity and resulting average
        subject.add_sample(Duration::new(1, 1));
        assert_eq!(subject.calculate_average(), Some(Duration::new(1, 1)));
    }

    #[test]
    fn calculate_correct_average_with_multiple_durations() {
        // test for average calculation over multiple Duration elements
        let mut subject = RollingAverageTime::new(3);

        // durations
        let duration_1 = Duration::new(1, 999_999_999_u32);
        let duration_2 = Duration::new(1, 0_u32);
        let duration_3 = Duration::new(0, 999_999_999_u32);

        // add samples
        subject.add_sample(duration_1);
        subject.add_sample(duration_2);
        subject.add_sample(duration_3);

        // compute correct average
        let correct_avg = (1_999_999_999 + 1_000_000_000 + 999_999_999) / subject.samples.len() as u64;
        let correct_duration = Some(Duration::from_nanos(correct_avg));

        // assert that calculate_average computes the correct average
        let output_avg = subject.calculate_average();
        assert_eq!(output_avg, correct_duration);
    }

    #[test]
    fn correct_calc_samples_per_second() {
        let mut subject = RollingAverageTime::new(3);

        // add samples
        subject.add_sample(Duration::new(0, 999_999_999));
        subject.add_sample(Duration::new(1, 0));
        subject.add_sample(Duration::new(0, 1));

        // assert that samples per second is correctly defined
        let total_time = 2_000_000_f64;
        let correct_sample_per_second = 1_000_000.0 * ((subject.samples.len() as f64) / total_time);
        assert_eq!(subject.calc_samples_per_second(), Some(correct_sample_per_second));
    }
}
