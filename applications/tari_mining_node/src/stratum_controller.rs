use crate::{stratum, stratum_miner::StratumMiner, stratum_types as types};
use log::*;
use std::{self, sync::mpsc, thread};

pub struct Controller {
    rx: mpsc::Receiver<types::MinerMessage>,
    pub tx: mpsc::Sender<types::MinerMessage>,
    client_tx: Option<mpsc::Sender<types::ClientMessage>>,
    current_height: u64,
    current_job_id: u64,
    _current_target_diff: u64,
    current_blob: String,
}

impl Controller {
    pub fn new() -> Result<Controller, String> {
        let (tx, rx) = mpsc::channel::<types::MinerMessage>();
        Ok(Controller {
            rx,
            tx,
            client_tx: None,
            current_height: 0,
            current_job_id: 0,
            _current_target_diff: 0,
            current_blob: "".to_string(),
        })
    }

    pub fn set_client_tx(&mut self, client_tx: mpsc::Sender<types::ClientMessage>) {
        self.client_tx = Some(client_tx);
    }

    pub fn run(&mut self, mut miner: StratumMiner) -> Result<(), stratum::Error> {
        loop {
            while let Some(message) = self.rx.try_iter().next() {
                debug!("Miner received message: {:?}", message);
                let result: Result<(), stratum::Error> = match message {
                    types::MinerMessage::ReceivedJob(height, job_id, diff, blob) => {
                        self.current_height = height;
                        self.current_job_id = job_id;
                        self._current_target_diff = diff;
                        self.current_blob = blob;
                        miner.notify(
                            self.current_job_id,
                            self.current_height,
                            self.current_blob.clone(),
                            diff,
                        )
                    },
                    types::MinerMessage::StopJob => {
                        debug!("Stopping jobs");
                        miner.pause_solvers();
                        Ok(())
                    },
                    types::MinerMessage::Shutdown => {
                        debug!("Stopping jobs and Shutting down mining controller");
                        miner.stop_solvers();
                        miner.wait_for_solver_shutdown();
                        return Ok(());
                    },
                };
                if let Err(e) = result {
                    error!("Mining Controller Error {:?}", e);
                }
            }

            let solutions = miner.get_solutions();
            if let Some(ss) = solutions {
                let _ = self
                    .client_tx
                    .as_mut()
                    .unwrap()
                    .send(types::ClientMessage::FoundSolution(ss.job_id, ss.hash, ss.nonce));
            }
            thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
