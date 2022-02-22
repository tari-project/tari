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
    pin::Pin,
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};

use crossbeam::channel::{bounded, Select, Sender, TrySendError};
use futures::Stream;
use log::*;
use tari_app_grpc::{conversions::timestamp, tari_rpc::BlockHeader};
use thread::JoinHandle;

use super::difficulty::BlockHeaderSha3;

pub const LOG_TARGET: &str = "tari_mining_node::miner::standalone";

// Identify how often mining thread is reporting / checking context
// ~400_000 hashes per second
const REPORTING_FREQUENCY: u64 = 3_000_000;

// Thread's stack size, ideally we would fit all thread's data in the CPU L1 cache
const STACK_SIZE: usize = 32_000;

/// Miner will send regular reports from every mining threads
#[derive(Debug)]
pub struct MiningReport {
    pub miner: usize,
    pub target_difficulty: u64,
    pub difficulty: u64,
    pub hashes: u64,
    pub elapsed: Duration,
    /// Will be set for when mined header is matching required difficulty
    pub header: Option<BlockHeader>,
    pub height: u64,
    pub last_nonce: u64,
}

/// Miner is starting number of mining threads and implements Stream for async reports polling
/// Communication with async world is performed via channel and waker so should be quite efficient
pub struct Miner {
    threads: Vec<JoinHandle<()>>,
    channels: Vec<crossbeam::channel::Receiver<MiningReport>>,
    num_threads: usize,
    header: BlockHeader,
    target_difficulty: u64,
    share_mode: bool,
}

impl Miner {
    pub fn init_mining(header: BlockHeader, target_difficulty: u64, num_threads: usize, share_mode: bool) -> Self {
        Self {
            threads: vec![],
            channels: vec![],
            header,
            num_threads,
            target_difficulty,
            share_mode,
        }
    }

    // this will kill all mining threads currently active and attached to this miner
    pub fn kill_threads(&mut self) {
        self.channels.clear();
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
                let share_mode = self.share_mode;
                let handle = thread
                    .spawn(move || mining_task(header, difficulty, tx, waker, i, share_mode))
                    .expect("Failed to create mining thread");
                (handle, rx)
            });

        let (threads, channels) = miners.unzip();
        self.threads = threads;
        self.channels = channels;
    }
}

impl Stream for Miner {
    type Item = MiningReport;

    fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        trace!(target: LOG_TARGET, "Polling Miner");
        // First poll would start all the threads passing async context waker
        if self.threads.is_empty() && self.num_threads > 0 {
            debug!(
                target: LOG_TARGET,
                "Starting {} mining threads for target difficulty {}", self.num_threads, self.target_difficulty
            );
            self.start_threads(ctx);
            return Poll::Pending;
        } else if self.num_threads == 0 {
            error!(target: LOG_TARGET, "Cannot mine: no mining threads");
            return Poll::Ready(None);
        } else if self.channels.is_empty() {
            debug!(target: LOG_TARGET, "Finished mining");
            return Poll::Ready(None);
        }

        // Non blocking select from all miner's receiver channels
        let mut sel = Select::new();
        for rx in self.channels.iter() {
            sel.recv(rx);
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
        if report.header.is_some() && !self.share_mode {
            // Dropping recipients would stop miners next time they try to report
            self.channels.clear();
        }
        Poll::Ready(Some(report))
    }
}

/// Miner starts with a random nonce and iterates until it finds a header hash that meets the desired
/// target
pub fn mining_task(
    header: BlockHeader,
    target_difficulty: u64,
    sender: Sender<MiningReport>,
    waker: Waker,
    miner: usize,
    share_mode: bool,
) {
    let start = Instant::now();
    let mut hasher = BlockHeaderSha3::new(header).unwrap();
    hasher.random_nonce();
    // We're mining over here!
    trace!(target: LOG_TARGET, "Mining thread {} started", miner);
    // Mining work
    loop {
        let difficulty = hasher.difficulty();
        if difficulty >= target_difficulty {
            debug!(
                target: LOG_TARGET,
                "Miner {} found nonce {} with matching difficulty {}", miner, hasher.nonce, difficulty
            );
            if let Err(err) = sender.try_send(MiningReport {
                miner,
                difficulty,
                hashes: hasher.hashes,
                elapsed: start.elapsed(),
                height: hasher.height(),
                last_nonce: hasher.nonce,
                header: Some(hasher.create_header()),
                target_difficulty,
            }) {
                error!(target: LOG_TARGET, "Miner {} failed to send report: {}", miner, err);
            }
            // If we are mining in share mode, this share might not be a block, so we need to keep mining till we get a
            // new job
            if !share_mode {
                waker.wake();
                trace!(target: LOG_TARGET, "Mining thread {} stopped", miner);
                return;
            } else {
                waker.clone().wake();
            }
        }
        if hasher.nonce % REPORTING_FREQUENCY == 0 {
            let res = sender.try_send(MiningReport {
                miner,
                difficulty,
                hashes: hasher.hashes,
                elapsed: start.elapsed(),
                header: None,
                last_nonce: hasher.nonce,
                height: hasher.height(),
                target_difficulty,
            });
            waker.clone().wake();
            trace!(target: LOG_TARGET, "Reporting from {} result {:?}", miner, res);
            if let Err(TrySendError::Disconnected(_)) = res {
                info!(target: LOG_TARGET, "Mining thread {} disconnected", miner);
                return;
            }
            if !(share_mode) {
                hasher.set_forward_timestamp(timestamp().seconds as u64);
            }
        }
        hasher.inc_nonce();
    }
}
