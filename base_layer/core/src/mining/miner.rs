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
    blocks::{genesis_block::get_genesis_block, Block, NewBlockTemplate},
    chain_storage::BlockchainBackend,
    consensus::{ConsensusConstants, ConsensusManager},
    mining::{blake_miner::CpuBlakePow, error::MinerError, CoinbaseBuilder},
    proof_of_work::Difficulty,
    transactions::{
        tari_amount::MicroTari,
        transaction::Transaction,
        types::{CryptoFactories, PrivateKey},
    },
};
use core::sync::atomic::AtomicBool;
use futures::channel::mpsc::{Receiver, Sender};
use std::{
    sync::{atomic::Ordering, Arc},
    thread,
    time,
    time::Duration,
};
use tari_crypto::keys::SecretKey;

struct Miner<B: BlockchainBackend> {
    kill_flag: Arc<AtomicBool>,
    stop_mining_flag: Arc<AtomicBool>,
    tx: Sender<Block>,
    consensus: ConsensusManager<B>,
}

impl<B: BlockchainBackend> Miner<B> {
    /// Constructs a new miner
    pub fn new(stop_flag: Arc<AtomicBool>, tx: Sender<Block>, consensus: ConsensusManager<B>) -> Miner<B> {
        Miner {
            kill_flag: Arc::new(AtomicBool::new(false)),
            tx,
            consensus,
            stop_mining_flag: stop_flag,
        }
    }

    /// Async function to mine a block
    pub fn mine(&mut self) -> Result<(), MinerError> {
        // Lets make sure its set to mine
        while !self.kill_flag.load(Ordering::Relaxed) {
            while !self.stop_mining_flag.load(Ordering::Relaxed) {
                thread::sleep(time::Duration::from_millis(100)); // wait for new block event
                if self.kill_flag.load(Ordering::Relaxed) {
                    return Ok(());
                }
            }
            let flag = self.stop_mining_flag.clone();
            flag.store(false, Ordering::Relaxed);

            let mut block_template = self.get_block_template()?;
            self.add_coinbase(&mut block_template)?;
            let mut block = self.get_block(block_template)?;
            let difficulty = self.get_req_difficulty()?;
            let result = CpuBlakePow::mine(
                difficulty,
                block.header,
                self.stop_mining_flag.clone(),
                self.kill_flag.clone(),
            );
            if result.is_some() {
                block.header = result.unwrap();
                self.tx.try_send(block);
            }
        }
        Ok(())
    }

    /// Stub function, temp use genesis block as template
    pub fn get_block_template(&self) -> Result<NewBlockTemplate, MinerError> {
        Ok(NewBlockTemplate::from(get_genesis_block()))
    }

    /// Stub function, temp use genesis block as template
    pub fn get_block(&self, block: NewBlockTemplate) -> Result<Block, MinerError> {
        Ok(get_genesis_block())
    }

    /// Stub function
    pub fn get_req_difficulty(&self) -> Result<Difficulty, MinerError> {
        Ok(Difficulty::min())
    }

    /// stub function, this function gets called when a new block event is triggered. It will ensure that the miner
    /// restarts/starts to mine.
    pub fn new_block_event(&mut self) {
        let flag = self.stop_mining_flag.clone();
        flag.store(true, Ordering::Relaxed);
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
        amount: MicroTari,
        maturity_height: u64,
    ) -> Result<(u64, PrivateKey, PrivateKey), MinerError>
    {
        let mut rng = rand::OsRng::new().unwrap();
        let r = PrivateKey::random(&mut rng);
        let key = PrivateKey::random(&mut rng);
        Ok((0, key, r))
    }

    /// Stub function to let wallet know about potential tx
    pub fn submit_tx_to_wallet(&self, tx: &Transaction, id: u64) {}
}
