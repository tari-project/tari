//  Copyright 2020, The Tari Project
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

use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

use crate::utils::datetime::format_duration;

/// Circuit breaker state for peer connections
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CircuitBreakerState {
    /// Normal operation - connections are allowed
    #[default]
    Closed,
    /// Failures exceeded threshold - connections are blocked
    Open { opened_at: Instant },
    /// Testing phase - limited connections allowed to test recovery
    HalfOpen,
}

impl CircuitBreakerState {
    pub fn is_open(&self) -> bool {
        matches!(self, CircuitBreakerState::Open { .. })
    }

    pub fn is_half_open(&self) -> bool {
        matches!(self, CircuitBreakerState::HalfOpen)
    }

    pub fn is_closed(&self) -> bool {
        matches!(self, CircuitBreakerState::Closed)
    }
}

/// Connection attempt result for tracking success rates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionAttemptResult {
    Success,
    Failure,
}

/// Time-windowed connection attempt record
#[derive(Debug, Clone)]
struct ConnectionAttempt {
    timestamp: Instant,
    result: ConnectionAttemptResult,
}

/// Health metrics for a peer connection
#[derive(Debug, Clone, Default)]
pub struct PeerHealthMetrics {
    /// Circuit breaker state
    circuit_breaker_state: CircuitBreakerState,
    /// Last connection attempt timestamp
    last_attempt: Option<Instant>,
    /// Consecutive failures count
    consecutive_failures: usize,
    /// Rolling window of connection attempts for success rate calculation
    connection_attempts: VecDeque<ConnectionAttempt>,
    /// Optional average latency for connection establishment
    avg_connection_latency: Option<Duration>,
}

impl PeerHealthMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful connection attempt
    pub fn record_success(&mut self, latency: Option<Duration>) {
        self.last_attempt = Some(Instant::now());
        self.consecutive_failures = 0;
        self.add_attempt(ConnectionAttemptResult::Success);

        if let Some(latency) = latency {
            self.update_latency(latency);
        }

        // Transition circuit breaker state on successful connection
        if self.circuit_breaker_state == CircuitBreakerState::HalfOpen {
            self.circuit_breaker_state = CircuitBreakerState::Closed;
        }
    }

    /// Record a failed connection attempt
    pub fn record_failure(&mut self, failure_threshold: usize) {
        self.last_attempt = Some(Instant::now());
        self.consecutive_failures += 1;
        self.add_attempt(ConnectionAttemptResult::Failure);

        // Transition to open state if threshold exceeded
        if self.consecutive_failures >= failure_threshold && !self.circuit_breaker_state.is_open() {
            self.circuit_breaker_state = CircuitBreakerState::Open {
                opened_at: Instant::now(),
            };
        }
    }

    /// Check if connection attempts should be allowed
    pub fn should_allow_connection(&self, retry_interval: Duration) -> bool {
        match &self.circuit_breaker_state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::HalfOpen => true,
            CircuitBreakerState::Open { opened_at } => opened_at.elapsed() >= retry_interval,
        }
    }

    /// Transition from open to half-open state for testing
    pub fn try_half_open(&mut self, retry_interval: Duration) -> bool {
        if let CircuitBreakerState::Open { opened_at } = &self.circuit_breaker_state {
            if opened_at.elapsed() >= retry_interval {
                self.circuit_breaker_state = CircuitBreakerState::HalfOpen;

                return true;
            }
        }
        false
    }

    /// Calculate success rate within the given time window
    pub fn success_rate(&self, window: Duration) -> f32 {
        let cutoff = Instant::now() - window;

        let recent_attempts: Vec<_> = self
            .connection_attempts
            .iter()
            .filter(|attempt| attempt.timestamp > cutoff)
            .collect();

        if recent_attempts.is_empty() {
            // Use conservative Bayesian prior: α=1 (success), β=3 (failure)
            // This gives initial expectation of 1/(1+3) = 0.25 for new peers
            return 0.25; // Conservative default based on Bayesian prior
        }

        let successes = recent_attempts
            .iter()
            .filter(|attempt| attempt.result == ConnectionAttemptResult::Success)
            .count();

        // Apply Bayesian averaging with conservative prior
        // α=1 (prior successes), β=3 (prior failures)
        let alpha = 1.0;
        let beta = 3.0;
        let success_count = successes as f32;
        let total_attempts = recent_attempts.len() as f32;

        (alpha + success_count) / (alpha + beta + total_attempts)
    }

    /// Get the circuit breaker state
    pub fn circuit_breaker_state(&self) -> &CircuitBreakerState {
        &self.circuit_breaker_state
    }

    /// Get consecutive failures count
    pub fn consecutive_failures(&self) -> usize {
        self.consecutive_failures
    }

    /// Get last attempt timestamp
    pub fn last_attempt(&self) -> Option<Instant> {
        self.last_attempt
    }

    /// Get average connection latency if available
    pub fn avg_connection_latency(&self) -> Option<Duration> {
        self.avg_connection_latency
    }

    /// Clean up old connection attempts outside the window
    pub fn cleanup_old_attempts(&mut self, window: Duration) {
        let cutoff = Instant::now() - window;
        while let Some(front) = self.connection_attempts.front() {
            if front.timestamp <= cutoff {
                self.connection_attempts.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate a health score for peer selection (0.0 = unhealthy, 1.0 = healthy)
    pub fn health_score(&self, window: Duration) -> f32 {
        if self.circuit_breaker_state.is_open() {
            return 0.0;
        }

        let success_rate = self.success_rate(window);
        let failure_penalty = if self.consecutive_failures > 0 {
            0.1 * self.consecutive_failures as f32
        } else {
            0.0
        };

        (success_rate - failure_penalty).clamp(0.0, 1.0)
    }

    fn add_attempt(&mut self, result: ConnectionAttemptResult) {
        self.connection_attempts.push_back(ConnectionAttempt {
            timestamp: Instant::now(),
            result,
        });

        // Limit queue size to prevent unbounded growth
        const MAX_ATTEMPTS_HISTORY: usize = 100;
        if self.connection_attempts.len() > MAX_ATTEMPTS_HISTORY {
            self.connection_attempts.pop_front();
        }
    }

    fn update_latency(&mut self, new_latency: Duration) {
        match self.avg_connection_latency {
            Some(current_avg) => {
                // Simple exponential moving average
                const ALPHA: f32 = 0.3;
                let new_avg_millis =
                    (1.0 - ALPHA) * current_avg.as_millis() as f32 + ALPHA * new_latency.as_millis() as f32;
                #[allow(clippy::cast_possible_truncation)]
                let millis = new_avg_millis as u64;
                self.avg_connection_latency = Some(Duration::from_millis(millis));
            },
            None => {
                self.avg_connection_latency = Some(new_latency);
            },
        }
    }
}

impl fmt::Display for PeerHealthMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Health(state: {:?}", self.circuit_breaker_state)?;

        if self.consecutive_failures > 0 {
            write!(f, ", failures: {}", self.consecutive_failures)?;
        }

        if let Some(latency) = self.avg_connection_latency {
            write!(f, ", latency: {}", format_duration(latency))?;
        }

        write!(f, ")")
    }
}

impl fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CircuitBreakerState::Closed => write!(f, "Closed"),
            CircuitBreakerState::Open { opened_at } => {
                write!(f, "Open({})", format_duration(opened_at.elapsed()))
            },
            CircuitBreakerState::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn test_circuit_breaker_transitions() {
        let mut metrics = PeerHealthMetrics::new();
        let failure_threshold = 3;
        let retry_interval = Duration::from_millis(100);

        // Start in closed state
        assert!(metrics.circuit_breaker_state.is_closed());
        assert!(metrics.should_allow_connection(retry_interval));

        // Record failures to trigger open state
        for _ in 0..failure_threshold {
            metrics.record_failure(failure_threshold);
        }

        assert!(metrics.circuit_breaker_state.is_open());
        assert!(!metrics.should_allow_connection(retry_interval));

        // Wait for retry interval
        thread::sleep(retry_interval + Duration::from_millis(10));

        // Should allow connection after retry interval
        assert!(metrics.should_allow_connection(retry_interval));

        // Transition to half-open
        assert!(metrics.try_half_open(retry_interval));
        assert!(metrics.circuit_breaker_state.is_half_open());

        // Success should close the circuit
        metrics.record_success(Some(Duration::from_millis(50)));
        assert!(metrics.circuit_breaker_state.is_closed());
    }

    #[test]
    fn test_success_rate_calculation() {
        let mut metrics = PeerHealthMetrics::new();
        let window = Duration::from_secs(60);

        // Record mixed results
        metrics.record_success(None);
        metrics.record_success(None);
        metrics.record_failure(5);
        metrics.record_success(None);

        let success_rate = metrics.success_rate(window);
        // Bayesian calculation: (α + successes) / (α + β + total_attempts)
        // (1 + 3) / (1 + 3 + 4) = 4/8 = 0.5
        assert_eq!(success_rate, 0.5);
    }

    #[test]
    fn test_circuit_breaker_cooldown_preservation() {
        let mut metrics = PeerHealthMetrics::new();

        // Record enough failures to open circuit breaker
        for _ in 0..5 {
            metrics.record_failure(3);
        }
        assert!(metrics.circuit_breaker_state.is_open());

        // Get the original opened_at timestamp
        let original_opened_at = match &metrics.circuit_breaker_state {
            CircuitBreakerState::Open { opened_at } => *opened_at,
            _ => panic!("Circuit breaker should be open"),
        };

        // Record additional failures - should NOT reset the cooldown timer
        metrics.record_failure(3);
        metrics.record_failure(3);

        // Verify the opened_at timestamp hasn't changed
        if let CircuitBreakerState::Open { opened_at } = &metrics.circuit_breaker_state {
            assert_eq!(
                *opened_at, original_opened_at,
                "Circuit breaker cooldown should not reset on repeated failures"
            );
        } else {
            panic!("Circuit breaker should still be open");
        }
    }

    #[test]
    fn test_bayesian_success_rate_with_no_data() {
        let metrics = PeerHealthMetrics::new();
        let window = Duration::from_secs(60);

        // With no data, should return conservative Bayesian prior
        let success_rate = metrics.success_rate(window);
        assert_eq!(success_rate, 0.25);
    }

    #[test]
    fn test_bayesian_success_rate_calculation() {
        let mut metrics = PeerHealthMetrics::new();
        let window = Duration::from_secs(60);

        // Test Bayesian calculation with different scenarios

        // Scenario 1: All successes
        for _ in 0..5 {
            metrics.record_success(None);
        }
        let rate1 = metrics.success_rate(window);
        // (α + successes) / (α + β + total) = (1 + 5) / (1 + 3 + 5) = 6/9 = 0.666...
        assert!((rate1 - 6.0 / 9.0).abs() < 0.001);

        // Reset for next test
        let mut metrics = PeerHealthMetrics::new();

        // Scenario 2: All failures
        for _ in 0..5 {
            metrics.record_failure(10); // High threshold to avoid circuit breaker
        }
        let rate2 = metrics.success_rate(window);
        // (α + successes) / (α + β + total) = (1 + 0) / (1 + 3 + 5) = 1/9 ≈ 0.111
        assert!((rate2 - 1.0 / 9.0).abs() < 0.001);
    }

    #[test]
    fn test_health_score() {
        let mut metrics = PeerHealthMetrics::new();
        let window = Duration::from_secs(60);

        // Conservative default health (Bayesian prior: 0.25)
        assert_eq!(metrics.health_score(window), 0.25);

        // Record some successes and failures to have a mixed success rate
        metrics.record_success(None);
        metrics.record_success(None);
        metrics.record_failure(5); // This will reset consecutive successes, add failure

        let score = metrics.health_score(window);
        assert!(score < 1.0); // Should be less than perfect due to failure penalty
        assert!(score > 0.0); // But should still be positive due to successes

        // Circuit breaker open should result in 0 score
        metrics.record_failure(5);
        metrics.record_failure(5);
        metrics.record_failure(5);
        assert_eq!(metrics.health_score(window), 0.0);
    }
}
