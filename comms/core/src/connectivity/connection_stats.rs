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

use std::{
    fmt,
    fmt::{Display, Formatter},
    time::{Duration, Instant},
};

use crate::{connectivity::peer_health::PeerHealthMetrics, utils::datetime::format_duration};

/// Basic stats for peer connection attempts. Allows the connectivity manager to keep track of successful/failed
/// connection attempts to allow it to mark peers as offline if necessary.
#[derive(Debug, Clone, Default)]
pub struct PeerConnectionStats {
    /// The last time a connection was successfully made or, None if a successful
    /// connection has never been made.
    pub last_connected_at: Option<Instant>,
    /// Represents the last connection attempt
    pub last_connection_attempt: LastConnectionAttempt,
    /// Advanced health metrics for proactive dialing
    pub health_metrics: PeerHealthMetrics,
}

impl PeerConnectionStats {
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the last connection as a success. `has_connected()` will return true from here on.
    pub fn set_connection_success(&mut self) {
        self.last_connected_at = Some(Instant::now());
        self.last_connection_attempt = LastConnectionAttempt::Succeeded(Instant::now());
        self.health_metrics.record_success(None);
    }

    /// Sets the last connection as a success with connection latency
    pub fn set_connection_success_with_latency(&mut self, latency: Duration) {
        self.last_connected_at = Some(Instant::now());
        self.last_connection_attempt = LastConnectionAttempt::Succeeded(Instant::now());
        self.health_metrics.record_success(Some(latency));
    }

    /// Sets the last connection as a failure
    pub fn set_connection_failed(&mut self) {
        self.last_connection_attempt = LastConnectionAttempt::Failed {
            failed_at: Instant::now(),
            num_attempts: self.failed_attempts() + 1,
        };
        // Use default from ConnectivityConfig to ensure centralization
        use super::config::ConnectivityConfig;
        let default_config = ConnectivityConfig::default();
        self.health_metrics
            .record_failure(default_config.circuit_breaker_failure_threshold);
    }

    /// Sets the last connection as a failure with configurable threshold
    pub fn set_connection_failed_with_threshold(&mut self, circuit_breaker_threshold: usize) {
        self.last_connection_attempt = LastConnectionAttempt::Failed {
            failed_at: Instant::now(),
            num_attempts: self.failed_attempts() + 1,
        };
        self.health_metrics.record_failure(circuit_breaker_threshold);
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
    pub fn last_failed_at(&self) -> Option<Instant> {
        match &self.last_connection_attempt {
            LastConnectionAttempt::Failed { failed_at, .. } => Some(*failed_at),
            _ => None,
        }
    }

    /// Check if connections should be allowed based on circuit breaker state
    pub fn should_allow_connection(&self, retry_interval: Duration) -> bool {
        self.health_metrics.should_allow_connection(retry_interval)
    }

    /// Calculate success rate within the given time window
    pub fn success_rate(&self, window: Duration) -> f32 {
        self.health_metrics.success_rate(window)
    }

    /// Calculate health score for peer selection
    pub fn health_score(&self, window: Duration) -> f32 {
        self.health_metrics.health_score(window)
    }

    /// Try to transition circuit breaker from open to half-open
    pub fn try_half_open(&mut self, retry_interval: Duration) -> bool {
        self.health_metrics.try_half_open(retry_interval)
    }

    /// Clean up old health metrics outside the window
    pub fn cleanup_old_health_data(&mut self, window: Duration) {
        self.health_metrics.cleanup_old_attempts(window);
    }

    /// Get access to underlying health metrics
    pub fn health_metrics(&self) -> &PeerHealthMetrics {
        &self.health_metrics
    }
}

impl fmt::Display for PeerConnectionStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.last_failed_at() {
            Some(_) => {
                write!(f, "{}", self.last_connection_attempt)?;
            },
            None => match self.last_connected_at.as_ref() {
                Some(dt) => {
                    write!(f, "Last connected {} ago", format_duration(dt.elapsed()))?;
                },
                None => {
                    write!(f, "{}", self.last_connection_attempt)?;
                },
            },
        }

        Ok(())
    }
}

/// Peer connection statistics
#[derive(Default, Debug, Clone, PartialOrd, PartialEq, Eq)]
pub enum LastConnectionAttempt {
    /// This node has never attempted to connect to this peer
    #[default]
    Never,
    /// The last connection attempt was successful
    Succeeded(Instant),
    /// The last connection attempt failed.
    Failed {
        /// Timestamp of the last failed attempt
        failed_at: Instant,
        /// Number of failed attempts in a row
        num_attempts: usize,
    },
}

impl Display for LastConnectionAttempt {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        use LastConnectionAttempt::{Failed, Never, Succeeded};
        match self {
            Never => write!(f, "Connection never attempted"),
            Succeeded(succeeded_at) => write!(
                f,
                "Connection succeeded {} ago",
                format_duration(succeeded_at.elapsed())
            ),
            Failed {
                failed_at,
                num_attempts,
            } => write!(
                f,
                "Connection failed {} ago ({} attempt(s))",
                format_duration(failed_at.elapsed()),
                num_attempts
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

        let mut state = PeerConnectionStats::new();
        state.set_connection_success();
        assert!(state.last_failed_at().is_none());
        assert_eq!(state.failed_attempts(), 0);

        let mut state = PeerConnectionStats::new();
        state.set_connection_failed();
        state.set_connection_failed();
        state.set_connection_failed();
        assert!(state.last_failed_at().is_some());
        assert_eq!(state.failed_attempts(), 3);

        state.set_connection_success();
        assert_eq!(state.failed_attempts(), 0);
        assert!(state.last_failed_at().is_none());
    }
}
