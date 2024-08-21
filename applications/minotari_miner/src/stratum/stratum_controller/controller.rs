//  Copyright 2024. The Tari Project
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
use std::{convert::TryFrom, sync::mpsc, thread, time::SystemTime};

use borsh::BorshDeserialize;
use futures::stream::StreamExt;
use log::*;
use minotari_app_grpc::tari_rpc::BlockHeader;
use tari_common_types::MaxSizeBytes;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    miner::Miner,
    run_miner::display_report,
    stratum::{error::Error, stratum_types as types},
};

pub const LOG_TARGET: &str = "minotari::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "minotari::logging::miner::stratum::controller";

type CurrentBlob = MaxSizeBytes<{ 4 * 1024 * 1024 }>; // 4 MiB

pub struct Controller {
    rx: mpsc::Receiver<types::miner_message::MinerMessage>,
    pub tx: mpsc::Sender<types::miner_message::MinerMessage>,
    client_tx: Option<mpsc::Sender<types::client_message::ClientMessage>>,
    current_height: u64,
    current_job_id: u64,
    current_difficulty_target: u64,
    current_blob: CurrentBlob,
    current_header: Option<BlockHeader>,
    keep_alive_time: SystemTime,
    num_mining_threads: usize,
}

impl Controller {
    pub fn new(num_mining_threads: usize) -> Result<Controller, String> {
        let (tx, rx) = mpsc::channel::<types::miner_message::MinerMessage>();
        Ok(Controller {
            rx,
            tx,
            client_tx: None,
            current_height: 0,
            current_job_id: 0,
            current_difficulty_target: 0,
            current_blob: CurrentBlob::default(),
            current_header: None,
            keep_alive_time: SystemTime::now(),
            num_mining_threads,
        })
    }

    pub fn set_client_tx(&mut self, client_tx: mpsc::Sender<types::client_message::ClientMessage>) {
        self.client_tx = Some(client_tx);
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run(&mut self) -> Result<(), Error> {
        let mut miner: Option<Miner> = None;
        loop {
            // lets see if we need to change the state of the miner.
            while let Some(message) = self.rx.try_iter().next() {
                debug!(target: LOG_TARGET_FILE, "Miner received message: {:?}", message);
                match message {
                    types::miner_message::MinerMessage::ReceivedJob(height, job_id, diff, blob) => {
                        match self.should_we_update_job(height, job_id, diff, CurrentBlob::try_from(blob)?) {
                            Ok(should_we_update) => {
                                if should_we_update {
                                    let header = self
                                        .current_header
                                        .clone()
                                        .ok_or_else(|| Error::MissingData("Header".to_string()))?;
                                    if let Some(acive_miner) = miner.as_mut() {
                                        acive_miner.kill_threads();
                                    }
                                    miner = Some(Miner::init_mining(
                                        header,
                                        self.current_difficulty_target,
                                        self.num_mining_threads,
                                        true,
                                    ));
                                } else {
                                    continue;
                                }
                            },
                            Err(e) => {
                                debug!(
                                    target: LOG_TARGET_FILE,
                                    "Miner could not decipher miner message: {:?}", e
                                );
                                // lets wait a second before we try again
                                thread::sleep(std::time::Duration::from_millis(1000));
                                continue;
                            },
                        }
                    },
                    types::miner_message::MinerMessage::StopJob => {
                        debug!(target: LOG_TARGET_FILE, "Stopping jobs");
                        miner = None;
                        continue;
                    },
                    types::miner_message::MinerMessage::ResumeJob => {
                        debug!(target: LOG_TARGET_FILE, "Resuming jobs");
                        miner = None;
                        continue;
                    },
                    types::miner_message::MinerMessage::Shutdown => {
                        debug!(
                            target: LOG_TARGET_FILE,
                            "Stopping jobs and Shutting down mining controller"
                        );
                        miner = None;
                    },
                };
            }
            let mut submit = true;
            if let Some(reporter) = miner.as_mut() {
                if let Some(report) = (*reporter).next().await {
                    if let Some(header) = report.header.clone() {
                        if report.difficulty < self.current_difficulty_target {
                            submit = false;
                            debug!(
                                target: LOG_TARGET_FILE,
                                "Mined difficulty {} below target difficulty {}. Not submitting.",
                                report.difficulty,
                                self.current_difficulty_target
                            );
                        }

                        if submit {
                            // Mined a block fitting the difficulty
                            let block_header: tari_core::blocks::BlockHeader =
                                tari_core::blocks::BlockHeader::try_from(header).map_err(Error::MissingData)?;
                            let hash = block_header.hash().to_hex();
                            info!(
                                target: LOG_TARGET,
                                "Miner found share with hash {}, nonce {} and difficulty {:?}",
                                hash,
                                block_header.nonce,
                                report.difficulty
                            );
                            debug!(
                                target: LOG_TARGET_FILE,
                                "Miner found share with hash {}, difficulty {:?} and data {:?}",
                                hash,
                                report.difficulty,
                                block_header
                            );
                            self.client_tx
                                .as_mut()
                                .ok_or_else(|| Error::Connection("No connection to pool".to_string()))?
                                .send(types::client_message::ClientMessage::FoundSolution(
                                    self.current_job_id,
                                    hash,
                                    block_header.nonce,
                                ))?;
                            self.keep_alive_time = SystemTime::now();
                            continue;
                        } else {
                            display_report(&report, self.num_mining_threads).await;
                        }
                    } else {
                        display_report(&report, self.num_mining_threads).await;
                    }
                }
            }
            if self.keep_alive_time.elapsed()?.as_secs() >= 30 {
                self.keep_alive_time = SystemTime::now();
                self.client_tx
                    .as_mut()
                    .ok_or(Error::ClientTxNotSet)?
                    .send(types::client_message::ClientMessage::KeepAlive)?;
            }
        }
    }

    pub fn should_we_update_job(
        &mut self,
        height: u64,
        job_id: u64,
        diff: u64,
        blob: CurrentBlob,
    ) -> Result<bool, Error> {
        if height != self.current_height ||
            job_id != self.current_job_id ||
            diff != self.current_difficulty_target ||
            blob != self.current_blob
        {
            self.current_height = height;
            self.current_job_id = job_id;
            self.current_blob = blob.clone();
            self.current_difficulty_target = diff;
            let mut buffer = blob.as_bytes();
            let tari_header: tari_core::blocks::BlockHeader = BorshDeserialize::deserialize(&mut buffer)
                .map_err(|_| Error::General("Byte Blob is not a valid header".to_string()))?;
            self.current_header = Some(minotari_app_grpc::tari_rpc::BlockHeader::from(tari_header));
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
