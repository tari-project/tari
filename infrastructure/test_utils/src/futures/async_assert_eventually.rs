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

/// Periodically check if a value becomes the expected value within a maximum number of attempts.
/// The reason this has an 'async' in the name is because this doesn't use `thread::sleep`, but
/// rather tokio::timer::delay(...).  Therefore, needs to be in an async context using tokio threadpool.
///
/// ```nocompile
/// let some_var = 123;
/// async_assert_eventually!(
///    some_var + 1,
///    expect = 124,
///    max_attempts = 10,
///    interval = Duration::from_millis(500)
/// );
/// ```
#[macro_export]
macro_rules! async_assert_eventually {
    ($check_expr:expr, expect = $expect:expr, max_attempts = $max_attempts:expr, interval = $interval:expr $(,)?) => {{
        let mut value = $check_expr;
        let mut attempts = 0;
        while value != $expect {
            attempts += 1;
            if attempts > $max_attempts {
                panic!(
                    "assert_eventually assertion failed. Expression did not equal value after {} attempts.",
                    $max_attempts
                );
            }
            tokio::time::delay_for($interval).await;
            value = $check_expr;
        }
    }};

    ($check_expr:expr, expect = $expect:expr, max_attempts = $max_attempts:expr, $(,)?) => {{
        async_assert_eventually!(
            $check_expr,
            expect = $expect,
            max_attempts = $max_attempts,
            interval = std::time::Duration::from_millis(100)
        );
    }};

    ($check_expr:expr, expect = $expect:expr $(,)?) => {{
        async_assert_eventually!(
            $check_expr,
            expect = $expect,
            max_attempts = 10,
            interval = std::time::Duration::from_millis(100)
        );
    }};
}
