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
    io,
    path::{Path, PathBuf},
};

use log::*;
use tari_transaction_components::consensus::ConsensusConstants;

const LOG_TARGET: &str = "c::cs::consensus_tracker";

/// Tracks consensus constants to detect changes between node restarts
pub struct ConsensusConstantsTracker {
    storage_path: PathBuf,
}

#[allow(dead_code)]
impl ConsensusConstantsTracker {
    pub fn new(data_dir: &Path) -> Self {
        let mut storage_path = data_dir.to_path_buf();
        storage_path.push("consensus_constants.json");
        Self { storage_path }
    }

    /// Load the previously stored consensus constants.
    ///
    /// `None` means "nothing to compare against", and `check_for_changes` then skips the fork alarm entirely. That
    /// makes it important to distinguish the three ways of getting there. A missing file is the ordinary first-run
    /// case and is quiet. An unreadable file and an unparseable file are not: they silently disable the CRITICAL
    /// "constants changed and are already active" check on the very start where it is most likely to fire, so both
    /// are logged at error level. A file written by a binary that predates a newly added field is the usual cause
    /// of the unparseable case, which is why new `ConsensusConstants` fields carry `#[serde(default)]`.
    pub fn load_previous(&self) -> Option<Vec<ConsensusConstants>> {
        let content = match fs::read_to_string(&self.storage_path) {
            Ok(content) => content,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!(
                    target: LOG_TARGET,
                    "No previous consensus constants file found at {}",
                    self.storage_path.display()
                );
                return None;
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not read the consensus constants file at {}: {}. The check for consensus constants that \
                     changed while already active is DISABLED for this start.",
                    self.storage_path.display(),
                    e
                );
                return None;
            },
        };

        match serde_json::from_str(&content) {
            Ok(constants) => {
                debug!(
                    target: LOG_TARGET,
                    "Loaded previous consensus constants from {}",
                    self.storage_path.display()
                );
                Some(constants)
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Could not parse the consensus constants file at {}: {}. The check for consensus constants that \
                     changed while already active is DISABLED for this start; if this node was just upgraded, that \
                     is exactly the start where it would have fired. The file will now be overwritten with the \
                     current constants.",
                    self.storage_path.display(),
                    e
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

            // Check whether the rules that apply to any *already mined* height changed.
            //
            // Two things matter here. First, the lookup must be the one the node itself runs on
            // (`ConsensusConstants::active_at_height`, which `ConsensusManager::consensus_constants` also calls);
            // selecting by "greatest effective height" instead is only equivalent while the vector is sorted, and
            // disagreeing with the runtime means reporting a fork that is not happening, or staying quiet about one
            // that is.
            //
            // Second, it is not enough to compare only at `current_height`. Moving an activation height can leave the
            // same entry selected at the tip while changing which rules apply over the range the height moved across:
            // correcting a fork height from 200_000 to 210_000 leaves a node at 220_000 selecting the same entry, but
            // blocks 200_000..209_999 now validate under the older rules, so any resync or reorg over that range
            // would reject a chain the node previously accepted. Comparing at every effective height of either vector
            // catches that, while still staying quiet when an entry is merely re-declared at the height it was always
            // actually in force.
            let mut breakpoints: Vec<u64> = current_constants
                .iter()
                .chain(previous_constants.iter())
                .map(|cc| cc.effective_from_height())
                .filter(|height| *height <= current_height)
                .chain(std::iter::once(current_height))
                .collect();
            breakpoints.sort_unstable();
            breakpoints.dedup();

            for height in breakpoints {
                let current_active = ConsensusConstants::active_at_height(current_constants, height);
                let previous_active = ConsensusConstants::active_at_height(&previous_constants, height);
                if let (Some(current), Some(previous)) = (current_active, previous_active) &&
                    !current.has_same_rules_as(previous)
                {
                    return Err(format!(
                        "CRITICAL: Consensus constants have changed and the new constants are already \
                         active!\nCurrent height: {}\nThe rules applying at height {} changed: previously the \
                         constants effective from height {}, now the constants effective from height {}\nThis \
                         indicates a potential network fork or version mismatch.\nPlease verify you are running the \
                         correct version of the node for this network.",
                        current_height,
                        height,
                        previous.effective_from_height(),
                        current.effective_from_height()
                    ));
                }
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
    use tari_transaction_components::consensus::{ConsensusConstantsBuilder, ConsensusManager};
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

        // Third run with an extra entry that is not effective yet - the active constants are unchanged, so this
        // should pass. NOTE: this previously swapped in a different network's constants and expected a pass. That
        // only held because the popped Esmeralda entry was effective from height 181_000, so nothing at all was
        // active at height 0 and the check was skipped; a node whose active rules changed at height 0 got no
        // warning. `ConsensusConstantsBuilder` now normalises to height 0, so the case is expressed properly.
        let mut constants3 = constants1.clone();
        constants3.push(
            ConsensusConstantsBuilder::new(Network::Esmeralda)
                .with_effective_from_height(1_000_000)
                .build(),
        );
        let result = tracker.check_for_changes(&constants3, 0);
        assert!(result.is_ok(), "A not yet effective new entry should pass");

        // Fourth run where the constants active *right now* really did change - should fail
        let result = tracker.check_for_changes(&constants2, 0);
        assert!(result.is_err(), "Changed active constants should raise the alarm");
    }

    /// Reconstructs NextNet's constants vector as it was declared before `con_5` was moved from 5000 to 5500, i.e.
    /// what an upgrading node has persisted from its previous run.
    fn nextnet_constants_before_the_sort_fix() -> Vec<ConsensusConstants> {
        let mut previous = ConsensusConstants::for_network(Network::NextNet);
        while previous.last().expect("nextnet is never empty").effective_from_height() > 5_500 {
            previous.pop();
        }
        let con_5 = previous.pop().expect("nextnet has five entries");
        assert_eq!(con_5.effective_from_height(), 5_500);
        previous.push(
            ConsensusConstantsBuilder::new(Network::NextNet)
                .with_consensus_constants(con_5)
                .with_effective_from_height(5_000)
                .build(),
        );
        previous
    }

    /// Moving NextNet's `con_5` from 5000 to 5500 left the rules byte identical at every height, so upgrading must
    /// not print the "constants have changed and are already active" alarm. It used to, because the tracker selected
    /// the active entry by greatest effective height: the old unsorted vector had a unique max at 5500 (`con_4`)
    /// while the new one ties at 5500 and resolves to `con_5`, rendering as "from effective height 5500 to 5500".
    #[test]
    fn upgrading_across_the_nextnet_sort_fix_raises_no_alarm() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        let previous = nextnet_constants_before_the_sort_fix();
        let current = ConsensusConstants::for_network(Network::NextNet);
        assert_ne!(
            previous, current,
            "the vectors must really differ, or this proves nothing"
        );

        tracker
            .store_current(&previous)
            .expect("Failed to store previous constants");

        // Sweep the heights around and beyond the affected boundary
        for height in [
            0, 1_439, 1_440, 1_499, 1_500, 4_999, 5_000, 5_499, 5_500, 5_501, 6_000, 1_000_000,
        ] {
            assert!(
                tracker.check_for_changes(&current, height).is_ok(),
                "false fork alarm at height {height}"
            );
            // `check_for_changes` persists what it was given, so restore the pre-upgrade state for the next height
            tracker
                .store_current(&previous)
                .expect("Failed to restore previous constants");
        }
    }

    /// Moving an activation height after the chain has already passed it changes consensus for the range the height
    /// moved across, even though a node at the tip still selects the same entry. Comparing only at the tip would miss
    /// it; comparing at every breakpoint catches it.
    #[test]
    fn moving_an_activation_height_past_the_tip_raises_the_alarm() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        // A fork that was scheduled for 200_000 ...
        let base = ConsensusConstantsBuilder::new(Network::MainNet).build();
        let forked = ConsensusConstantsBuilder::new(Network::MainNet)
            .with_pow_backoff_cap(32)
            .with_difficulty_block_window(45)
            .with_effective_from_height(200_000)
            .build();
        let previous = vec![base.clone(), forked.clone()];

        // ... and is later "corrected" to 210_000, changing nothing at the tip but everything over 200_000..209_999
        let moved = ConsensusConstantsBuilder::new(Network::MainNet)
            .with_consensus_constants(forked)
            .with_effective_from_height(210_000)
            .build();
        let current = vec![base, moved];

        tracker.store_current(&previous).expect("Failed to store");
        let err = tracker
            .check_for_changes(&current, 220_000)
            .expect_err("moving an already passed activation height must raise the alarm");
        assert!(
            err.contains("The rules applying at height 200000 changed"),
            "the message must name the breakpoint where the rules diverge, got: {err}"
        );

        // The same edit made *before* the chain reaches either height is a legitimate reschedule and stays quiet
        tracker.store_current(&previous).expect("Failed to store");
        assert!(tracker.check_for_changes(&current, 199_999).is_ok());
    }

    /// The tracker must agree with the lookup the node actually runs on, otherwise it either cries fork when there
    /// is none or stays quiet when there is one.
    #[test]
    fn the_tracker_agrees_with_the_runtime_lookup() {
        for network in [
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
            Network::NextNet,
            Network::StageNet,
            Network::MainNet,
        ] {
            let constants = ConsensusConstants::for_network(network);
            let manager = ConsensusManager::builder(network).build();
            for height in [0u64, 1, 1_440, 1_500, 5_000, 5_500, 6_000, 126_000, u64::MAX] {
                assert_eq!(
                    ConsensusConstants::active_at_height(&constants, height).expect("never empty"),
                    manager.consensus_constants(height),
                    "{network} tracker and runtime disagree at height {height}"
                );
            }
        }
    }

    /// The first start after an upgrade is exactly when the "constants changed and are already active" alarm
    /// matters, and it is also the only start where the persisted file was written by a binary that did not know
    /// about the new fields. Without `#[serde(default)]` the parse fails, `load_previous` returns `None`, and the
    /// alarm is silently skipped. This reproduces that start: an on-disk file in the previous release's shape,
    /// against current constants, at a height the fork is already active at.
    #[test]
    fn an_upgrade_from_the_previous_releases_file_shape_still_raises_the_alarm() {
        // v5.6.0 is the last public release, and therefore the binary most nodes upgrade from.
        const FIELDS_ABSENT_FROM_THE_LAST_RELEASE: [&str; 3] = [
            "bipartite_cuckaroo_verification",
            "aux_chain_merkle_proof_depth_binding",
            "pow_backoff_cap",
        ];

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        let current = ConsensusConstants::for_network(Network::MainNet);
        let activation = current.last().expect("mainnet is never empty").effective_from_height();

        // The file the previous release would have written: the current shape minus the fields it never had.
        let mut json: serde_json::Value = serde_json::to_value(&current).expect("serializes");
        for entry in json.as_array_mut().expect("a vector of constants") {
            let map = entry.as_object_mut().expect("constants are a JSON object");
            for field in FIELDS_ABSENT_FROM_THE_LAST_RELEASE {
                assert!(map.remove(field).is_some(), "{field} is not in the serialized shape");
            }
        }
        std::fs::write(&tracker.storage_path, serde_json::to_string_pretty(&json).unwrap()).expect("write");

        let previous = tracker
            .load_previous()
            .expect("the previous release's file shape must still parse, or the alarm below is skipped");
        assert_ne!(previous, current, "the stripped file must really differ");

        let err = tracker
            .check_for_changes(&current, activation)
            .expect_err("the fork is already active, so the alarm must fire");
        assert!(
            err.contains("CRITICAL"),
            "expected the already-active alarm, got: {err}"
        );
    }

    /// A file that cannot be parsed at all must not be mistaken for a missing file. Both return `None`, but only
    /// one of them means "there was nothing to compare against".
    #[test]
    fn an_unparseable_file_is_not_treated_as_a_missing_one() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        assert!(tracker.load_previous().is_none(), "no file yet");

        std::fs::write(&tracker.storage_path, "{ this is not consensus constants").expect("write");
        assert!(tracker.load_previous().is_none(), "garbage must not parse");

        // ...and the tracker recovers: the next run overwrites the file with something readable.
        let current = ConsensusConstants::for_network(Network::MainNet);
        tracker.check_for_changes(&current, 0).expect("must not hard fail");
        assert_eq!(tracker.load_previous().as_ref(), Some(&current));
    }

    #[test]
    fn test_tracked_consensus_constants_serialization() {
        let constants = ConsensusConstantsBuilder::new(Network::Esmeralda).build();

        // Test serialization
        let json = serde_json::to_string(&constants).expect("Should serialize");
        let deserialized: ConsensusConstants = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(constants, deserialized);
    }

    /// The alarm is about the rules a node is running *now*, so it must key off which entry is active at the current
    /// height rather than off the vector as a whole.
    #[test]
    fn test_consensus_constants_effective_height_detection() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tracker = ConsensusConstantsTracker::new(temp_dir.path());

        let base = ConsensusConstantsBuilder::new(Network::LocalNet).build();
        let constants_v1 = vec![base.clone()];
        // A genuinely different rule set, scheduled for a future height
        let future_change = ConsensusConstantsBuilder::new(Network::Esmeralda)
            .with_effective_from_height(1_000)
            .build();
        let constants_v2 = vec![base, future_change];

        let result = tracker.check_for_changes(&constants_v1, 0);
        assert!(result.is_ok(), "First run should pass");

        // Below the new entry's effective height the active constants are unchanged
        let result = tracker.check_for_changes(&constants_v2, 999);
        assert!(
            result.is_ok(),
            "Should pass while the new constants are not yet effective"
        );

        // At and above it they are already active, which is exactly what this alarm is for
        tracker.store_current(&constants_v1).expect("Failed to store");
        let result = tracker.check_for_changes(&constants_v2, 1_000);
        assert!(result.is_err(), "Should fail when the new constants are already active");
    }
}
