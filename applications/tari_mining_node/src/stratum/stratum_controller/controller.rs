// Copyright 2021. The Tari Project
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
//
use std::{
    self,
    sync::{mpsc, Arc, RwLock},
    thread,
    time::{Duration, SystemTime},
};

use log::*;

use crate::{
    stratum,
    stratum::{stratum_miner::miner::StratumMiner, stratum_statistics::stats, stratum_types as types},
};

pub const LOG_TARGET: &str = "tari_mining_node::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "tari_mining_node::logging::miner::stratum::controller";
const REPORTING_FREQUENCY: u64 = 20;

pub struct Controller {
    rx: mpsc::Receiver<types::miner_message::MinerMessage>,
    pub tx: mpsc::Sender<types::miner_message::MinerMessage>,
    client_tx: Option<mpsc::Sender<types::client_message::ClientMessage>>,
    current_height: u64,
    current_job_id: u64,
    current_blob: String,
    keep_alive_time: SystemTime,
    stats: Arc<RwLock<stats::Statistics>>,
    elapsed: SystemTime,
}

impl Controller {
    pub fn new(stats: Arc<RwLock<stats::Statistics>>) -> Result<Controller, String> {
        let (tx, rx) = mpsc::channel::<types::miner_message::MinerMessage>();
        Ok(Controller {
            rx,
            tx,
            client_tx: None,
            current_height: 0,
            current_job_id: 0,
            current_blob: "".to_string(),
            keep_alive_time: SystemTime::now(),
            stats,
            elapsed: SystemTime::now(),
        })
    }

    pub fn set_client_tx(&mut self, client_tx: mpsc::Sender<types::client_message::ClientMessage>) {
        self.client_tx = Some(client_tx);
    }

    fn display_stats(&mut self, elapsed: Duration) {
        let mut stats = self.stats.write().unwrap();
        info!(
            target: LOG_TARGET,
            "{}",
            "--------------- Mining Statistics ---------------".to_string()
        );
        info!(
            target: LOG_TARGET,
            "{}",
            format!("Number of solver threads: {}", stats.mining_stats.solvers)
        );
        if stats.mining_stats.solution_stats.found > 0 {
            info!(
                target: LOG_TARGET,
                "{}",
                format!(
                    "Estimated combined solver share rate: {:.1$} (S/s)",
                    stats.mining_stats.sols(),
                    5
                )
            );
        }
        info!(
            target: LOG_TARGET,
            "{}",
            format!(
                "Combined solver hash rate: {:.1$} (Mh/s)",
                stats.mining_stats.hash_rate(elapsed),
                5
            )
        );
        info!(
            target: LOG_TARGET,
            "{}",
            format!(
                "Shares found: {}, accepted: {}, rejected: {}",
                stats.mining_stats.solution_stats.found,
                stats.mining_stats.solution_stats.found - stats.mining_stats.solution_stats.rejected,
                stats.mining_stats.solution_stats.rejected
            )
        );
        info!(
            target: LOG_TARGET,
            "{}",
            "-------------------------------------------------".to_string()
        );
    }

    pub fn run(&mut self, mut miner: StratumMiner) -> Result<(), stratum::error::Error> {
        loop {
            if let Ok(report) = self.elapsed.elapsed() {
                if report.as_secs() >= REPORTING_FREQUENCY {
                    self.display_stats(report);
                    self.elapsed = SystemTime::now();
                }
            }
            while let Some(message) = self.rx.try_iter().next() {
                debug!(target: LOG_TARGET_FILE, "Miner received message: {:?}", message);
                let result: Result<(), stratum::error::Error> = match message {
                    types::miner_message::MinerMessage::ReceivedJob(height, job_id, diff, blob) => {
                        self.current_height = height;
                        self.current_job_id = job_id;
                        self.current_blob = blob;
                        miner.notify(
                            self.current_job_id,
                            self.current_height,
                            self.current_blob.clone(),
                            diff,
                        )
                    },
                    types::miner_message::MinerMessage::StopJob => {
                        debug!(target: LOG_TARGET_FILE, "Stopping jobs");
                        miner.pause_solvers();
                        Ok(())
                    },
                    types::miner_message::MinerMessage::ResumeJob => {
                        debug!(target: LOG_TARGET_FILE, "Resuming jobs");
                        miner.resume_solvers();
                        Ok(())
                    },
                    types::miner_message::MinerMessage::Shutdown => {
                        debug!(
                            target: LOG_TARGET_FILE,
                            "Stopping jobs and Shutting down mining controller"
                        );
                        miner.stop_solvers();
                        miner.wait_for_solver_shutdown();
                        Ok(())
                    },
                };
                if let Err(e) = result {
                    error!(target: LOG_TARGET, "Mining Controller Error {:?}", e);
                }
            }

            let solutions = miner.get_solutions();
            if let Some(ss) = solutions {
                let _ = self
                    .client_tx
                    .as_mut()
                    .unwrap()
                    .send(types::client_message::ClientMessage::FoundSolution(
                        ss.job_id, ss.hash, ss.nonce,
                    ));
                self.keep_alive_time = SystemTime::now();
                let mut stats = self.stats.write().unwrap();
                stats.mining_stats.solution_stats.found += 1;
            } else if self.keep_alive_time.elapsed().unwrap().as_secs() >= 30 {
                self.keep_alive_time = SystemTime::now();
                let _ = self
                    .client_tx
                    .as_mut()
                    .unwrap()
                    .send(types::client_message::ClientMessage::KeepAlive);
            }
            thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
