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
use crate::{
    config::MinerConfig,
    difficulty::BlockHeaderSha3,
    stratum,
    stratum::{
        stratum_miner::{
            control_message::ControlMessage,
            job_shared_data::{JobSharedData, JobSharedDataType},
            solution::Solution,
            solver_instance::SolverInstance,
        },
        stratum_statistics::stats::Statistics,
    },
};
use log::*;
use std::{
    convert::TryFrom,
    sync::{mpsc, Arc, RwLock},
    thread,
    time,
    time::{Duration, SystemTime},
};
use tari_core::blocks::BlockHeader;
use tari_utilities::{hex::Hex, Hashable};

pub const LOG_TARGET: &str = "tari_mining_node::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "tari_mining_node::logging::miner::stratum::controller";

fn calculate_sols(elapsed: Duration) -> f64 {
    1.0 / ((elapsed.as_secs() * 1_000_000_000 + elapsed.subsec_nanos() as u64) as f64 / 1_000_000_000.0)
}

pub struct StratumMiner {
    config: MinerConfig,
    pub shared_data: Arc<RwLock<JobSharedData>>,
    control_txs: Vec<mpsc::Sender<ControlMessage>>,
    solver_loop_txs: Vec<mpsc::Sender<ControlMessage>>,
    solver_stopped_rxs: Vec<mpsc::Receiver<ControlMessage>>,
    stats: Arc<RwLock<Statistics>>,
}

impl StratumMiner {
    pub fn new(config: MinerConfig, stats: Arc<RwLock<Statistics>>) -> StratumMiner {
        let threads = config.num_mining_threads;
        StratumMiner {
            config,
            shared_data: Arc::new(RwLock::new(JobSharedData::new(threads))),
            control_txs: vec![],
            solver_loop_txs: vec![],
            solver_stopped_rxs: vec![],
            stats,
        }
    }

    fn solver_thread(
        mut solver: SolverInstance,
        instance: usize,
        shared_data: JobSharedDataType,
        control_rx: mpsc::Receiver<ControlMessage>,
        solver_loop_rx: mpsc::Receiver<ControlMessage>,
        solver_stopped_tx: mpsc::Sender<ControlMessage>,
        statistics: Arc<RwLock<Statistics>>,
    ) {
        let stop_handle = thread::spawn(move || loop {
            while let Some(message) = control_rx.iter().next() {
                match message {
                    ControlMessage::Stop => {
                        debug!(target: LOG_TARGET_FILE, "Stopping Solvers");
                        return;
                    },
                    ControlMessage::Pause => {
                        debug!(target: LOG_TARGET_FILE, "Pausing Solvers");
                    },
                    ControlMessage::Resume => {
                        debug!(target: LOG_TARGET_FILE, "Resuming Solvers");
                    },
                    _ => {},
                };
            }
        });

        let mut paused = true;
        let mut timer = SystemTime::now();
        loop {
            if let Some(message) = solver_loop_rx.try_iter().next() {
                debug!(
                    target: LOG_TARGET_FILE,
                    "solver_thread - solver_loop_rx got msg: {:?}", message
                );
                match message {
                    ControlMessage::Stop => break,
                    ControlMessage::Pause => {
                        paused = true;
                        solver.solver_reset = true;
                    },
                    ControlMessage::Resume => {
                        paused = false;
                        timer = SystemTime::now();
                    },
                    _ => {},
                }
            }

            if paused {
                thread::sleep(time::Duration::from_micros(100));
                continue;
            }

            let header = { shared_data.read().unwrap().header.clone() };
            match header {
                Some(header) => {
                    let height = { shared_data.read().unwrap().height };
                    let job_id = { shared_data.read().unwrap().job_id };
                    let target_difficulty = { shared_data.read().unwrap().difficulty };
                    let mut hasher = BlockHeaderSha3::new(tari_app_grpc::tari_rpc::BlockHeader::from(header)).unwrap();

                    if solver.solver_reset {
                        hasher.random_nonce();
                        solver.current_nonce = hasher.nonce;
                        solver.solver_reset = false;
                    } else {
                        hasher.nonce = solver.current_nonce;
                        hasher.inc_nonce();
                        solver.current_nonce = hasher.nonce;
                    }
                    let mut stats = statistics.write().unwrap();
                    let difficulty = hasher.difficulty();
                    stats.mining_stats.add_hash();
                    if difficulty >= target_difficulty {
                        let block_header: BlockHeader = BlockHeader::try_from(hasher.into_header()).unwrap();
                        info!(
                            target: LOG_TARGET,
                            "Miner found share with hash {}, nonce {} and difficulty {:?}",
                            block_header.hash().to_hex(),
                            solver.current_nonce,
                            difficulty
                        );
                        debug!(
                            target: LOG_TARGET_FILE,
                            "Miner found share with hash {}, difficulty {:?} and data {:?}",
                            block_header.hash().to_hex(),
                            difficulty,
                            block_header
                        );

                        let still_valid = { height == shared_data.read().unwrap().height };
                        if still_valid {
                            let mut s = shared_data.write().unwrap();
                            s.solutions.push(Solution {
                                height,
                                job_id,
                                difficulty: target_difficulty,
                                hash: block_header.hash().to_hex(),
                                nonce: block_header.nonce,
                            });
                            if let Ok(elapsed) = timer.elapsed() {
                                let sols = calculate_sols(elapsed);
                                stats.mining_stats.add_sols(sols);
                            }
                            timer = SystemTime::now();
                        }
                    }
                    solver.solutions = Solution::default();
                },
                None => {
                    continue;
                },
            }
        }

        let _ = stop_handle.join();
        let _ = solver_stopped_tx.send(ControlMessage::SolverStopped(instance));
    }

    pub fn start_solvers(&mut self) -> Result<(), stratum::error::Error> {
        let mut stats = self.stats.write().unwrap();
        stats.mining_stats.solvers = self.config.num_mining_threads;
        let num_solvers = self.config.num_mining_threads;
        info!(target: LOG_TARGET, "Spawning {} solvers", num_solvers);
        let mut solvers = Vec::with_capacity(num_solvers);
        while solvers.len() < solvers.capacity() {
            solvers.push(SolverInstance::new()?);
        }
        for (i, s) in solvers.into_iter().enumerate() {
            let sd = self.shared_data.clone();
            let (control_tx, control_rx) = mpsc::channel::<ControlMessage>();
            let (solver_tx, solver_rx) = mpsc::channel::<ControlMessage>();
            let (solver_stopped_tx, solver_stopped_rx) = mpsc::channel::<ControlMessage>();
            self.control_txs.push(control_tx);
            self.solver_loop_txs.push(solver_tx);
            self.solver_stopped_rxs.push(solver_stopped_rx);
            let stats = self.stats.clone();
            thread::spawn(move || {
                StratumMiner::solver_thread(s, i, sd, control_rx, solver_rx, solver_stopped_tx, stats);
            });
        }
        Ok(())
    }

    pub fn notify(
        &mut self,
        job_id: u64,
        height: u64,
        blob: String,
        difficulty: u64,
    ) -> Result<(), stratum::error::Error> {
        let header_hex =
            hex::decode(blob).map_err(|_| stratum::error::Error::Json("Blob is not a valid hex value".to_string()))?;
        let header: BlockHeader = serde_json::from_str(&String::from_utf8_lossy(&header_hex).to_string())?;

        let mut sd = self.shared_data.write().unwrap();
        let paused = if height != sd.height {
            // stop/pause any existing jobs if job is for a new
            // height
            self.pause_solvers();
            true
        } else {
            false
        };

        sd.job_id = job_id;
        sd.height = height;
        sd.difficulty = difficulty;
        sd.header = Some(header);
        if paused {
            info!(
                target: LOG_TARGET,
                "Hashing in progress... height: {}, target difficulty: {}", height, difficulty
            );
            self.resume_solvers();
        }
        Ok(())
    }

    pub fn get_solutions(&self) -> Option<Solution> {
        {
            let mut s = self.shared_data.write().unwrap();
            if !s.solutions.is_empty() {
                let sol = s.solutions.pop().unwrap();
                return Some(sol);
            }
        }
        None
    }

    pub fn stop_solvers(&self) {
        for t in self.control_txs.iter() {
            let _ = t.send(ControlMessage::Stop);
        }
        for t in self.solver_loop_txs.iter() {
            let _ = t.send(ControlMessage::Stop);
        }
        debug!(target: LOG_TARGET_FILE, "Stop message sent");
    }

    pub fn pause_solvers(&self) {
        for t in self.control_txs.iter() {
            let _ = t.send(ControlMessage::Pause);
        }
        for t in self.solver_loop_txs.iter() {
            let _ = t.send(ControlMessage::Pause);
        }
        debug!(target: LOG_TARGET_FILE, "Pause message sent");
    }

    pub fn resume_solvers(&self) {
        for t in self.control_txs.iter() {
            let _ = t.send(ControlMessage::Resume);
        }
        for t in self.solver_loop_txs.iter() {
            let _ = t.send(ControlMessage::Resume);
        }
        debug!(target: LOG_TARGET_FILE, "Resume message sent");
    }

    pub fn wait_for_solver_shutdown(&self) {
        for r in self.solver_stopped_rxs.iter() {
            if let Some(ControlMessage::SolverStopped(i)) = r.iter().next() {
                debug!(target: LOG_TARGET_FILE, "Solver stopped: {}", i);
            }
        }
    }
}
