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
use std::{
    convert::TryFrom,
    sync::mpsc,
    time::{Duration, SystemTime},
};

use borsh::BorshDeserialize;
use futures::stream::StreamExt;
use log::*;
use minotari_app_grpc::tari_rpc::BlockHeader;
use tari_common_types::types::FixedHash;
use tari_max_size::MaxSizeBytes;
use tari_utilities::{ByteArray, hex::Hex};

use crate::{
    miner::Miner,
    run_miner::display_report,
    stratum::{error::Error, stratum_types as types},
};

pub const LOG_TARGET: &str = "minotari::miner::stratum::controller";
pub const LOG_TARGET_FILE: &str = "minotari::logging::miner::stratum::controller";

type CurrentBlob = MaxSizeBytes<{ 4 * 1024 * 1024 }>; // 4 MiB

/// Decodes the blob of a job received from the pool.
///
/// The blob is untrusted pool input. If it does not fit in a [`CurrentBlob`] this logs a warning and returns `None`
/// so that the caller skips the job and keeps mining the current one, instead of returning an error out of the
/// controller loop and stopping the miner.
fn decode_job_blob(height: u64, job_id: u64, blob: Vec<u8>) -> Option<CurrentBlob> {
    let blob_len = blob.len();
    match CurrentBlob::try_from(blob) {
        Ok(blob) => Some(blob),
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Skipping job {job_id} at height {height}: blob of {blob_len} bytes is invalid ({e})"
            );
            None
        },
    }
}

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
                debug!(target: LOG_TARGET_FILE, "Miner received message: {message:?}");
                match message {
                    types::miner_message::MinerMessage::ReceivedJob(height, job_id, diff, blob) => {
                        // A job from the pool with an oversized blob is skipped; it must not stop the controller
                        // (and with it the job currently being mined).
                        let Some(blob) = decode_job_blob(height, job_id, blob) else {
                            continue;
                        };
                        match self.should_we_update_job(height, job_id, diff, blob) {
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
                                        FixedHash::zero(),
                                        None,
                                    ));
                                } else {
                                    continue;
                                }
                            },
                            Err(e) => {
                                debug!(
                                    target: LOG_TARGET_FILE,
                                    "Miner could not decipher miner message: {e:?}"
                                );
                                // lets wait a second before we try again
                                tokio::time::sleep(Duration::from_secs(1)).await;
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
            if let Some(reporter) = miner.as_mut() &&
                let Some(report) = (*reporter).next().await &&
                let Some(header) = report.header.clone()
            {
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
                    let block_header: tari_node_components::blocks::BlockHeader =
                        tari_node_components::blocks::BlockHeader::try_from(header).map_err(Error::MissingData)?;
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
            // Decode first so that a blob which is not a valid header leaves the current job untouched
            let mut buffer = blob.as_bytes();
            let tari_header: tari_node_components::blocks::BlockHeader = BorshDeserialize::deserialize(&mut buffer)
                .map_err(|_| Error::General("Byte Blob is not a valid header".to_string()))?;
            self.current_height = height;
            self.current_job_id = job_id;
            self.current_blob = blob;
            self.current_difficulty_target = diff;
            self.current_header = Some(minotari_app_grpc::tari_rpc::BlockHeader::from(tari_header));
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MAX_BLOB: usize = 4 * 1024 * 1024;

    #[test]
    fn decode_job_blob_accepts_blobs_up_to_the_limit() {
        assert_eq!(decode_job_blob(1, 2, vec![]).unwrap().len(), 0);
        assert_eq!(decode_job_blob(1, 2, vec![7u8; 10]).unwrap().as_bytes(), &[7u8; 10]);
        assert_eq!(decode_job_blob(1, 2, vec![0u8; MAX_BLOB]).unwrap().len(), MAX_BLOB);
    }

    #[test]
    fn decode_job_blob_skips_an_oversized_blob_instead_of_failing() {
        assert!(decode_job_blob(1, 2, vec![0u8; MAX_BLOB + 1]).is_none());
    }

    #[test]
    fn an_invalid_header_blob_leaves_the_current_job_untouched() {
        use borsh::BorshSerialize;

        let mut controller = Controller::new(1).unwrap();
        let mut valid = Vec::new();
        tari_node_components::blocks::BlockHeader::new(0)
            .serialize(&mut valid)
            .unwrap();
        let valid = CurrentBlob::try_from(valid).unwrap();
        assert!(controller.should_we_update_job(5, 7, 11, valid.clone()).unwrap());
        let header = controller.current_header.clone();
        assert!(header.is_some());

        // Fits in a `CurrentBlob`, but is not a borsh `BlockHeader`
        let invalid = CurrentBlob::try_from(vec![0xffu8; 16]).unwrap();
        assert!(controller.should_we_update_job(6, 8, 12, invalid).is_err());
        assert_eq!(controller.current_height, 5);
        assert_eq!(controller.current_job_id, 7);
        assert_eq!(controller.current_difficulty_target, 11);
        assert_eq!(controller.current_blob, valid);
        assert_eq!(controller.current_header, header);
    }
}
