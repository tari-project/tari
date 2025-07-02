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
//! - Subscribe to progress updates via tokio watch channels
//! - Support for multiple subscribers with additional watch sender channels
//! - Non-async API for easy integration
//!
//! ## Usage Example
//!
//! ```no_run
//! use std::{thread, time::Duration};
//!
//! // Create a stats collector for tracking
//! let collector = StatsCollector::with_total_height(1000);
//!
//! // Subscribe to progress updates
//! let mut progress_receiver = collector.subscribe();
//!
//! // Subscribe additional receivers for multiple consumers
//! let mut ui_receiver = collector.subscribe_sender();
//! let mut logging_receiver = collector.subscribe_sender();
//!
//! // Or add an existing sender
//! let (external_sender, external_receiver) =
//!     tokio::sync::watch::channel(collector.current_stats());
//! collector.add_sender(external_sender);
//!
//! // Simulate work being done
//! for i in 0..=1000 {
//!     // Update progress - all subscribers will receive the update
//!     collector.update_progress(i);
//!
//!     // Check current stats
//!     let percentage = collector.progress_percentage();
//!
//!     if i % 100 == 0 {
//!         println!("Progress: {:.1}%, Ops/sec: {:.2}", percentage, ops_per_sec);
//!         println!(
//!             "Additional subscribers: {}",
//!             collector.additional_subscribers_count()
//!         );
//!     }
//!
//!     // Simulate some work
//!     thread::sleep(Duration::from_millis(10));
//! }
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use chrono::Utc;
use tokio::sync::watch;

use super::lmdb_db::{MetadataKey, MetadataValue};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MigrationStats {
    pub current_height: u64,
    pub total_height: u64,
    pub progress_percentage: f64,
    pub current_db_version: u64,
    pub target_db_version: u64,
}

/// Statistics data for database operations
#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseStats {
    pub migration_stats: MigrationStats,
    pub last_updated: Instant,
    pub metadata: HashMap<MetadataKey, MetadataValue>,
    pub timestamp: u64,
}

impl Default for DatabaseStats {
    fn default() -> Self {
        Self {
            migration_stats: MigrationStats::default(),
            last_updated: Instant::now(),
            metadata: HashMap::new(),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }
}

impl DatabaseStats {
    /// Create new DatabaseStats with initial values
    pub fn new(current_height: u64, total_height: u64) -> Self {
        let progress_percentage = 0.0; // It should be updated shortly after receiving calculated height

        Self {
            migration_stats: MigrationStats {
                current_height,
                total_height,
                progress_percentage,
                current_db_version: 0,
                target_db_version: 0,
            },
            last_updated: Instant::now(),
            metadata: HashMap::new(),
            timestamp: Utc::now().timestamp_millis() as u64,
        }
    }

    /// Set metadata key-value pair
    fn set_metadata(&mut self, key: MetadataKey, value: &MetadataValue) {
        self.metadata.insert(key, value.to_owned());
        if key == MetadataKey::MigrationVersion {
            if let MetadataValue::MigrationVersion(version) = value {
                self.migration_stats.current_db_version = *version;
            }
        }
    }

    /// Get metadata value by key
    pub fn get_metadata(&self, key: &MetadataKey) -> Option<&MetadataValue> {
        self.metadata.get(key)
    }

    /// Clear all metadata
    pub fn clear_metadata(&mut self) {
        self.metadata.clear();
    }

    pub fn set_migration_total_height(&mut self, total_height: u64) {
        self.migration_stats.total_height = total_height;
        self.timestamp = Utc::now().timestamp_millis() as u64;
    }

    /// Update progress with new current height
    pub fn update_migration_progress(&mut self, current_height: u64) {
        self.migration_stats.current_height = current_height;
        self.timestamp = Utc::now().timestamp_millis() as u64;

        // Update progress percentage
        self.migration_stats.progress_percentage = if self.migration_stats.total_height > 0 {
            (self.migration_stats.current_height as f64 / self.migration_stats.total_height as f64) * 100.0
        } else {
            0.0
        };
    }
}

/// StatsCollector for tracking database operation progress using tokio watch channels
pub struct LMDBStatsCollector {
    sender: watch::Sender<DatabaseStats>,
    receiver: watch::Receiver<DatabaseStats>,
    additional_senders: Arc<Mutex<Vec<watch::Sender<DatabaseStats>>>>,
}

impl LMDBStatsCollector {
    /// Create a new StatsCollector with initial stats
    pub fn new() -> Self {
        let initial_stats = DatabaseStats::default();
        let (sender, receiver) = watch::channel(initial_stats);

        Self {
            sender,
            receiver,
            additional_senders: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_current_timestamp(&self) -> u64 {
        Utc::now().timestamp_millis() as u64
    }

    fn update_db_stats(&self, stats: DatabaseStats) {
        if let Err(e) = self.sender.send(stats.clone()) {
            log::warn!("Failed to send stats update to primary channel: {}", e);
        }

        // Send to all additional subscribers
        if let Ok(mut senders) = self.additional_senders.lock() {
            senders.retain(|sender| {
                match sender.send(stats.clone()) {
                    Ok(_) => true, // Keep this sender
                    Err(e) => {
                        log::debug!("Removing failed stats subscriber: {}", e);
                        false // Remove this sender
                    },
                }
            });
        }
    }

    fn update_migration_stats(&self, stats: MigrationStats) {
        let new_stats = DatabaseStats {
            migration_stats: stats,
            timestamp: self.get_current_timestamp(),
            ..self.receiver.borrow().clone()
        };
        self.update_db_stats(new_stats);
    }

    /// Create a new StatsCollector with specified total height
    pub fn with_total_height(total_height: u64) -> Self {
        let initial_stats = DatabaseStats::new(0, total_height);
        let (sender, receiver) = watch::channel(initial_stats);

        Self {
            sender,
            receiver,
            additional_senders: Arc::new(Mutex::new(Vec::new())),
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

    /// Set the target database version for migration
    pub fn set_target_db_version(&self, target_version: u64) {
        let mut stats = self.receiver.borrow().clone();
        stats.migration_stats.target_db_version = target_version;
        self.update_db_stats(stats);
    }

    /// Set the current database version for migration
    pub fn set_current_db_version(&self, current_version: u64) {
        let mut stats = self.receiver.borrow().clone();
        stats.migration_stats.current_db_version = current_version;
        self.update_db_stats(stats);
    }

    /// Update the current progress
    pub fn update_migration_progress(&self, current_height: u64) {
        let mut stats = self.receiver.borrow().clone();
        stats.update_migration_progress(current_height);
        self.update_db_stats(stats);
    }

    /// Update metadata in the current stats
    pub fn update_metadata(&self, key: MetadataKey, value: &MetadataValue) {
        let mut stats = self.receiver.borrow().clone();
        stats.set_metadata(key, value);
        self.update_db_stats(stats);
    }

    /// Set the total height for progress calculation
    pub fn set_total_height(&self, total_height: u64) {
        let mut stats = self.receiver.borrow().clone().migration_stats;
        stats.total_height = total_height;
        self.update_migration_stats(stats);
    }

    /// Reset the stats collector with new parameters
    pub fn reset(&self, current_height: u64, total_height: u64) {
        let stats = DatabaseStats::new(current_height, total_height);
        self.update_db_stats(stats);
    }

    /// Subscribe an additional watch sender to receive updates
    /// Returns a receiver that will get all future updates
    pub fn subscribe_sender(&self) -> watch::Receiver<DatabaseStats> {
        let current_stats = self.receiver.borrow().clone();
        let (sender, receiver) = watch::channel(current_stats);

        if let Ok(mut senders) = self.additional_senders.lock() {
            senders.push(sender);
        }

        receiver
    }

    /// Add an existing watch sender to receive updates
    /// The sender will immediately receive the current stats
    pub fn add_sender(&self, sender: watch::Sender<DatabaseStats>) {
        let current_stats = self.receiver.borrow().clone();
        if let Err(e) = sender.send(current_stats) {
            log::warn!("Failed to send initial stats to added sender: {}", e);
        }
        if let Ok(mut senders) = self.additional_senders.lock() {
            senders.push(sender);
        }
    }

    /// Remove all additional senders
    pub fn clear_additional_senders(&self) {
        if let Ok(mut senders) = self.additional_senders.lock() {
            senders.clear();
        }
    }

    /// Get the count of additional subscribers
    pub fn additional_subscribers_count(&self) -> usize {
        self.additional_senders.lock().map(|senders| senders.len()).unwrap_or(0)
    }
}

impl Default for LMDBStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    #[test]
    fn test_database_stats_creation() {
        let stats = DatabaseStats::new(50, 100).migration_stats;
        assert_eq!(stats.current_height, 50);
        assert_eq!(stats.total_height, 100);
        assert_eq!(stats.progress_percentage, 0.0); // Initially 0.0, gets calculated on first update
    }

    #[test]
    fn test_database_stats_progress_update() {
        let mut stats = DatabaseStats::new(0, 100);

        // Simulate some time passing
        thread::sleep(Duration::from_millis(100));
        stats.update_migration_progress(25);

        let stats = stats.migration_stats;

        assert_eq!(stats.current_height, 25);
        assert_eq!(stats.progress_percentage, 25.0);
    }

    #[test]
    fn test_database_stats_completion() {
        let mut stats = DatabaseStats::new(0, 100);

        stats.update_migration_progress(100);

        let stats = stats.migration_stats;
        assert_eq!(stats.progress_percentage, 100.0);
    }

    #[test]
    fn test_stats_collector_creation() {
        let collector = LMDBStatsCollector::new();
        let stats = collector.current_stats().migration_stats;

        assert_eq!(stats.current_height, 0);
        assert_eq!(stats.total_height, 0);
        assert_eq!(stats.progress_percentage, 0.0);
    }

    #[test]
    fn test_stats_collector_with_total_height() {
        let collector = LMDBStatsCollector::with_total_height(1000);
        let stats = collector.current_stats().migration_stats;

        assert_eq!(stats.current_height, 0);
        assert_eq!(stats.total_height, 1000);
        assert_eq!(stats.progress_percentage, 0.0);
    }

    #[test]
    fn test_stats_collector_progress_update() {
        let collector = LMDBStatsCollector::with_total_height(100);

        collector.update_migration_progress(25);
        let stats = collector.current_stats().migration_stats;
        assert_eq!(stats.current_height, 25);
        assert_eq!(stats.progress_percentage, 25.0);

        collector.update_migration_progress(100);
        let final_stats = collector.current_stats().migration_stats;
        assert_eq!(final_stats.current_height, 100);
        assert_eq!(final_stats.total_height, 100);
        assert_eq!(final_stats.progress_percentage, 100.0);
    }

    #[test]
    fn test_stats_collector_subscription() {
        let collector = LMDBStatsCollector::with_total_height(100);
        let mut receiver = collector.subscribe();

        let stats = receiver.borrow_and_update().migration_stats.clone();
        // Initial state
        assert_eq!(stats.current_height, 0);

        // Update progress
        collector.update_migration_progress(50);

        // Check if receiver got the update
        let stats = receiver.borrow_and_update().migration_stats.clone();
        assert_eq!(stats.current_height, 50);
        assert_eq!(stats.progress_percentage, 50.0);
    }

    #[test]
    fn test_stats_collector_reset() {
        let collector = LMDBStatsCollector::with_total_height(100);
        let mut receiver = collector.subscribe();

        collector.update_migration_progress(50);
        collector.reset(0, 200);
        let stats = receiver.borrow_and_update().migration_stats.clone();
        assert_eq!(stats.current_height, 0);
        assert_eq!(stats.progress_percentage, 0.0);
    }

    #[test]
    fn test_stats_collector_additional_subscribers() {
        let collector = LMDBStatsCollector::with_total_height(100);

        // Subscribe additional receivers
        let mut receiver1 = collector.subscribe_sender();
        let mut receiver2 = collector.subscribe_sender();

        assert_eq!(collector.additional_subscribers_count(), 2);

        let stats1 = receiver1.borrow_and_update().migration_stats.clone();
        let stats2 = receiver2.borrow_and_update().migration_stats.clone();

        // Initial state should be received
        assert_eq!(stats1.current_height, 0);
        assert_eq!(stats2.current_height, 0);

        // Update progress
        collector.update_migration_progress(25);

        // All receivers should get the update
        let stats1 = receiver1.borrow_and_update().migration_stats.clone();
        let stats2 = receiver2.borrow_and_update().migration_stats.clone();

        assert_eq!(stats1.current_height, 25);
        assert_eq!(stats2.current_height, 25);
        assert_eq!(stats1.progress_percentage, 25.0);
        assert_eq!(stats2.progress_percentage, 25.0);
    }

    #[test]
    fn test_stats_collector_add_sender() {
        let collector = LMDBStatsCollector::with_total_height(100);

        // Set some initial progress
        collector.update_migration_progress(30);

        // Create external sender/receiver pair
        let initial_stats = DatabaseStats::default();
        let (sender, mut receiver) = watch::channel(initial_stats);

        // Add the sender
        collector.add_sender(sender);

        // Should immediately receive current stats
        {
            let stats = receiver.borrow_and_update().migration_stats.clone();
            assert_eq!(stats.current_height, 30);
            assert_eq!(stats.progress_percentage, 30.0);
        }

        // Update progress again
        collector.update_migration_progress(60);

        // External receiver should get the update
        {
            let updated_stats = receiver.borrow_and_update().migration_stats.clone();
            assert_eq!(updated_stats.current_height, 60);
            assert_eq!(updated_stats.progress_percentage, 60.0);
        }
    }

    #[test]
    fn test_stats_collector_clear_additional_senders() {
        let collector = LMDBStatsCollector::with_total_height(100);

        // Add some subscribers
        let _receiver1 = collector.subscribe_sender();
        let _receiver2 = collector.subscribe_sender();

        assert_eq!(collector.additional_subscribers_count(), 2);

        // Clear all additional senders
        collector.clear_additional_senders();

        assert_eq!(collector.additional_subscribers_count(), 0);
    }

    #[test]
    fn test_stats_collector_update_metadata() {
        let collector = LMDBStatsCollector::with_total_height(100);
        let mut receiver = collector.subscribe();

        // Update metadata
        let key = MetadataKey::ChainHeight;
        let value = MetadataValue::ChainHeight(50);
        collector.update_metadata(key, &value);

        // Check if receiver got the metadata update
        let stats = receiver.borrow_and_update();
        assert_eq!(stats.get_metadata(&key), Some(&value));
    }

    #[test]
    fn test_database_stats_metadata() {
        let mut stats = DatabaseStats::new(0, 100);

        // Set metadata
        let key = MetadataKey::ChainHeight;
        let value = MetadataValue::ChainHeight(75);
        stats.set_metadata(key, &value);

        // Get metadata
        assert_eq!(stats.get_metadata(&key), Some(&value));
        assert_eq!(stats.get_metadata(&MetadataKey::PruningHorizon), None);

        // Clear metadata
        stats.clear_metadata();
        assert_eq!(stats.get_metadata(&key), None);
    }

    #[test]
    fn test_stats_collector_metadata_propagation() {
        let collector = LMDBStatsCollector::with_total_height(100);

        // Subscribe additional receivers
        let mut receiver1 = collector.subscribe_sender();
        let mut receiver2 = collector.subscribe_sender();

        // Update metadata
        let key = MetadataKey::ChainHeight;
        let value = MetadataValue::ChainHeight(25);
        collector.update_metadata(key, &value);

        // All receivers should get the metadata update
        let stats1 = receiver1.borrow_and_update();
        let stats2 = receiver2.borrow_and_update();

        assert_eq!(stats1.get_metadata(&key), Some(&value));
        assert_eq!(stats2.get_metadata(&key), Some(&value));
    }
}
