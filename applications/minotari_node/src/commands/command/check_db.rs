//  Copyright 2022, The Tari Project
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

use std::{fmt::Display, time::Instant};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use tari_core::chain_storage::MetadataKey; // ensure this import path matches your crate layout
use tari_core::chain_storage::{BlockchainCheckStatus, CheckFailure};
use tokio::{
    task,
    time::{sleep, Duration},
};

use super::{CommandContext, HandleCommand};

/// Checks the blockchain database for missing blocks and headers, verify block contents, verify accumulated difficulty
#[derive(Clone, Copy, Debug, Parser)]
#[clap(
    about = "Checks the blockchain database for missing data / difficulty / full validation",
    disable_help_subcommand = true,
    term_width = 100,
    help_template = "{about-section}\n\nUSAGE:\n    {usage}\n\nOPTIONS:\n{options}\n"
)]
pub struct Args {
    /// What to check.
    #[clap(value_enum, short = 'm', long = "mode")]
    pub mode: Mode,

    /// Seconds between status polls (default 15s).
    #[clap(long, short = 'p', default_value_t = 15)]
    pub poll_seconds: u64,

    /// Milli-seconds 'breathing time' between consecutive checks - very short breathing time may starve other critical
    /// tasks (minimum 1 ms, maximum 1000ms, default 10ms).
    #[clap(long, short = 'b', default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..1000))]
    pub breathing_time_ms: u64,
}

/// What to check
#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq)]
pub enum Mode {
    /// Check that all blocks and headers can be read from genesis to tip
    LightChain,
    /// Check that all blocks and headers can be read from genesis to tip (with autocorrect)
    LightAutocorrect,
    /// Check that all blocks and headers can be read and validated stand-alone from genesis to tip
    FullChain,
    /// Check that all blocks and headers can be read and validated stand-alone from genesis to tip (with autocorrect)
    FullAutocorrect,
    /// Check that accumulated difficulty is correct from genesis to tip
    AccDiff,
    /// Check that accumulated difficulty is correct from genesis to tip (with autocorrect)
    AccDiffAutocorrect,
    /// Check that all blocks and headers can be read AND that accumulated difficulty is correct from genesis to tip
    /// (concurrent tasks)
    AllLight,
    /// Check that all blocks and headers can be read and validated stand-alone AND that accumulated difficulty is
    /// correct from genesis to tip (concurrent tasks)
    AllFull,
    /// Check that all blocks and headers can be read AND that accumulated difficulty is correct from genesis to tip
    /// (concurrent tasks with autocorrect)
    AllLightAutocorrect,
    /// Check that all blocks and headers can be read and validated stand-alone AND that accumulated difficulty is
    /// correct from genesis to tip (concurrent tasks with autocorrect)
    AllFullAutocorrect,
    /// Print the current status of the last check
    PrintStatus,
    /// Reset all counters so the next check can start from the genesis block (if not running)
    ResetCounters,
    /// Stop any currently running check
    Stop,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Mode::LightChain => "Light blockchain consistency",
            Mode::LightAutocorrect => "Light blockchain consistency (autocorrect)",
            Mode::FullChain => "Full blockchain consistency",
            Mode::FullAutocorrect => "Full blockchain consistency (autocorrect)",
            Mode::AccDiff => "Accumulated difficulty",
            Mode::AccDiffAutocorrect => "Accumulated difficulty (autocorrect)",
            Mode::AllLight => "Light blockchain consistency and accumulated difficulty",
            Mode::AllFull => "Full blockchain consistency and accumulated difficulty",
            Mode::AllLightAutocorrect => "Light blockchain consistency and accumulated difficulty (autocorrect)",
            Mode::AllFullAutocorrect => "Full blockchain consistency and accumulated difficulty (autocorrect)",
            Mode::PrintStatus => "Print the status",
            Mode::ResetCounters => "Reset the counters",
            Mode::Stop => "Stop",
        };
        write!(f, "{s}")
    }
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        println!("\n[check-db] args: {:?}", args);

        match args.mode {
            Mode::PrintStatus => {
                let acc_diff_status: Option<BlockchainCheckStatus> = self
                    .blockchain_db
                    .fetch_blockchain_check_status(MetadataKey::AccumulatedDataCheckStatus)
                    .await?;
                println!("\n[check-db] acc_diff status:\n  {:?}", acc_diff_status);
                let consistency_status: Option<BlockchainCheckStatus> = self
                    .blockchain_db
                    .fetch_blockchain_check_status(MetadataKey::BlockchainConsistencyCheckStatus)
                    .await?;
                println!("\n[check-db] chain status:\n  {:?}", consistency_status);
                println!();
            },
            Mode::ResetCounters => {
                println!("\n[check-db] Resetting database check counters...");
                self.blockchain_db.reset_check_db_counters().await?;
                println!("\n[check-db] Reset complete.\n");
            },
            Mode::Stop => {
                println!("\n[check-db] Stopping any current database check...");
                self.blockchain_db.stop_running_check_db_background_tasks().await?;
                println!("\n[check-db] Stopped.\n");
            },
            _ => {
                self.check_db(args).await?;
            },
        }

        Ok(())
    }
}

impl CommandContext {
    /// Run the requested check and poll status until it finishes or fails.
    pub async fn check_db(&mut self, args: Args) -> Result<(), Error> {
        // Kick off the appropriate background task by setting metadata + running the checker
        let auto_correct = matches!(
            args.mode,
            Mode::LightAutocorrect |
                Mode::FullAutocorrect |
                Mode::AccDiffAutocorrect |
                Mode::AllLightAutocorrect |
                Mode::AllFullAutocorrect
        );

        match args.mode {
            Mode::AccDiff | Mode::AccDiffAutocorrect => {
                self.blockchain_db
                    .request_accumulated_data_check(auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(MetadataKey::AccumulatedDataCheckStatus, args.poll_seconds, args.mode)
                    .await?;
            },
            Mode::LightChain | Mode::LightAutocorrect => {
                self.blockchain_db
                    .request_blockchain_consistency_check(false, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    MetadataKey::BlockchainConsistencyCheckStatus,
                    args.poll_seconds,
                    args.mode,
                )
                .await?;
            },
            Mode::FullChain | Mode::FullAutocorrect => {
                self.blockchain_db
                    .request_blockchain_consistency_check(true, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    MetadataKey::BlockchainConsistencyCheckStatus,
                    args.poll_seconds,
                    args.mode,
                )
                .await?;
            },
            Mode::AllLight | Mode::AllLightAutocorrect => {
                // Blockchain consistency
                self.blockchain_db
                    .request_blockchain_consistency_check(false, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    MetadataKey::BlockchainConsistencyCheckStatus,
                    args.poll_seconds,
                    args.mode,
                )
                .await?;
                // Accumulated data
                self.blockchain_db
                    .request_accumulated_data_check(auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(MetadataKey::AccumulatedDataCheckStatus, args.poll_seconds, args.mode)
                    .await?;
            },
            Mode::AllFull | Mode::AllFullAutocorrect => {
                // Blockchain consistency
                self.blockchain_db
                    .request_blockchain_consistency_check(true, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    MetadataKey::BlockchainConsistencyCheckStatus,
                    args.poll_seconds,
                    args.mode,
                )
                .await?;
                // Accumulated data
                self.blockchain_db
                    .request_accumulated_data_check(auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(MetadataKey::AccumulatedDataCheckStatus, args.poll_seconds, args.mode)
                    .await?;
            },
            _ => {
                return Err(anyhow!(
                    "[check-db] {}, unexpected mode when starting the check.",
                    args.mode
                ));
            },
        }

        Ok(())
    }

    /// Poll the given metadata (in the background) key until the background task reports completion or corruption.
    async fn poll_status(&mut self, key: MetadataKey, poll_s: u64, mode: Mode) -> Result<(), Error> {
        let mode = match (mode, key) {
            (Mode::AllLight, MetadataKey::AccumulatedDataCheckStatus) => Mode::AccDiff,
            (Mode::AllFull, MetadataKey::AccumulatedDataCheckStatus) => Mode::AccDiff,
            (Mode::AllLightAutocorrect, MetadataKey::AccumulatedDataCheckStatus) => Mode::AccDiffAutocorrect,
            (Mode::AllFullAutocorrect, MetadataKey::AccumulatedDataCheckStatus) => Mode::AccDiffAutocorrect,
            (Mode::AllLight, MetadataKey::BlockchainConsistencyCheckStatus) => Mode::LightChain,
            (Mode::AllFull, MetadataKey::BlockchainConsistencyCheckStatus) => Mode::FullChain,
            (Mode::AllLightAutocorrect, MetadataKey::BlockchainConsistencyCheckStatus) => Mode::LightAutocorrect,
            (Mode::AllFullAutocorrect, MetadataKey::BlockchainConsistencyCheckStatus) => Mode::FullAutocorrect,
            _ => mode,
        };

        let status = self
            .blockchain_db
            .fetch_blockchain_check_status(key)
            .await?
            .unwrap_or_default();
        println!(
            "\n[check-db] Starting '{}' check from #{} to tip (#{}), running({})",
            mode,
            status.last_check_height.unwrap_or(1),
            self.node_service.get_metadata().await?.best_block_height(),
            status.is_running(),
        );

        // Monitor the status of blockchain check task in the background
        let blockchain_db = self.blockchain_db.clone();
        task::spawn(async move {
            let start = Instant::now();
            loop {
                sleep(Duration::from_secs(poll_s)).await;

                let status = match blockchain_db.fetch_blockchain_check_status(key).await {
                    Ok(Some(status)) => status,
                    Ok(None) => {
                        if start.elapsed().as_secs() > Duration::from_secs(60).as_secs() {
                            println!("[check-db] {}, no status found after 60s, aborting!", mode);
                            break;
                        }
                        continue;
                    },
                    Err(e) => {
                        println!("[check-db] {}, error fetching status, cannot continue! ({})", mode, e);
                        break;
                    },
                };

                // Progress
                let (has_concluded, last_check_height, current_height) = status.checked_status();
                if status.is_running() {
                    let pct = if current_height > 0 {
                        last_check_height as f64 * 100.0 / current_height as f64
                    } else {
                        0.0
                    };
                    println!("[check-db] {mode}, progress: height {last_check_height}/{current_height} ~ {pct:.2}%");
                } else if let Some(last_failure) = status.last_failure.clone() {
                    print_failure_message(last_check_height, current_height, mode, &last_failure);
                    break;
                } else {
                    println!(
                        "[check-db] {mode}, processed up to height {last_check_height}/{current_height} - \
                         completed({has_concluded})."
                    );
                    break;
                }
            }

            println!("\n[check-db] {mode}, done\n");
        });
        Ok(())
    }
}

fn print_failure_message(last_check_height: u64, current_height: u64, mode: Mode, last_failure: &CheckFailure) {
    println!("{}", "-".repeat(110));
    if last_failure.corrupt_db {
        println!(
            "[check-db] {mode}, detected corruption at height {}: {}",
            last_check_height + 1,
            last_failure.error
        );
    } else {
        println!(
            "[check-db] {mode}, processed up to height {last_check_height}/{current_height}, but encountered an issue \
             that prevented it from completing: {}.",
            last_failure.error
        );
        println!("{}", "-".repeat(110));
        return;
    }

    match mode {
        Mode::LightChain | Mode::FullChain => {
            println!(
                "[check-db] {mode}, re-run with 'autocorrect' to rewind to {last_check_height} automatically, or run \
                 'rewind-blockchain {last_check_height}' manually.\n   If the counters are not reset, the check will \
                 resume from {last_check_height}."
            );
        },
        Mode::LightAutocorrect | Mode::FullAutocorrect => {
            println!(
                "[check-db] {mode}, blockchain was rewound to {last_check_height}. Node should automatically resync \
                 from there."
            );
        },
        Mode::AccDiff => {
            println!(
                "[check-db] {mode}, re-run with 'autocorrect' to rebuild the accumulated data. \n   If the counters \
                 are not reset, the check will resume from {last_check_height}."
            );
        },
        Mode::AccDiffAutocorrect => {
            println!("[check-db] {mode}, fixed to height {last_check_height}/{current_height}.");
        },
        _ => {
            // Nothing here
        },
    }
    println!("{}", "-".repeat(110));
}
