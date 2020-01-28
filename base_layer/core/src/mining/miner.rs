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
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{BlockAddResult, BlockchainBackend},
    consensus::{ConsensusConstants, ConsensusManager},
    mining::{blake_miner::CpuBlakePow, error::MinerError, CoinbaseBuilder},
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        tari_amount::MicroTari,
        transaction::Transaction,
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
use rand::rngs::OsRng;
use std::{
    sync::{atomic::Ordering, Arc},
    thread,
    time,
};

use tari_crypto::keys::SecretKey;
use tokio::runtime;

pub struct Miner<B: BlockchainBackend> {
    kill_flag: Arc<AtomicBool>,
    received_new_block_flag: Arc<AtomicBool>,
    consensus: ConsensusManager<B>,
    node_interface: LocalNodeCommsInterface,
    executor: runtime::Handle,
}

impl<B: BlockchainBackend> Miner<B> {
    /// Constructs a new miner
    pub fn new(
        stop_flag: Arc<AtomicBool>,
        consensus: ConsensusManager<B>,
        node_interface: &LocalNodeCommsInterface,
        executor: runtime::Handle,
    ) -> Miner<B>
    {
        Miner {
            kill_flag: stop_flag,
            consensus,
            received_new_block_flag: Arc::new(AtomicBool::new(false)),
            node_interface: node_interface.clone(),
            executor,
        }
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
            self.add_coinbase(&mut block_template)?;
            let mut block = self.get_block(block_template).await?;
            let difficulty = self.get_req_difficulty().await?;
            let (mut tx, mut rx): (Sender<Option<BlockHeader>>, Receiver<Option<BlockHeader>>) = mpsc::channel(1);
            let new_block_event_flag = self.received_new_block_flag.clone();
            let kill = self.kill_flag.clone();
            let header = block.header.clone();
            self.executor.spawn(async move {
                let result = CpuBlakePow::mine(difficulty, header, new_block_event_flag, kill);
                tx.try_send(result);
            });

            let local_node_interface = self.node_interface.clone();
            let recv_fut = async move {
                let rcvd = rx.next().await;
                let result = rcvd.unwrap();
                if result.is_some() {
                    block.header = result.unwrap();
                    send_block(local_node_interface, block).await;
                }
            };
            let _rcvd = recv_fut.await;
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
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?)
    }

    /// function, temp use genesis block as template
    pub async fn get_block(&mut self, block: NewBlockTemplate) -> Result<Block, MinerError> {
        Ok(self
            .node_interface
            .get_new_block(block)
            .await
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?)
    }

    /// function to get the required difficulty
    pub async fn get_req_difficulty(&mut self) -> Result<Difficulty, MinerError> {
        Ok(self
            .node_interface
            .get_target_difficulty(PowAlgorithm::Blake)
            .await
            .map_err(|e| MinerError::CommunicationError(e.to_string()))?)
    }

    // add the coinbase to the NewBlockTemplate
    fn add_coinbase(&self, block: &mut NewBlockTemplate) -> Result<(), MinerError> {
        let fees = block.body.get_total_fee();
        let height = block.header.height;
        let (tx_id, key, r) = self.get_spending_key(
            fees + self.consensus.emission_schedule().block_reward(height),
            height + ConsensusConstants::current().coinbase_lock_height(),
        )?;
        let factories = Arc::new(CryptoFactories::default());
        let builder = CoinbaseBuilder::new(factories.clone());
        let builder = builder
            .with_block_height(block.header.height)
            .with_fees(fees)
            .with_nonce(r)
            .with_spend_key(key);
        let tx = builder
            .build(self.consensus.clone())
            .expect("invalid constructed coinbase");
        self.submit_tx_to_wallet(&tx, tx_id);
        block.body.add_output(tx.body.outputs()[0].clone());
        block.body.add_kernel(tx.body.kernels()[0].clone());
        Ok(())
    }

    /// stub function, get private key and tx_id from wallet
    pub fn get_spending_key(
        &self,
        _amount: MicroTari,
        _maturity_height: u64,
    ) -> Result<(u64, PrivateKey, PrivateKey), MinerError>
    {
        let r = PrivateKey::random(&mut OsRng);
        let key = PrivateKey::random(&mut OsRng);
        Ok((0, key, r))
    }

    /// Stub function to let wallet know about potential tx
    pub fn submit_tx_to_wallet(&self, _tx: &Transaction, _id: u64) {}
}
///  function to send a block
async fn send_block(mut node_interface: LocalNodeCommsInterface, block: Block) -> Result<(), MinerError> {
    node_interface
        .submit_block(block)
        .await
        .map_err(|e| MinerError::CommunicationError(e.to_string()))?;
    Ok(())
}
