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
//

use std::time::Duration;
use tokio::time;

/// A simple back-off strategy. `BackOff` is typically used in situations where you want to retry an operation a
/// number of times, with an increasing delay between attempts
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use tari_core::base_node::BackOff;
///
/// fn foo(n: u64) -> Result<(), u64> {
///     if n < 3 {
///         Err(n)
///     } else {
///         Ok(())
///     }
/// }
/// let mut backoff = BackOff::new(5, Duration::from_millis(100), 1.5);
/// async {
///     let mut attempts = 1;
///     while !backoff.is_finished() {
///         match foo(attempts) {
///             Ok(_) => backoff.stop(),
///             Err(n) => {
///                 assert!(n < 3);
///                 backoff.wait().await;
///                 attempts += 1;
///             },
///         }
///     }
/// };
/// assert_eq!(backoff.attempts(), 4);
/// ```
pub struct BackOff {
    max_attempts: usize,
    current_attempts: usize,
    delay: Duration,
    backoff: f64,
    stopped: bool,
}

impl BackOff {
    /// Create a new `BackOff` timer.
    ///
    /// # Parameters
    /// * max_attempts: The total number of attempts to make
    /// * delay: The initial duration to wait for after the first attempt
    /// * factor: The factor to apply to the delay after each attempt
    pub fn new(max_attempts: usize, delay: Duration, factor: f64) -> Self {
        BackOff {
            max_attempts,
            current_attempts: 0,
            delay,
            backoff: factor,
            stopped: false,
        }
    }

    pub fn attempts(&self) -> usize {
        self.current_attempts
    }

    pub fn max_attempts(&self) -> usize {
        self.max_attempts
    }

    pub fn is_finished(&self) -> bool {
        self.current_attempts >= self.max_attempts || self.stopped
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub fn stop(&mut self) {
        self.stopped = true
    }

    pub async fn wait(&mut self) -> () {
        if self.is_finished() {
            return;
        }
        time::delay_for(self.delay).await;
        self.current_attempts += 1;
        self.delay = self.delay.mul_f64(self.backoff);
    }
}

#[cfg(test)]
mod test {
    use crate::base_node::BackOff;
    use std::time::Duration;

    #[tokio_macros::test]
    async fn retry() {
        let mut retry = BackOff::new(3, Duration::from_millis(100), 1.5);
        assert_eq!(retry.attempts(), 0);
        retry.wait().await;
        assert_eq!(retry.attempts(), 1);
        assert_eq!(retry.is_finished(), false);
        retry.wait().await;
        assert_eq!(retry.attempts(), 2);
        assert_eq!(retry.is_finished(), false);
        retry.wait().await;
        assert_eq!(retry.attempts(), 3);
        assert_eq!(retry.is_finished(), true);
        retry.wait().await;
        assert_eq!(retry.attempts(), 3);
        assert_eq!(retry.is_finished(), true);
    }
}
