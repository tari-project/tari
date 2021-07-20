use crate::{config::MinerConfig, difficulty::BlockHeaderSha3, stratum};
use log::*;
use std::{
    convert::TryFrom,
    sync::{mpsc, Arc, RwLock},
    thread,
    time,
};
use tari_core::{
    blocks::BlockHeader,
    crypto::tari_utilities::{hex::Hex, Hashable},
};

#[derive(Clone)]
pub struct Solution {
    pub height: u64,
    pub job_id: u64,
    pub difficulty: u64,
    pub hash: String,
    pub nonce: u64,
}

impl Default for Solution {
    fn default() -> Solution {
        Solution {
            height: 0,
            job_id: 0,
            difficulty: 0,
            hash: "".to_string(),
            nonce: 0,
        }
    }
}

pub type JobSharedDataType = Arc<RwLock<JobSharedData>>;

pub struct JobSharedData {
    pub job_id: u64,
    pub height: u64,
    pub header: Option<BlockHeader>,
    pub difficulty: u64,
    pub solutions: Vec<Solution>,
}

impl Default for JobSharedData {
    fn default() -> JobSharedData {
        JobSharedData {
            job_id: 0,
            height: 0,
            header: None,
            difficulty: 0,
            solutions: Vec::new(),
        }
    }
}

impl JobSharedData {
    pub fn new(_num_solvers: usize) -> JobSharedData {
        JobSharedData {
            job_id: 0,
            height: 0,
            header: None,
            difficulty: 1,
            solutions: Vec::new(),
        }
    }
}

pub struct SolverInstance {
    pub solutions: Solution,
}

impl SolverInstance {
    pub fn new() -> Result<SolverInstance, stratum::Error> {
        Ok(SolverInstance {
            solutions: Solution::default(),
        })
    }
}

#[derive(Debug)]
enum ControlMessage {
    Stop,
    Pause,
    Resume,
    SolverStopped(usize),
}

pub struct StratumMiner {
    _config: MinerConfig,
    pub shared_data: Arc<RwLock<JobSharedData>>,
    control_txs: Vec<mpsc::Sender<ControlMessage>>,
    solver_loop_txs: Vec<mpsc::Sender<ControlMessage>>,
    solver_stopped_rxs: Vec<mpsc::Receiver<ControlMessage>>,
}

impl StratumMiner {
    pub fn new(config: MinerConfig) -> StratumMiner {
        let threads = config.num_mining_threads;
        StratumMiner {
            _config: config,
            shared_data: Arc::new(RwLock::new(JobSharedData::new(threads))),
            control_txs: vec![],
            solver_loop_txs: vec![],
            solver_stopped_rxs: vec![],
        }
    }

    fn solver_thread(
        mut solver: SolverInstance,
        instance: usize,
        shared_data: JobSharedDataType,
        control_rx: mpsc::Receiver<ControlMessage>,
        solver_loop_rx: mpsc::Receiver<ControlMessage>,
        solver_stopped_tx: mpsc::Sender<ControlMessage>,
    ) {
        let stop_handle = thread::spawn(move || loop {
            while let Some(message) = control_rx.iter().next() {
                match message {
                    ControlMessage::Stop => {
                        info!("Stopping Solver");
                        // todo
                        return;
                    },
                    ControlMessage::Pause => {
                        info!("Pausing Solver");
                        // todo
                    },
                    ControlMessage::Resume => {
                        info!("Resuming Solver");
                        // todo
                    },
                    _ => {},
                };
            }
        });

        let mut paused = true;
        loop {
            if let Some(message) = solver_loop_rx.try_iter().next() {
                debug!("solver_thread - solver_loop_rx got msg: {:?}", message);
                match message {
                    ControlMessage::Stop => break,
                    ControlMessage::Pause => paused = true,
                    ControlMessage::Resume => paused = false,
                    _ => {},
                }
            }
            if paused {
                thread::sleep(time::Duration::from_micros(100));
                continue;
            }
            let header = { shared_data.read().unwrap().header.clone().unwrap() };
            let height = { shared_data.read().unwrap().height };
            let job_id = { shared_data.read().unwrap().job_id };
            let target_difficulty = { shared_data.read().unwrap().difficulty };

            let mut hasher = BlockHeaderSha3::new(tari_app_grpc::tari_rpc::BlockHeader::from(header)).unwrap();
            hasher.random_nonce();
            let difficulty = hasher.difficulty();
            if difficulty >= target_difficulty {
                let block_header: BlockHeader = BlockHeader::try_from(hasher.into_header()).unwrap();
                info!(
                    "Miner found block header {} with difficulty {:?}",
                    block_header, difficulty,
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
                }
            }
            solver.solutions = Solution::default();
            thread::sleep(time::Duration::from_micros(100));
        }

        let _ = stop_handle.join();
        let _ = solver_stopped_tx.send(ControlMessage::SolverStopped(instance));
    }

    pub fn start_solvers(&mut self) -> Result<(), stratum::Error> {
        let solvers = vec![SolverInstance::new()?];
        for (i, s) in solvers.into_iter().enumerate() {
            let sd = self.shared_data.clone();
            let (control_tx, control_rx) = mpsc::channel::<ControlMessage>();
            let (solver_tx, solver_rx) = mpsc::channel::<ControlMessage>();
            let (solver_stopped_tx, solver_stopped_rx) = mpsc::channel::<ControlMessage>();
            self.control_txs.push(control_tx);
            self.solver_loop_txs.push(solver_tx);
            self.solver_stopped_rxs.push(solver_stopped_rx);
            thread::spawn(move || {
                StratumMiner::solver_thread(s, i, sd, control_rx, solver_rx, solver_stopped_tx);
            });
        }
        Ok(())
    }

    pub fn notify(&mut self, job_id: u64, height: u64, blob: String, difficulty: u64) -> Result<(), stratum::Error> {
        let header_hex =
            hex::decode(blob).map_err(|_| stratum::Error::JsonError("Blob is not a valid hex value".to_string()))?;
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
        debug!("Stop message sent");
    }

    pub fn pause_solvers(&self) {
        for t in self.control_txs.iter() {
            let _ = t.send(ControlMessage::Pause);
        }
        for t in self.solver_loop_txs.iter() {
            let _ = t.send(ControlMessage::Pause);
        }
        debug!("Pause message sent");
    }

    pub fn resume_solvers(&self) {
        for t in self.control_txs.iter() {
            let _ = t.send(ControlMessage::Resume);
        }
        for t in self.solver_loop_txs.iter() {
            let _ = t.send(ControlMessage::Resume);
        }
        debug!("Resume message sent");
    }

    pub fn wait_for_solver_shutdown(&self) {
        for r in self.solver_stopped_rxs.iter() {
            while let Some(message) = r.iter().next() {
                if let ControlMessage::SolverStopped(i) = message {
                    debug!("Solver stopped: {}", i);
                    break;
                }
            }
        }
    }
}
