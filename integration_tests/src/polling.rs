//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{sync::OnceLock, time::Duration};

/// Convenience: 2-minute timeout (matches the old `TWO_MINUTES_WITH_HALF_SECOND_SLEEP` pattern).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Convenience: shorter timeout for operations that should be fast.
pub const SHORT_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns the timeout multiplier, read from `INTEGRATION_TEST_TIMEOUT_MULTIPLIER` env var.
///
/// Defaults to 1.0. Set to e.g. 2.0 in CI to double all timeouts for slower environments,
/// or 0.5 locally to fail faster during development.
///
/// The value is read once and cached for the lifetime of the process.
pub fn timeout_multiplier() -> f64 {
    static MULTIPLIER: OnceLock<f64> = OnceLock::new();
    *MULTIPLIER.get_or_init(|| {
        std::env::var("INTEGRATION_TEST_TIMEOUT_MULTIPLIER")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| if v > 0.0 { v } else { 1.0 })
            .unwrap_or(1.0)
    })
}

/// Apply the timeout multiplier to a duration.
pub fn scaled_timeout(base: Duration) -> Duration {
    Duration::from_secs_f64(base.as_secs_f64() * timeout_multiplier())
}

/// Poll an async condition with exponential backoff until it succeeds or the timeout is reached.
///
/// The `$condition` expression must evaluate to `Result<bool, String>`:
/// - `Ok(true)` — condition met, stop polling
/// - `Ok(false)` — not yet met, continue polling
/// - `Err(msg)` — not yet met, record `msg` for the timeout panic message
///
/// The polling interval starts at 250ms and grows by 1.5x each iteration, capped at 4s.
///
/// Timeouts are automatically scaled by `INTEGRATION_TEST_TIMEOUT_MULTIPLIER` env var
/// (default 1.0). Set to 2.0 in CI for slower environments.
///
/// # Usage
/// ```ignore
/// wait_for!(
///     timeout: DEFAULT_TIMEOUT,
///     description: format!("node {} to reach height {}", node, height),
///     condition: async {
///         let tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
///         let h = tip.metadata.unwrap().best_block_height;
///         if h >= height { Ok(true) } else { Err(format!("current height {h}")) }
///     }
/// );
/// ```
#[macro_export]
macro_rules! wait_for {
    // Variant with custom max_interval for poll-sensitive operations
    (
        timeout: $timeout:expr,
        max_interval: $max_iv:expr,
        description: $desc:expr,
        condition: async $body:block
    ) => {{
        let __start = ::tokio::time::Instant::now();
        let __timeout: ::std::time::Duration = $crate::polling::scaled_timeout($timeout);
        let __desc = $desc;
        let mut __interval = ::std::time::Duration::from_millis(250);
        let __max_interval: ::std::time::Duration = $max_iv;
        let mut __last_error: Option<String> = None;

        loop {
            let __result: Result<bool, String> = async $body .await;
            match __result {
                Ok(true) => break,
                Ok(false) => {},
                Err(e) => { __last_error = Some(e); },
            }

            if __start.elapsed() >= __timeout {
                let mut __msg = format!(
                    "Timed out after {:.1}s waiting for: {}",
                    __start.elapsed().as_secs_f64(),
                    __desc
                );
                if let Some(ref e) = __last_error {
                    __msg.push_str(&format!(" (last state: {e})"));
                }
                panic!("{}", __msg);
            }

            let __remaining = __timeout.saturating_sub(__start.elapsed());
            let __sleep_dur = __interval.min(__remaining);
            ::tokio::time::sleep(__sleep_dur).await;

            __interval = ::std::time::Duration::from_secs_f64(
                (__interval.as_secs_f64() * 1.5).min(__max_interval.as_secs_f64())
            );
        }
    }};
    // Default variant — 4s max interval
    (
        timeout: $timeout:expr,
        description: $desc:expr,
        condition: async $body:block
    ) => {{
        let __start = ::tokio::time::Instant::now();
        let __timeout: ::std::time::Duration = $crate::polling::scaled_timeout($timeout);
        let __desc = $desc;
        let mut __interval = ::std::time::Duration::from_millis(250);
        let __max_interval = ::std::time::Duration::from_secs(4);
        let mut __last_error: Option<String> = None;

        loop {
            let __result: Result<bool, String> = async $body .await;
            match __result {
                Ok(true) => break,
                Ok(false) => {},
                Err(e) => { __last_error = Some(e); },
            }

            if __start.elapsed() >= __timeout {
                let mut __msg = format!(
                    "Timed out after {:.1}s waiting for: {}",
                    __start.elapsed().as_secs_f64(),
                    __desc
                );
                if let Some(ref e) = __last_error {
                    __msg.push_str(&format!(" (last state: {e})"));
                }
                panic!("{}", __msg);
            }

            let __remaining = __timeout.saturating_sub(__start.elapsed());
            let __sleep_dur = __interval.min(__remaining);
            ::tokio::time::sleep(__sleep_dur).await;

            // Exponential backoff capped at max_interval
            __interval = ::std::time::Duration::from_secs_f64(
                (__interval.as_secs_f64() * 1.5).min(__max_interval.as_secs_f64())
            );
        }
    }};
}
