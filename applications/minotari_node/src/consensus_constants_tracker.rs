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
    fs,
    path::{Path, PathBuf},
};

use log::*;
use tari_transaction_components::consensus::ConsensusConstants;

const LOG_TARGET: &str = "c::cs::consensus_tracker";

/// Tracks consensus constants to detect changes between node restarts
pub struct ConsensusConstantsTracker {
    storage_path: PathBuf,
}

impl ConsensusConstantsTracker {
    pub fn new(data_dir: &Path) -> Self {
        let mut storage_path = data_dir.to_path_buf();
        storage_path.push("consensus_constants.json");
        Self { storage_path }
    }

    /// Load the previously stored consensus constants
    pub fn load_previous(&self) -> Option<Vec<ConsensusConstants>> {
        match fs::read_to_string(&self.storage_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(constants) => {
                    debug!(
                        target: LOG_TARGET,
                        "Loaded previous consensus constants from {}",
                        self.storage_path.display()
                    );
                    Some(constants)
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to parse consensus constants file: {}",
                        e
                    );
                    None
                },
            },
            Err(_) => {
                debug!(
                    target: LOG_TARGET,
                    "No previous consensus constants file found at {}",
                    self.storage_path.display()
                );
                None
            },
        }
    }

    /// Store the current consensus constants
    pub fn store_current(&self, consensus_constants: &[ConsensusConstants]) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_json::to_string_pretty(consensus_constants)?;

        // Ensure the directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.storage_path, content)?;
        debug!(
            target: LOG_TARGET,
            "Stored consensus constants to {}",
            self.storage_path.display()
        );
        Ok(())
    }

    /// Check if consensus constants have changed and if any new constants are already active
    pub fn check_for_changes(
        &self,
        current_constants: &[ConsensusConstants],
        current_height: u64,
    ) -> Result<(), String> {
        if let Some(previous_constants) = self.load_previous() &&
            current_constants != previous_constants
        {
            info!(
                target: LOG_TARGET,
                "Consensus constants have changed since last startup"
            );

            // Check if any new consensus constants are already active at current height
            let current_active = current_constants
                .iter()
                .filter(|cc| cc.effective_from_height() <= current_height)
                .max_by_key(|cc| cc.effective_from_height());

            let previous_active = previous_constants
                .iter()
                .filter(|cc| cc.effective_from_height() <= current_height)
                .max_by_key(|cc| cc.effective_from_height());

            if let (Some(current), Some(previous)) = (current_active, previous_active) &&
                current != previous
            {
                return Err(format!(
                    "CRITICAL: Consensus constants have changed and the new constants are already active!\nCurrent \
                     height: {}\nActive consensus constants changed from effective height {} to {}\nThis indicates a \
                     potential network fork or version mismatch.\nPlease verify you are running the correct version \
                     of the node for this network.",
                    current_height,
                    previous.effective_from_height(),
                    current.effective_from_height()
                ));
            }
        }

        // Store current constants for next time
        if let Err(e) = self.store_current(current_constants) {
            warn!(
                target: LOG_TARGET,
                "Failed to store consensus constants: {}",
                e
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tari_common::configuration::Network;
    use tari_transaction_components::consensus::ConsensusConstantsBuilder;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_consensus_constants_tracker() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        // Create some mock consensus constants
        let constants1 = vec![ConsensusConstantsBuilder::new(Network::Esmeralda).build()];

        let constants2 = vec![ConsensusConstantsBuilder::new(Network::LocalNet).build()];

        // First run - no previous constants, should pass
        let result = tracker.check_for_changes(&constants1, 0);
        assert!(result.is_ok(), "First run should pass");

        // Verify the file was created
        assert!(tracker.storage_path.exists(), "Storage file should exist");

        // Second run with same constants - should pass
        let result = tracker.check_for_changes(&constants1, 0);
        assert!(result.is_ok(), "Same constants should pass");

        // Third run with different constants but same active height - should pass
        let result = tracker.check_for_changes(&constants2, 0);
        assert!(result.is_ok(), "Different constants but same active should pass");
    }

    #[test]
    fn test_tracked_consensus_constants_serialization() {
        let constants = ConsensusConstantsBuilder::new(Network::Esmeralda).build();

        // Test serialization
        let json = serde_json::to_string(&constants).expect("Should serialize");
        let deserialized: ConsensusConstants = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(constants, deserialized);
    }

    #[test]
    fn test_consensus_constants_effective_height_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        // Create constants with different networks to simulate different values
        let constants_v1 = vec![ConsensusConstantsBuilder::new(Network::LocalNet).build()];

        let constants_v2 = vec![ConsensusConstantsBuilder::new(Network::Esmeralda).build()];

        // First run with v1 constants at height 0
        let result = tracker.check_for_changes(&constants_v1, 0);
        assert!(result.is_ok(), "First run should pass");

        // Second run with v2 constants but still at height 0 (same active constants)
        let result = tracker.check_for_changes(&constants_v2, 0);
        assert!(
            result.is_ok(),
            "Should pass when active constants haven't changed at height 0"
        );
    }
}
