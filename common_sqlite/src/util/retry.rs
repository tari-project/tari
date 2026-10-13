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

use std::{
    fmt::Display,
    thread,
    time::{Duration, Instant},
};

use diesel::result::Error as DieselError;
use log::*;

use crate::error::{SqliteStorageError, StorageError};

const LOG_TARGET: &str = "common_sqlite::retry";

/// Maximum number of attempts for database-locked errors. A secondary bound; the time budget normally ends first.
const DEFAULT_MAX_RETRIES: usize = 10;

/// How long [`retry_db`] keeps retrying, counted from the first attempt.
pub const DEFAULT_RETRY_BUDGET: Duration = Duration::from_secs(120);

/// Initial backoff duration between retries.
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);

/// Maximum backoff duration between retries.
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// An error that may wrap a diesel error, so [`retry_db`] can tell a SQLite lock from anything else.
pub trait DbLockedError {
    /// The underlying diesel error, if this error wraps one.
    fn diesel_error(&self) -> Option<&DieselError>;
}

impl DbLockedError for DieselError {
    fn diesel_error(&self) -> Option<&DieselError> {
        Some(self)
    }
}

impl DbLockedError for StorageError {
    fn diesel_error(&self) -> Option<&DieselError> {
        match self {
            StorageError::DieselResultError(e) => Some(e),
            _ => None,
        }
    }
}

impl DbLockedError for SqliteStorageError {
    /// Never a statement error: a pool checkout that timed out is not retried, because its wait has already been as
    /// long as a busy timeout.
    fn diesel_error(&self) -> Option<&DieselError> {
        None
    }
}

/// Returns `true` if `err` wraps a diesel `DatabaseError` whose SQLite message is "database is locked" or "database
/// table is locked". Pool checkout errors never match, even when their text mentions a lock.
pub fn is_database_locked<E: DbLockedError>(err: &E) -> bool {
    match err.diesel_error() {
        Some(DieselError::DatabaseError(_, info)) => {
            let msg = info.message().to_lowercase();
            msg == "database is locked" || msg == "database table is locked"
        },
        _ => false,
    }
}

/// Execute a database operation with automatic retry on "database is locked" errors, for at most
/// [`DEFAULT_RETRY_BUDGET`]. See [`retry_db_with_budget`].
pub fn retry_db<T, E, F>(op_name: &str, f: F) -> Result<T, E>
where
    E: DbLockedError + Display,
    F: FnMut() -> Result<T, E>,
{
    retry_db_with_budget(op_name, DEFAULT_RETRY_BUDGET, f)
}

/// Execute a database operation with automatic retry on "database is locked" errors.
///
/// Uses exponential backoff starting at 100ms, doubling each retry up to 5s. Stops when the next attempt would start
/// after `budget` has elapsed since the first, or after `DEFAULT_MAX_RETRIES` attempts, and returns the last error.
///
/// The closure must be a single transaction or otherwise idempotent: a failed attempt is run again from the start.
/// This function blocks the calling thread with `std::thread::sleep`; callers on an async runtime should wrap it in
/// `spawn_blocking`.
///
/// # Arguments
/// * `op_name` - A label for logging which operation is being retried.
/// * `budget` - The total time allowed for attempts and backoff.
/// * `f` - The closure to execute. Called repeatedly until it succeeds, the error is not a "database is locked" error,
///   or the budget or attempts are exhausted.
pub fn retry_db_with_budget<T, E, F>(op_name: &str, budget: Duration, mut f: F) -> Result<T, E>
where
    E: DbLockedError + Display,
    F: FnMut() -> Result<T, E>,
{
    let start = Instant::now();
    let mut backoff = INITIAL_BACKOFF;
    let mut attempt: usize = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let e = match f() {
            Ok(val) => {
                if attempt > 1 {
                    debug!(
                        target: LOG_TARGET,
                        "{}: succeeded after {} attempts", op_name, attempt
                    );
                }
                return Ok(val);
            },
            Err(e) => e,
        };
        if !is_database_locked(&e) {
            if attempt > 1 {
                warn!(
                    target: LOG_TARGET,
                    "{}: failed after {} attempts: {}", op_name, attempt, e
                );
            }
            return Err(e);
        }
        if attempt >= DEFAULT_MAX_RETRIES {
            warn!(
                target: LOG_TARGET,
                "{}: database still locked after {} attempts in {:.1?}: {}", op_name, attempt, start.elapsed(), e
            );
            return Err(e);
        }
        if start.elapsed().saturating_add(backoff) >= budget {
            warn!(
                target: LOG_TARGET,
                "{}: budget exhausted after {} attempts in {:.1?}: {}", op_name, attempt, start.elapsed(), e
            );
            return Err(e);
        }
        warn!(
            target: LOG_TARGET,
            "{}: database is locked (attempt {}/{}), retrying in {:?}", op_name, attempt, DEFAULT_MAX_RETRIES, backoff
        );
        thread::sleep(backoff);
        backoff = backoff.saturating_mul(2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    use diesel::{Connection, SqliteConnection, connection::SimpleConnection, result::DatabaseErrorKind};

    use super::*;

    fn locked_error() -> DieselError {
        DieselError::DatabaseError(DatabaseErrorKind::Unknown, Box::new("database is locked".to_string()))
    }

    #[test]
    fn retry_db_does_not_retry_checkout_timeout() {
        let mut attempts = 0;
        let result: Result<(), _> = retry_db("checkout", || {
            attempts += 1;
            Err(SqliteStorageError::DieselR2d2Error(
                "timed out waiting for connection: database is locked".to_string(),
            ))
        });
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn retry_db_retries_locked_diesel_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.db").to_str().unwrap().to_string();
        let mut setup = SqliteConnection::establish(&path).unwrap();
        setup.batch_execute("CREATE TABLE t (id INTEGER);").unwrap();
        drop(setup);

        let (locked_tx, locked_rx) = mpsc::channel();
        let holder_path = path.clone();
        let holder = thread::spawn(move || {
            let mut holder = SqliteConnection::establish(&holder_path).unwrap();
            holder.batch_execute("BEGIN EXCLUSIVE;").unwrap();
            locked_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(300));
            holder.batch_execute("COMMIT;").unwrap();
        });
        locked_rx.recv().unwrap();

        let mut conn = SqliteConnection::establish(&path).unwrap();
        conn.batch_execute("PRAGMA busy_timeout = 100;").unwrap();
        let mut attempts = 0;
        retry_db("insert", || {
            attempts += 1;
            conn.batch_execute("INSERT INTO t VALUES (1);")
        })
        .unwrap();
        holder.join().unwrap();
        assert!(attempts >= 2, "attempts = {attempts}");
    }

    #[test]
    fn retry_db_respects_budget() {
        let start = Instant::now();
        let mut attempts = 0;
        let result: Result<(), _> = retry_db_with_budget("always_locked", Duration::from_millis(400), || {
            attempts += 1;
            Err(locked_error())
        });
        assert!(is_database_locked(&result.unwrap_err()));
        assert!(start.elapsed() < Duration::from_secs(1), "took {:?}", start.elapsed());
        assert!(attempts < 10, "attempts = {attempts}");
        assert!(attempts >= 2, "attempts = {attempts}");
    }

    #[test]
    fn storage_error_classifies_by_variant() {
        assert!(is_database_locked(&StorageError::DieselResultError(locked_error())));
        assert!(!is_database_locked(&StorageError::UnexpectedResult(
            "database is locked".to_string()
        )));
        assert!(!is_database_locked(&DieselError::NotFound));
    }
}
