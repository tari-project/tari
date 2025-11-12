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
use tari_core::chain_storage::{
    async_db::AsyncBlockchainDb,
    BlockchainCheckStatus,
    ChainStorageError,
    CheckFailure,
    LMDBDatabase,
};
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
    #[clap(long, short = 'b', default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=1000))]
    pub breathing_time_ms: u64,

    /// Option to not clear counters when error detected or user requested a stop (Default: false).
    /// Note: This is a long-winded-option-to-write intended for expert use.
    #[clap(long, default_value = "false", action = clap::ArgAction::SetTrue)]
    pub do_not_clear_counters_on_error_or_stop: bool,
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
                let acc_diff_status = fetch_check_status(CheckType::AccumulatedData, &self.blockchain_db).await?;
                println!("\n[check-db] acc_diff status:\n  {:?}", acc_diff_status);
                let consistency_status = fetch_check_status(CheckType::ChainConsistency, &self.blockchain_db).await?;
                println!("\n[check-db] chain status:\n  {:?}", consistency_status);
                println!();
            },
            Mode::ResetCounters => {
                println!("\n[check-db] Resetting database check counters...");
                self.blockchain_db.reset_accumulated_data_check_db_counters().await?;
                self.blockchain_db
                    .reset_blockchain_consistency_check_db_counters()
                    .await?;
                println!("\n[check-db] Reset complete.\n");
            },
            Mode::Stop => {
                println!("\n[check-db] Stopping any current database check...");
                if let Err(e) = self.blockchain_db.stop_running_accumulated_data_check_task().await {
                    println!("[check-db] {}, error stopping check task: {}", args.mode, e);
                }
                if let Err(e) = self
                    .blockchain_db
                    .stop_running_blockchain_consistency_check_task()
                    .await
                {
                    println!("[check-db] {}, error stopping check task: {}", args.mode, e);
                }
                println!("\n[check-db] Stopped.\n");
            },
            _ => {
                self.check_db(args).await?;
            },
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum CheckType {
    AccumulatedData,
    ChainConsistency,
}

impl Display for CheckType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CheckType::AccumulatedData => "AccumulatedData",
            CheckType::ChainConsistency => "ChainConsistency",
        };
        write!(f, "{s}")
    }
}

impl CheckType {
    pub fn try_from(mode: Mode) -> Result<Vec<CheckType>, Error> {
        match mode {
            Mode::AccDiff | Mode::AccDiffAutocorrect => Ok(vec![CheckType::AccumulatedData]),
            Mode::LightChain | Mode::LightAutocorrect | Mode::FullChain | Mode::FullAutocorrect => {
                Ok(vec![CheckType::ChainConsistency])
            },
            Mode::AllLight | Mode::AllFull | Mode::AllLightAutocorrect | Mode::AllFullAutocorrect => {
                Ok(vec![CheckType::ChainConsistency, CheckType::AccumulatedData])
            },
            _ => Err(anyhow!(
                "[check-db] {}, unexpected mode when determining check type.",
                mode
            )),
        }
    }
}

impl CommandContext {
    /// Run the requested check and poll status until it finishes or fails.
    pub async fn check_db(&mut self, args: Args) -> Result<(), Error> {
        // Prompt user to confirm using existing status
        let check_types = CheckType::try_from(args.mode)?;
        for check_type in check_types {
            self.prompt_confirm_initial_status(check_type, args.mode, args.do_not_clear_counters_on_error_or_stop)
                .await?;
        }

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
                self.poll_status(
                    CheckType::AccumulatedData,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
                .await?;
            },
            Mode::LightChain | Mode::LightAutocorrect => {
                self.blockchain_db
                    .request_blockchain_consistency_check(false, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    CheckType::ChainConsistency,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
                .await?;
            },
            Mode::FullChain | Mode::FullAutocorrect => {
                self.blockchain_db
                    .request_blockchain_consistency_check(true, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    CheckType::ChainConsistency,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
                .await?;
            },
            Mode::AllLight | Mode::AllLightAutocorrect => {
                // Blockchain consistency
                self.blockchain_db
                    .request_blockchain_consistency_check(false, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    CheckType::ChainConsistency,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
                .await?;
                // Accumulated data
                self.blockchain_db
                    .request_accumulated_data_check(auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    CheckType::AccumulatedData,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
                .await?;
            },
            Mode::AllFull | Mode::AllFullAutocorrect => {
                // Blockchain consistency
                self.blockchain_db
                    .request_blockchain_consistency_check(true, auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    CheckType::ChainConsistency,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
                .await?;
                // Accumulated data
                self.blockchain_db
                    .request_accumulated_data_check(auto_correct, args.breathing_time_ms)
                    .await?;
                self.poll_status(
                    CheckType::AccumulatedData,
                    args.poll_seconds,
                    args.mode,
                    args.do_not_clear_counters_on_error_or_stop,
                )
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
    async fn poll_status(
        &mut self,
        check_type: CheckType,
        poll_s: u64,
        mode: Mode,
        do_not_clear_counters: bool,
    ) -> Result<(), Error> {
        let mode = match (mode, check_type) {
            (Mode::AllLight, CheckType::AccumulatedData) => Mode::AccDiff,
            (Mode::AllFull, CheckType::AccumulatedData) => Mode::AccDiff,
            (Mode::AllLightAutocorrect, CheckType::AccumulatedData) => Mode::AccDiffAutocorrect,
            (Mode::AllFullAutocorrect, CheckType::AccumulatedData) => Mode::AccDiffAutocorrect,
            (Mode::AllLight, CheckType::ChainConsistency) => Mode::LightChain,
            (Mode::AllFull, CheckType::ChainConsistency) => Mode::FullChain,
            (Mode::AllLightAutocorrect, CheckType::ChainConsistency) => Mode::LightAutocorrect,
            (Mode::AllFullAutocorrect, CheckType::ChainConsistency) => Mode::FullAutocorrect,
            _ => mode,
        };

        let status = fetch_check_status(check_type, &self.blockchain_db)
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
        let mut shutdown = self.shutdown.to_signal().clone();
        task::spawn(async move {
            let start = Instant::now();
            loop {
                tokio::select! {
                    _ = sleep(Duration::from_secs(poll_s)) => {
                        let status = match fetch_check_status(check_type, &blockchain_db).await {
                            Ok(Some(status)) => status,
                            Ok(None) => {
                                if start.elapsed().as_secs() > Duration::from_secs(60).as_secs() {
                                    println!("[check-db] {}, no status found after 60s, aborting!", mode);
                                    stop_and_clear_counters(&blockchain_db, check_type, mode, do_not_clear_counters)
                                        .await;
                                    break;
                                }
                                continue;
                            },
                            Err(e) => {
                                println!("[check-db] {}, error fetching status, cannot continue! ({})", mode, e);
                                stop_and_clear_counters(&blockchain_db, check_type, mode, do_not_clear_counters).await;
                                break;
                            },
                        };

                        // Progress
                        let (has_concluded, last_check_height, current_height) = status.checked_status();
                        if status.is_running() {
                            let pct = if current_height > 0 {
                                (last_check_height as f64 * 100.0 / current_height as f64).clamp(0.0, 100.0)
                            } else {
                                0.0
                            };
                            println!(
                                "[check-db] {mode}, progress: height {last_check_height}/{current_height} ~ {pct:.2}%"
                            );
                        } else if let Some(last_failure) = status.last_failure.clone() {
                            print_failure_message(last_check_height, current_height, mode, &last_failure);
                            stop_and_clear_counters(&blockchain_db, check_type, mode, do_not_clear_counters).await;
                            break;
                        } else {
                            println!(
                                "[check-db] {mode}, processed up to height {last_check_height}/{current_height} - \
                                 completed({has_concluded})."
                            );
                            let _unused = stop_and_clear_counters(&blockchain_db, check_type, mode, do_not_clear_counters).await;
                            break;
                        }
                    }
                    _ = shutdown.wait() => {
                        println!("[check-db] {mode}, cancelled by shutdown.");
                        break
                    },
                }
            }

            println!("\n[check-db] {mode}, done\n");
        });
        Ok(())
    }

    async fn prompt_confirm_initial_status(
        &self,
        check_type: CheckType,
        mode: Mode,
        do_not_clear_counters: bool,
    ) -> Result<(), ChainStorageError> {
        let status_res = fetch_check_status(check_type, &self.blockchain_db)
            .await
            .inspect_err(|e| {
                println!("[check-db] {}, error fetching status: {}", mode, e);
            })?;
        if let Some(val) = status_res {
            if val != BlockchainCheckStatus::default() && do_not_clear_counters {
                // Prompt user to confirm using existing status
                println!(
                    "\n[check-db] '{}'/'{}' check: found existing status - height: #{}, running: {}, \
                     do_not_clear_counters: {}.\n Do you want to resume from this height? (y/n): ",
                    mode,
                    check_type,
                    val.last_check_height.unwrap_or(1),
                    val.is_running(),
                    do_not_clear_counters,
                );
                // Do not block the async runtime
                let input = task::spawn_blocking(|| {
                    use std::io::{stdin, stdout, Write};
                    let mut input = String::new();
                    print!("> ");
                    let _unused = stdout().flush();
                    let _unused = stdin().read_line(&mut input);
                    input
                })
                .await
                .unwrap_or_default()
                .trim()
                .to_lowercase();
                if input != "y" && input != "yes" {
                    // Reset status
                    println!("\n[check-db] '{}' check: resetting previous counters...", mode);
                    stop_and_clear_counters(&self.blockchain_db, check_type, mode, false).await;
                }
            } else {
                println!("\n[check-db] '{}' check: resetting previous counters...", mode);
                stop_and_clear_counters(&self.blockchain_db, check_type, mode, false).await;
            }
        }
        Ok(())
    }
}

async fn fetch_check_status(
    check_type: CheckType,
    blockchain_db: &AsyncBlockchainDb<LMDBDatabase>,
) -> Result<Option<BlockchainCheckStatus>, ChainStorageError> {
    match check_type {
        CheckType::AccumulatedData => blockchain_db.fetch_accumulated_data_check_status().await,
        CheckType::ChainConsistency => blockchain_db.fetch_blockchain_consistency_check_status().await,
    }
}

async fn stop_and_clear_counters(
    blockchain_db: &AsyncBlockchainDb<LMDBDatabase>,
    check_type: CheckType,
    mode: Mode,
    do_not_clear_counters: bool,
) {
    let stop_running = match check_type {
        CheckType::AccumulatedData => blockchain_db.stop_running_accumulated_data_check_task().await,
        CheckType::ChainConsistency => blockchain_db.stop_running_blockchain_consistency_check_task().await,
    };
    if let Err(e) = stop_running {
        println!("[check-db] {}, error stopping background check task: {}", mode, e);
    }

    if do_not_clear_counters {
        return;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    let clear_counters = match check_type {
        CheckType::AccumulatedData => blockchain_db.reset_accumulated_data_check_db_counters().await,
        CheckType::ChainConsistency => blockchain_db.reset_blockchain_consistency_check_db_counters().await,
    };
    if let Err(e) = clear_counters {
        println!("[check-db] {}, error resetting counters: {}", mode, e);
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
