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

use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fmt::{Display, Formatter},
    time::Duration,
};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct PeerConnectionStats {
    /// The last time a connection was successfully made or, None if a successful
    /// connection has never been made.
    pub last_connected_at: Option<NaiveDateTime>,
    /// Represents the last connection attempt
    pub last_connection_attempt: LastConnectionAttempt,
}

impl PeerConnectionStats {
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the last connection as a success. `has_connected()` will return true from here on.
    pub fn set_connection_success(&mut self) {
        self.last_connected_at = Some(Utc::now().naive_utc());
        self.last_connection_attempt = LastConnectionAttempt::Succeeded(Utc::now().naive_utc());
    }

    /// Sets the last connection as a failure
    pub fn set_connection_failed(&mut self) {
        self.last_connection_attempt = LastConnectionAttempt::Failed {
            failed_at: Utc::now().naive_utc(),
            num_attempts: self.failed_attempts() + 1,
        };
    }

    /// Returns true if a successful connection has ever been recorded, otherwise false
    pub fn has_ever_connected(&self) -> bool {
        self.last_connected_at.is_some()
    }

    /// Returns the number of failed attempts. 0 is returned if the `last_connection_attempt` is not `Failed`
    pub fn failed_attempts(&self) -> usize {
        match self.last_connection_attempt {
            LastConnectionAttempt::Failed { num_attempts, .. } => num_attempts,
            _ => 0,
        }
    }

    /// Returns the date time (UTC) since the last failed connection occurred. None is returned if the
    /// `last_connection_attempt` is not `Failed`
    pub fn last_failed_at(&self) -> Option<&NaiveDateTime> {
        match &self.last_connection_attempt {
            LastConnectionAttempt::Failed { failed_at, .. } => Some(failed_at),
            _ => None,
        }
    }

    /// Returns the Duration since the last failed connection occurred. None is returned if the
    /// `last_connection_attempt` is not `Failed`
    pub fn time_since_last_failure(&self) -> Option<Duration> {
        self.last_failed_at()
            .map(|failed_at| Utc::now().naive_utc() - *failed_at)
            .map(convert_to_std_duration)
    }
}

impl fmt::Display for PeerConnectionStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.last_connected_at.as_ref() {
            Some(dt) => {
                write!(f, "Last connected at '{}'.", dt)?;

                if self.last_failed_at().is_some() {
                    write!(f, " {}", self.last_connection_attempt)?;
                }
            },
            None => {
                write!(f, "Never connected to this peer.")?;
                write!(f, " {}", self.last_connection_attempt)?;
            },
        }

        Ok(())
    }
}

/// Peer connection statistics
#[derive(Debug, Clone, Deserialize, Serialize, PartialOrd, PartialEq)]
pub enum LastConnectionAttempt {
    /// This node has never attempted to connect to this peer
    Never,
    /// The last connection attempt was successful
    Succeeded(NaiveDateTime),
    /// The last connection attempt failed.
    Failed {
        /// Timestamp of the last failed attempt
        failed_at: NaiveDateTime,
        /// Number of failed attempts in a row
        num_attempts: usize,
    },
}

/// Convert `chrono::Duration` to `std::time::Duration`
fn convert_to_std_duration(old_duration: chrono::Duration) -> Duration {
    Duration::from_millis(old_duration.num_milliseconds() as u64)
}

impl Default for LastConnectionAttempt {
    fn default() -> Self {
        LastConnectionAttempt::Never
    }
}

impl Display for LastConnectionAttempt {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        use LastConnectionAttempt::*;
        match self {
            Never => write!(f, "Connection never attempted"),
            Succeeded(succeeded_at) => write!(f, "Connection succeeded at {}", succeeded_at),
            Failed {
                failed_at,
                num_attempts,
            } => write!(
                f,
                "Connection failed at {} after {} attempt(s)",
                failed_at, num_attempts
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn peer_connection_stats() {
        let state = PeerConnectionStats::new();
        assert!(state.last_failed_at().is_none());
        assert_eq!(state.failed_attempts(), 0);
        assert!(state.time_since_last_failure().is_none());
        assert_eq!(state.has_ever_connected(), false);

        let mut state = PeerConnectionStats::new();
        state.set_connection_success();
        assert!(state.last_failed_at().is_none());
        assert_eq!(state.failed_attempts(), 0);
        assert!(state.time_since_last_failure().is_none());
        assert_eq!(state.has_ever_connected(), true);

        let mut state = PeerConnectionStats::new();
        state.set_connection_failed();
        state.set_connection_failed();
        state.set_connection_failed();
        assert!(state.last_failed_at().is_some());
        assert_eq!(state.failed_attempts(), 3);
        assert!(state.time_since_last_failure().unwrap().as_millis() < 100);

        assert_eq!(state.has_ever_connected(), false);
        state.set_connection_success();
        assert_eq!(state.has_ever_connected(), true);
    }
}
