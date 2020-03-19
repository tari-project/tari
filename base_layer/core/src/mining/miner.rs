// Copyright 2019. The Tari Project
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

use crate::{
    base_node::{
        comms_interface::{BlockEvent, LocalNodeCommsInterface},
        states::BaseNodeState,
    },
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{BlockAddResult, BlockchainBackend},
    consensus::ConsensusManager,
    mining::{blake_miner::CpuBlakePow, error::MinerError, CoinbaseBuilder},
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        transaction::UnblindedOutput,
        types::{CryptoFactories, PrivateKey},
    },
};
use core::sync::atomic::AtomicBool;
use futures::{
    channel::{
        mpsc,
        mpsc::{Receiver, Sender},
    },
    future::FutureExt,
    pin_mut,
    StreamExt,
};
use log::*;
use rand::rngs::OsRng;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tari_broadcast_channel::Subscriber;
use tari_crypto::keys::SecretKey;
use tokio::task::spawn_blocking;

use tokio::task;

pub const LOG_TARGET: &str = "c::m::miner";

pub struct Miner<B: BlockchainBackend> {
    kill_flag: Arc<AtomicBool>,
    stop_mining_flag: Arc<AtomicBool>,
    consensus: ConsensusManager<B>,
    node_interface: LocalNodeCommsInterface,
    utxo_sender: Sender<UnblindedOutput>,
    state_change_event_rx: Option<Subscriber<BaseNodeState>>,
    threads: usize,
}

impl<B: BlockchainBackend + 'static> Miner<B> {
    /// Constructs a new miner
    pub fn new(
        stop_flag: Arc<AtomicBool>,
        consensus: ConsensusManager<B>,
        node_interface: &LocalNodeCommsInterface,
        threads: usize,
    ) -> Miner<B>
    {
        let (utxo_sender, _): (Sender<UnblindedOutput>, Receiver<UnblindedOutput>) = mpsc::channel(1);
        Miner {
            kill_flag: stop_flag,
            consensus,
            stop_mining_flag: Arc::new(AtomicBool::new(false)),
            node_interface: node_interface.clone(),
            utxo_sender,
            state_change_event_rx: None,
            threads,
        }
    }

    /// This function instantiates a new channel and returns the receiver so that the miner can send out a unblinded
    /// output. This output is only sent if the miner successfully mines a block
    pub fn get_utxo_receiver_channel(&mut self) -> Receiver<UnblindedOutput> {
        // this should not be too large, as this should not get lost as these are your coinbase utxo's
        let (sender, receiver): (Sender<UnblindedOutput>, Receiver<UnblindedOutput>) = mpsc::channel(20);
        self.utxo_sender = sender;
        receiver
    }

    /// This provides a tari_broadcast_channel to the miner so that it can subscribe to the state machine.
    /// The state machine will publish state changes here. The miner is only interested to know when the state machine
    /// transitions to listing state. This means that the miner has moved from some disconnected state to up to date
    /// and the miner can ask for a new block to mine upon.
    pub fn subscribe_to_state_change(&mut self, state_change_event_rx: Subscriber<BaseNodeState>) {
        self.state_change_event_rx = Some(state_change_event_rx);
    }

    /// Mine blocks asynchronously.
    ///
    /// On the first iteration, the thread will loop around until `received_new_block_flag` is true. This flag is set
    /// to true when either a new block is received from the node, or when the node reaches the `Listening` state
    /// (see [Miner::mine]).
    ///
    /// Then, if the miner hasn't been stopped, it starts the main mining loop:
    /// 1. We request a new template block from the base node
    /// 2. We add our Coinbase UTXO to the block
    /// 3. We send thi sback to the node to calculate the MMR roots
    /// 4. We iterate on the header nonce until
    ///     * the target difficulty is reached
    ///     * or the loop is interrupted because a new block was found in the interim, or the miner is stopped
    async fn mining(mut self) -> Result<Miner<B>, MinerError> {
        // Lets make sure its set to mine
        debug!(target: LOG_TARGET, "Miner asking for new candidate block to mine.");
        let mut block_template = self.get_block_template().await?;
        let output = self.add_coinbase(&mut block_template)?;
        let mut block = self.get_block(block_template).await?;
        debug!(target: LOG_TARGET, "Miner got new block to mine.");
        let difficulty = self.get_req_difficulty().await?;
        let (tx, mut rx): (Sender<Option<BlockHeader>>, Receiver<Option<BlockHeader>>) = mpsc::channel(self.threads);
        for _ in 0..self.threads {
            let stop_mining_flag = self.stop_mining_flag.clone();
            let header = block.header.clone();
            let mut tx_channel = tx.clone();
            trace!("spawning mining thread");
            spawn_blocking(move || {
                let result = CpuBlakePow::mine(difficulty, header, stop_mining_flag);
                // send back what the miner found, None will be sent if the miner did not find a nonce
                tx_channel.try_send(result);
            });
        }
        drop(tx); // lets ensure that the tx all get dropped
        while let Some(value) = rx.next().await {
            // let see if we sound a header, this will be none if no header was found
            if let Some(r) = value {
                // found block, lets ensure we kill all other threads
                self.stop_mining_flag.store(true, Ordering::Relaxed);
                block.header = r;
                self.send_block(block).await.or_else(|e| {
                    error!(target: LOG_TARGET, "Could not send block to base node. {:?}.", e);
                    Err(e)
                })?;
                self.utxo_sender
                    .try_send(output)
                    .or_else(|e| {
                        error!(target: LOG_TARGET, "Could not send utxo to wallet. {:?}.", e);
                        Err(e)
                    })
                    .map_err(|e| MinerError::CommunicationError(e.to_string()))?;
                break;
            }
        }
        trace!("returning closing thread");
        Ok(self)
    }

    // This is just a helper function to get around the rust borrow checker
    async fn not_mining(self) -> Result<Miner<B>, MinerError> {
        Ok(self)
    }

    /// function, this function gets called when a new block event is triggered. It will ensure that the miner
    /// restarts/starts to mine.
    pub async fn mine(mut self) {
        // This flag is used to stop the mining;
        let flag = self.stop_mining_flag.clone();
        let block_event = self.node_interface.clone().get_block_event_stream_fused();
        let state_event = self
            .state_change_event_rx
            .take()
            .expect("Miner does not have access to state event stream")
            .fuse();

        let t_miner_kill_check = check_kill_flag(self.kill_flag.clone(), flag.clone()).fuse();

        pin_mut!(block_event);
        pin_mut!(state_event);
        pin_mut!(t_miner_kill_check);
        let mut start_mining = false;
        trace!("starting mining thread");
        'main: loop {
            flag.store(false, Ordering::Relaxed); // ensure we can mine if we need to
            let mining_future = match start_mining {
                true => task::spawn(self.mining()),
                false => task::spawn(self.not_mining()),
            };
            futures::select! {
            msg = block_event.select_next_some() => {
                match (*msg).clone() {
                    BlockEvent::Verified((_, result)) => {
                    if result == BlockAddResult::Ok{
                        flag.store(true, Ordering::Relaxed);
                        start_mining = true;
                    };
                },
                _ => (),
                }
            },
            msg = state_event.select_next_some() => {
                flag.store(true, Ordering::Relaxed);
                match (*msg).clone() {
                    BaseNodeState::Listening(_) => {
                        start_mining = true;
                    },
                    _ => {
                        start_mining = false;
                    },
                }
            },
            (_) = t_miner_kill_check => {
                flag.store(true, Ordering::Relaxed);
                break 'main;
            }
            };
            self = mining_future.await.expect("Miner crashed").expect("Miner crashed");
        }
        debug!(target: LOG_TARGET, "Mining thread stopped.");
    }

    /// function, temp use genesis block as template
    pub async fn get_block_template(&mut self) -> Result<NewBlockTemplate, MinerError> {
        trace!(target: LOG_TARGET, "Requesting new block template from node.");
        Ok(self
            .node_interface
            .get_new_block_template()
            .await
            .or_else(|e| {
                error!(
                    target: LOG_TARGET,
                    "Could not get a new block template from the base node. {:?}.", e
                );
                Err(e)
            })
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?)
    }

    /// function, temp use genesis block as template
    pub async fn get_block(&mut self, block: NewBlockTemplate) -> Result<Block, MinerError> {
        trace!(
            target: LOG_TARGET,
            "Asking node to fill in MMR roots for new block candidate"
        );
        Ok(self
            .node_interface
            .get_new_block(block)
            .await
            .or_else(|e| {
                error!(
                    target: LOG_TARGET,
                    "Could not get a new block from the base node. {:?}.", e
                );
                Err(e)
            })
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?)
    }

    /// function to get the required difficulty
    pub async fn get_req_difficulty(&mut self) -> Result<Difficulty, MinerError> {
        trace!(target: LOG_TARGET, "Requesting target difficulty from node");
        Ok(self
            .node_interface
            .get_target_difficulty(PowAlgorithm::Blake)
            .await
            .or_else(|e| {
                error!(
                    target: LOG_TARGET,
                    "Could not get the required difficulty from the base node. {:?}.", e
                );

                Err(e)
            })
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?)
    }

    // add the coinbase to the NewBlockTemplate
    fn add_coinbase(&self, block: &mut NewBlockTemplate) -> Result<UnblindedOutput, MinerError> {
        let fees = block.body.get_total_fee();
        let (key, r) = self.get_spending_key()?;
        let factories = CryptoFactories::default();
        let builder = CoinbaseBuilder::new(factories);
        let builder = builder
            .with_block_height(block.header.height)
            .with_fees(fees)
            .with_nonce(r)
            .with_spend_key(key);
        let (tx, unblinded_output) = builder
            .build(self.consensus.clone())
            .expect("invalid constructed coinbase");
        block.body.add_output(tx.body.outputs()[0].clone());
        block.body.add_kernel(tx.body.kernels()[0].clone());
        Ok(unblinded_output)
    }

    /// function to create private key and nonce for coinbase
    pub fn get_spending_key(&self) -> Result<(PrivateKey, PrivateKey), MinerError> {
        let r = PrivateKey::random(&mut OsRng);
        let key = PrivateKey::random(&mut OsRng);
        Ok((key, r))
    }

    ///  function to send a block
    async fn send_block(&mut self, block: Block) -> Result<(), MinerError> {
        info!(target: LOG_TARGET, "Mined a block: {}", block);
        self.node_interface
            .submit_block(block)
            .await
            .or_else(|e| {
                error!(target: LOG_TARGET, "Could not send block to base node. {:?}.", e);
                Err(e)
            })
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?;
        Ok(())
    }
}

async fn check_kill_flag(kill_flag: Arc<AtomicBool>, stop_flag: Arc<AtomicBool>) {
    loop {
        tokio::time::delay_for(Duration::from_millis(100)).await;
        if kill_flag.load(Ordering::Relaxed) {
            stop_flag.store(true, Ordering::Relaxed);
        }
    }
}
