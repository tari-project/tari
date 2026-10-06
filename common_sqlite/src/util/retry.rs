// Copyright 2024. The Tari Project
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

use std::{fmt::Display, thread, time::Duration};

use log::*;

const LOG_TARGET: &str = "common_sqlite::retry";

/// Maximum number of retry attempts for database-locked errors.
const DEFAULT_MAX_RETRIES: usize = 10;

/// Initial backoff duration between retries.
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);

/// Maximum backoff duration between retries.
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Returns `true` if the error message indicates a SQLite "database is locked" error.
pub fn is_database_locked_error(err: &dyn Display) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("database is locked") || msg.contains("database table is locked")
}

/// Execute a database operation with automatic retry on "database is locked" errors.
///
/// Uses exponential backoff starting at 100ms, doubling each retry up to 5s,
/// for a maximum of `DEFAULT_MAX_RETRIES` attempts.
///
/// # Arguments
/// * `op_name` - A label for logging which operation is being retried.
/// * `f` - The closure to execute. Called repeatedly until it succeeds or the error is not a "database is locked" error
///   or retries are exhausted.
pub fn retry_db<T, E, F>(op_name: &str, mut f: F) -> Result<T, E>
where
    E: Display,
    F: FnMut() -> Result<T, E>,
{
    let mut backoff = INITIAL_BACKOFF;
    for attempt in 0..DEFAULT_MAX_RETRIES {
        match f() {
            Ok(val) => {
                if attempt > 0 {
                    debug!(
                        target: LOG_TARGET,
                        "{}: succeeded after {} retries", op_name, attempt
                    );
                }
                return Ok(val);
            },
            Err(e) => {
                if !is_database_locked_error(&e) || attempt == DEFAULT_MAX_RETRIES - 1 {
                    if attempt > 0 {
                        warn!(
                            target: LOG_TARGET,
                            "{}: failed after {} retries: {}", op_name, attempt.saturating_add(1), e
                        );
                    }
                    return Err(e);
                }
                warn!(
                    target: LOG_TARGET,
                    "{}: database is locked (attempt {}/{}), retrying in {:?}",
                    op_name,
                    attempt.saturating_add(1),
                    DEFAULT_MAX_RETRIES,
                    backoff
                );
                thread::sleep(backoff);
                backoff = backoff.saturating_mul(2).min(MAX_BACKOFF);
            },
        }
    }
    unreachable!()
}
