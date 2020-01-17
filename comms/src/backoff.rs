// Copyright 2019, The Tari Project
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

use std::time::Duration;

pub type BoxedBackoff = Box<dyn Backoff + Send + Sync>;

pub trait Backoff {
    fn calculate_backoff(&self, attempts: usize) -> Duration;
}

impl Backoff for BoxedBackoff {
    fn calculate_backoff(&self, attempts: usize) -> Duration {
        (**self).calculate_backoff(attempts)
    }
}

#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    factor: f32,
}

impl ExponentialBackoff {
    pub fn new(factor: f32) -> Self {
        Self { factor }
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self::new(1.5)
    }
}

impl Backoff for ExponentialBackoff {
    fn calculate_backoff(&self, attempts: usize) -> Duration {
        if attempts == 0 {
            return Duration::from_secs(0);
        }
        let secs = (self.factor as f64) * (f64::powf(2.0, attempts as f64) - 1.0);
        Duration::from_secs(secs.ceil() as u64)
    }
}

#[derive(Clone)]
pub struct ConstantBackoff(Duration);

impl ConstantBackoff {
    pub fn new(timeout: Duration) -> Self {
        Self(timeout)
    }
}

impl Backoff for ConstantBackoff {
    fn calculate_backoff(&self, attempts: usize) -> Duration {
        if attempts <= 1 {
            return Duration::from_secs(0);
        }
        self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default_backoff() {
        let backoff = ExponentialBackoff::default();
        assert_eq!(backoff.calculate_backoff(0).as_secs(), 0);
        assert_eq!(backoff.calculate_backoff(1).as_secs(), 2);
        assert_eq!(backoff.calculate_backoff(2).as_secs(), 5);
        assert_eq!(backoff.calculate_backoff(3).as_secs(), 11);
        assert_eq!(backoff.calculate_backoff(4).as_secs(), 23);
        assert_eq!(backoff.calculate_backoff(5).as_secs(), 47);
        assert_eq!(backoff.calculate_backoff(6).as_secs(), 95);
        assert_eq!(backoff.calculate_backoff(7).as_secs(), 191);
        assert_eq!(backoff.calculate_backoff(8).as_secs(), 383);
        assert_eq!(backoff.calculate_backoff(9).as_secs(), 767);
        assert_eq!(backoff.calculate_backoff(10).as_secs(), 1535);
    }

    #[test]
    fn zero_backoff() {
        let backoff = ExponentialBackoff::new(0.0);
        assert_eq!(backoff.calculate_backoff(0).as_secs(), 0);
        assert_eq!(backoff.calculate_backoff(1).as_secs(), 0);
        assert_eq!(backoff.calculate_backoff(200).as_secs(), 0);
    }
}
