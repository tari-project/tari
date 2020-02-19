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
    base_node::comms_interface::{BlockEvent, LocalNodeCommsInterface},
    blocks::{Block, NewBlockTemplate},
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
    thread,
    time,
};
use tari_crypto::keys::SecretKey;
use tokio::task::spawn_blocking;

pub const LOG_TARGET: &str = "c::m::miner";

pub struct Miner<B: BlockchainBackend> {
    kill_flag: Arc<AtomicBool>,
    received_new_block_flag: Arc<AtomicBool>,
    consensus: ConsensusManager<B>,
    node_interface: LocalNodeCommsInterface,
    utxo_sender: Sender<UnblindedOutput>,
}

impl<B: BlockchainBackend> Miner<B> {
    /// Constructs a new miner
    pub fn new(
        stop_flag: Arc<AtomicBool>,
        consensus: ConsensusManager<B>,
        node_interface: &LocalNodeCommsInterface,
    ) -> Miner<B>
    {
        let (utxo_sender, _): (Sender<UnblindedOutput>, Receiver<UnblindedOutput>) = mpsc::channel(1);
        Miner {
            kill_flag: stop_flag,
            consensus,
            received_new_block_flag: Arc::new(AtomicBool::new(false)),
            node_interface: node_interface.clone(),
            utxo_sender,
        }
    }

    /// This function instantiates a new channel and returns the receiver so that the miner can send out a unblinded
    /// output. This output is only sent if the miner successfully mines a block
    pub fn get_utxo_receiver_channel(&mut self) -> Receiver<UnblindedOutput> {
        let (sender, receiver): (Sender<UnblindedOutput>, Receiver<UnblindedOutput>) = mpsc::channel(1);
        self.utxo_sender = sender;
        receiver
    }

    /// Async function to mine a block
    async fn mining(&mut self) -> Result<(), MinerError> {
        // Lets make sure its set to mine
        while !self.kill_flag.load(Ordering::Relaxed) {
            while !self.received_new_block_flag.load(Ordering::Relaxed) {
                thread::sleep(time::Duration::from_millis(100)); // wait for new block event
                if self.kill_flag.load(Ordering::Relaxed) {
                    return Ok(());
                }
            }
            let flag = self.received_new_block_flag.clone();
            flag.store(false, Ordering::Relaxed);

            let mut block_template = self.get_block_template().await?;
            let output = self.add_coinbase(&mut block_template)?;
            let mut block = self.get_block(block_template).await?;
            let difficulty = self.get_req_difficulty().await?;
            let new_block_event_flag = self.received_new_block_flag.clone();
            let kill = self.kill_flag.clone();
            let header = block.header.clone();
            let mining_handle =
                spawn_blocking(move || CpuBlakePow::mine(difficulty, header, new_block_event_flag, kill));
            let result = mining_handle.await.unwrap_or(None);
            if let Some(r) = result {
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
            }
        }
        Ok(())
    }

    /// function, this function gets called when a new block event is triggered. It will ensure that the miner
    /// restarts/starts to mine.
    pub async fn mine(mut self) {
        let flag = self.received_new_block_flag.clone();
        let mut block_event = self.node_interface.clone().get_block_event_stream_fused();
        let t_miner = self.mining().fuse();
        pin_mut!(t_miner);
        loop {
            futures::select! {
                msg = block_event.select_next_some() => {
                    match (*msg).clone() {
                        BlockEvent::Verified((_, result)) => {
                            if result == BlockAddResult::Ok{
                                flag.store(true, Ordering::Relaxed);
                            }
                        },
                        _ => (),
                    }
                },
                (_) = t_miner => break
            }
        }
    }

    /// function, temp use genesis block as template
    pub async fn get_block_template(&mut self) -> Result<NewBlockTemplate, MinerError> {
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
        trace!(target: LOG_TARGET, "Mined a block: {:?}", block);
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
