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

//! # StatsCollector
//!
//! A thread-safe statistics collector for tracking database operation progress using tokio watch channels.
//! This module provides a non-async way to track and monitor progress of long-running database operations.
//!
//! ## Features
//! - Track current and total progress (e.g., block heights)
//! - Calculate operations per second
//! - Estimate remaining time
//! - Subscribe to progress updates via tokio watch channels
//! - Non-async API for easy integration
//!
//! ## Usage Example
//!
//! ```no_run
//! use std::time::Duration;
//! use std::thread;
//!
//! // Create a stats collector for tracking 1000 operations
//! let collector = StatsCollector::with_total_height(1000);
//!
//! // Subscribe to progress updates
//! let mut progress_receiver = collector.subscribe();
//!
//! // Simulate work being done
//! for i in 0..=1000 {
//!     // Update progress
//!     collector.update_progress(i);
//!
//!     // Check current stats
//!     let percentage = collector.progress_percentage();
//!     let ops_per_sec = collector.operations_per_second();
//!
//!     if i % 100 == 0 {
//!         println!("Progress: {:.1}%, Ops/sec: {:.2}", percentage, ops_per_sec);
//!     }
//!
//!     // Simulate some work
//!     thread::sleep(Duration::from_millis(10));
//! }
//!
//! // Check if complete
//! assert!(collector.is_complete());
//! ```

use std::time::{Duration, Instant};

use tokio::sync::watch;

/// Statistics data for database operations
#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseStats {
    pub current_height: u64,
    pub total_height: u64,
    pub progress_percentage: f64,
    pub operations_per_second: f64,
    pub elapsed_time: Duration,
    pub estimated_remaining_time: Option<Duration>,
    pub last_updated: Instant,
}

impl Default for DatabaseStats {
    fn default() -> Self {
        Self {
            current_height: 0,
            total_height: 0,
            progress_percentage: 0.0,
            operations_per_second: 0.0,
            elapsed_time: Duration::from_secs(0),
            estimated_remaining_time: None,
            last_updated: Instant::now(),
        }
    }
}

impl DatabaseStats {
    /// Create new DatabaseStats with initial values
    pub fn new(current_height: u64, total_height: u64) -> Self {
        let progress_percentage = if total_height > 0 {
            (current_height as f64 / total_height as f64) * 100.0
        } else {
            0.0
        };

        Self {
            current_height,
            total_height,
            progress_percentage,
            operations_per_second: 0.0,
            elapsed_time: Duration::from_secs(0),
            estimated_remaining_time: None,
            last_updated: Instant::now(),
        }
    }

    /// Update progress with new current height
    pub fn update_progress(&mut self, current_height: u64, start_time: Instant) {
        self.current_height = current_height;
        self.last_updated = Instant::now();
        self.elapsed_time = self.last_updated - start_time;

        // Update progress percentage
        self.progress_percentage = if self.total_height > 0 {
            (self.current_height as f64 / self.total_height as f64) * 100.0
        } else {
            0.0
        };

        // Calculate operations per second
        if self.elapsed_time.as_millis() > 0 {
            self.operations_per_second = self.current_height as f64 / self.elapsed_time.as_secs_f64();
        }

        // Estimate remaining time
        if self.operations_per_second > 0.0 && self.current_height < self.total_height {
            let remaining_operations = self.total_height - self.current_height;
            let remaining_seconds = remaining_operations as f64 / self.operations_per_second;
            self.estimated_remaining_time = Some(Duration::from_secs_f64(remaining_seconds));
        } else if self.current_height >= self.total_height {
            self.estimated_remaining_time = Some(Duration::from_secs(0));
        }
    }

    /// Check if the operation is complete
    pub fn is_complete(&self) -> bool {
        self.current_height >= self.total_height && self.total_height > 0
    }
}

/// StatsCollector for tracking database operation progress using tokio watch channels
pub struct StatsCollector {
    sender: watch::Sender<DatabaseStats>,
    receiver: watch::Receiver<DatabaseStats>,
    start_time: Instant,
}

impl StatsCollector {
    /// Create a new StatsCollector with initial stats
    pub fn new() -> Self {
        let initial_stats = DatabaseStats::default();
        let (sender, receiver) = watch::channel(initial_stats);

        Self {
            sender,
            receiver,
            start_time: Instant::now(),
        }
    }

    /// Create a new StatsCollector with specified total height
    pub fn with_total_height(total_height: u64) -> Self {
        let initial_stats = DatabaseStats::new(0, total_height);
        let (sender, receiver) = watch::channel(initial_stats);

        Self {
            sender,
            receiver,
            start_time: Instant::now(),
        }
    }

    /// Get a receiver for watching stats updates
    pub fn subscribe(&self) -> watch::Receiver<DatabaseStats> {
        self.receiver.clone()
    }

    /// Get the current stats without watching for changes
    pub fn current_stats(&self) -> DatabaseStats {
        self.receiver.borrow().clone()
    }

    /// Update the current progress
    pub fn update_progress(&self, current_height: u64) {
        let mut stats = self.receiver.borrow().clone();
        stats.update_progress(current_height, self.start_time);
        let _ = self.sender.send(stats);
    }

    /// Set the total height for progress calculation
    pub fn set_total_height(&self, total_height: u64) {
        let mut stats = self.receiver.borrow().clone();
        stats.total_height = total_height;
        stats.update_progress(stats.current_height, self.start_time);
        let _ = self.sender.send(stats);
    }

    /// Reset the stats collector with new parameters
    pub fn reset(&self, current_height: u64, total_height: u64) {
        let stats = DatabaseStats::new(current_height, total_height);
        let _ = self.sender.send(stats);
    }

    /// Get the progress percentage (0.0 to 100.0)
    pub fn progress_percentage(&self) -> f64 {
        self.receiver.borrow().progress_percentage
    }

    /// Get the current operations per second
    pub fn operations_per_second(&self) -> f64 {
        self.receiver.borrow().operations_per_second
    }

    /// Get the estimated remaining time
    pub fn estimated_remaining_time(&self) -> Option<Duration> {
        self.receiver.borrow().estimated_remaining_time
    }

    /// Check if the operation is complete
    pub fn is_complete(&self) -> bool {
        self.receiver.borrow().is_complete()
    }

    /// Get elapsed time since collector was created/reset
    pub fn elapsed_time(&self) -> Duration {
        self.receiver.borrow().elapsed_time
    }

    /// Get the current and total heights as a tuple
    pub fn heights(&self) -> (u64, u64) {
        let stats = self.receiver.borrow();
        (stats.current_height, stats.total_height)
    }
}

impl Default for StatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_database_stats_creation() {
        let stats = DatabaseStats::new(50, 100);
        assert_eq!(stats.current_height, 50);
        assert_eq!(stats.total_height, 100);
        assert_eq!(stats.progress_percentage, 50.0);
    }

    #[test]
    fn test_database_stats_progress_update() {
        let mut stats = DatabaseStats::new(0, 100);
        let start_time = Instant::now();

        // Simulate some time passing
        thread::sleep(Duration::from_millis(100));
        stats.update_progress(25, start_time);

        assert_eq!(stats.current_height, 25);
        assert_eq!(stats.progress_percentage, 25.0);
        // Only check operations per second if enough time has elapsed
        if stats.elapsed_time.as_millis() > 0 {
            assert!(stats.operations_per_second > 0.0);
        }
        assert!(!stats.is_complete());
    }

    #[test]
    fn test_database_stats_completion() {
        let mut stats = DatabaseStats::new(0, 100);
        let start_time = Instant::now();

        stats.update_progress(100, start_time);
        assert!(stats.is_complete());
        assert_eq!(stats.progress_percentage, 100.0);
    }

    #[test]
    fn test_stats_collector_creation() {
        let collector = StatsCollector::new();
        let stats = collector.current_stats();

        assert_eq!(stats.current_height, 0);
        assert_eq!(stats.total_height, 0);
        assert_eq!(stats.progress_percentage, 0.0);
    }

    #[test]
    fn test_stats_collector_with_total_height() {
        let collector = StatsCollector::with_total_height(1000);
        let stats = collector.current_stats();

        assert_eq!(stats.current_height, 0);
        assert_eq!(stats.total_height, 1000);
        assert_eq!(stats.progress_percentage, 0.0);
    }

    #[test]
    fn test_stats_collector_progress_update() {
        let collector = StatsCollector::with_total_height(100);

        collector.update_progress(25);
        assert_eq!(collector.progress_percentage(), 25.0);
        assert_eq!(collector.heights(), (25, 100));
        assert!(!collector.is_complete());

        collector.update_progress(100);
        assert_eq!(collector.progress_percentage(), 100.0);
        assert!(collector.is_complete());
    }

    #[test]
    fn test_stats_collector_subscription() {
        let collector = StatsCollector::with_total_height(100);
        let mut receiver = collector.subscribe();

        // Initial state
        assert_eq!(receiver.borrow().current_height, 0);

        // Update progress
        collector.update_progress(50);

        // Check if receiver got the update
        let stats = receiver.borrow_and_update();
        assert_eq!(stats.current_height, 50);
        assert_eq!(stats.progress_percentage, 50.0);
    }

    #[test]
    fn test_stats_collector_reset() {
        let collector = StatsCollector::with_total_height(100);

        collector.update_progress(50);
        assert_eq!(collector.heights(), (50, 100));

        collector.reset(0, 200);
        assert_eq!(collector.heights(), (0, 200));
        assert_eq!(collector.progress_percentage(), 0.0);
    }
}
