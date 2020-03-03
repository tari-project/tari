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

use std::time::Duration;
use tokio::time::delay_for;

/// The structure can be initialized with some state and an internal Delay future. The struct will resolve to the
/// internal state when the delay elapses.
/// This allows for one to create unique delays that can be await'ed upon in a collection like FuturesUnordered
pub struct StateDelay<T> {
    state: T,
    period: Duration,
}

impl<T> StateDelay<T> {
    pub fn new(period: Duration, state: T) -> Self {
        Self { state, period }
    }

    /// The future that will delay for the specified time and then return the internal state
    pub async fn delay(self) -> T {
        delay_for(self.period).await;
        self.state
    }
}

#[cfg(test)]
mod test {
    use crate::util::futures::StateDelay;
    use chrono::{Duration as ChronoDuration, Utc};
    use std::time::Duration;
    use tokio::runtime::Runtime;

    #[derive(Clone, Debug, PartialEq)]
    struct Dummy {
        a: i32,
        b: String,
    }

    #[test]
    fn test_state_delay() {
        let mut runtime = Runtime::new().unwrap();
        let state = Dummy {
            a: 22,
            b: "Testing".to_string(),
        };
        let delay = 1;
        let state_delay_future = StateDelay::new(Duration::from_secs(delay), state.clone());
        let tick = Utc::now().naive_utc();
        let result = runtime.block_on(state_delay_future.delay());
        let tock = Utc::now().naive_utc();
        assert!(tock.signed_duration_since(tick) > ChronoDuration::seconds(0i64));
        assert_eq!(result, state);
    }
}
