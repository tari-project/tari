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
use crossbeam::channel::{bounded, Select, Sender, TrySendError};
use futures::Stream;
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{
    pin::Pin,
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};
use tari_core::{blocks::BlockHeader, proof_of_work::sha3_difficulty, tari_utilities::epoch_time::EpochTime};
use thread::JoinHandle;

// Identify how often mining thread is reporting / checking context
// ~400_000 hashes per second
const REPORTING_FREQUENCY: u64 = 3_000_000;

// Thread's stack size, ideally we would fit all thread's data in the CPU L1 cache
const STACK_SIZE: usize = 32_000;

/// Miner will send regular reports from every mining threads
#[derive(Debug)]
pub struct MiningReport {
    pub miner: usize,
    pub difficulty: u64,
    pub hashes: u64,
    pub elapsed: Duration,
    /// Will be set for when mined header is matching required difficulty
    pub header: Option<BlockHeader>,
}

/// Miner is starting number of mining threads and implements Stream for async reports polling
/// Communication with async world is performed via channel and waker so should be quite efficient
pub struct Miner {
    threads: Vec<JoinHandle<()>>,
    channels: Vec<crossbeam::channel::Receiver<MiningReport>>,
    num_threads: usize,
    header: BlockHeader,
    target_difficulty: u64,
}

impl Miner {
    pub fn init_mining(header: BlockHeader, target_difficulty: u64, num_threads: usize) -> Self {
        Self {
            threads: vec![],
            channels: vec![],
            header,
            num_threads,
            target_difficulty,
        }
    }

    // Start mining threads with async context waker
    fn start_threads(&mut self, ctx: &Context<'_>) {
        let miners = (0..self.num_threads)
            .map(|i| {
                (
                    thread::Builder::new()
                        .name(format!("cpu-miner-{}", i))
                        .stack_size(STACK_SIZE),
                    i,
                )
            })
            .map(|(thread, i)| {
                let (tx, rx) = bounded(1);
                let header = self.header.clone();
                let waker = ctx.waker().clone();
                let difficulty = self.target_difficulty;
                let handle = thread
                    .spawn(move || mining_task(header, difficulty, tx, waker, i))
                    .expect("Failed to create mining thread");
                (handle, rx)
            });

        let (threads, channels) = miners.unzip();
        self.threads = threads;
        self.channels = channels;
    }

    pub fn join(self) {
        for handle in self.threads.into_iter() {
            handle.join().unwrap();
        }
    }
}

impl Stream for Miner {
    type Item = MiningReport;

    fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        trace!("Polling Miner");
        // First poll would start all the threads passing async context waker
        if self.threads.len() == 0 && self.num_threads > 0 {
            debug!(
                "Starting {} mining threads for target difficulty {}",
                self.num_threads, self.target_difficulty
            );
            self.start_threads(&ctx);
            return Poll::Pending;
        } else if self.num_threads == 0 {
            error!("Cannot mine: no mining threads");
            return Poll::Ready(None);
        } else if self.channels.len() == 0 {
            debug!("Finished mining");
            return Poll::Ready(None);
        }

        // Non blocking select from all miner's receiver channels
        let mut sel = Select::new();
        for rx in self.channels.iter() {
            sel.recv(&rx);
        }
        let report = match sel.try_select() {
            Ok(oper) => {
                let idx = oper.index();
                match oper.recv(&self.channels[idx]) {
                    Ok(report) => report,
                    Err(_) => {
                        // Received error would mean thread is disconnected already
                        trace!("Thread {} disconnected.", idx);
                        return Poll::Ready(None);
                    },
                }
            },
            Err(_) => {
                // No reports
                return Poll::Pending;
            },
        };
        if report.header.is_some() {
            // Dropping recipients would stop miners next time they try to report
            self.channels.clear();
        }
        return Poll::Ready(Some(report));
    }
}

/// Miner starts with a random nonce and iterates until it finds a header hash that meets the desired
/// target
pub fn mining_task(
    mut header: BlockHeader,
    target_difficulty: u64,
    sender: Sender<MiningReport>,
    waker: Waker,
    miner: usize,
)
{
    let start = Instant::now();
    let mut nonce: u64 = OsRng.next_u64();
    let start_nonce = nonce;
    // We're mining over here!
    info!("Mining thread {} started", miner);
    // Mining work
    loop {
        header.nonce = nonce;
        let difficulty: u64 = sha3_difficulty(&header).into();
        if difficulty >= target_difficulty {
            debug!(
                "Miner {} found nonce {} with matching difficulty {}",
                miner, nonce, difficulty
            );
            if let Err(err) = sender.try_send(MiningReport {
                miner,
                difficulty: difficulty.into(),
                hashes: nonce.wrapping_sub(start_nonce),
                elapsed: start.elapsed(),
                header: Some(header),
            }) {
                error!("Miner {} failed to send report: {}", miner, err);
            }
            waker.clone().wake();
            info!("Mining thread {} stopped", miner);
            return;
        }
        if nonce % REPORTING_FREQUENCY == 0 {
            let res = sender.try_send(MiningReport {
                miner,
                difficulty: difficulty.into(),
                hashes: nonce.wrapping_sub(start_nonce),
                elapsed: start.elapsed(),
                header: None,
            });
            waker.clone().wake();
            trace!("Reporting from {} result {:?}", miner, res);
            if let Err(TrySendError::Disconnected(_)) = res {
                info!("Mining thread {} disconnected", miner);
                return;
            }
            header.timestamp = EpochTime::now();
        }
        nonce = nonce.wrapping_add(1);
    }
}
