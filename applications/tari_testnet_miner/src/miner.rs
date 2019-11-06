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

use crate::t_blake_pow::TestBlakePow;
use core::sync::atomic::AtomicBool;
use derive_error::Error;
use digest::Digest;
use futures::{
    channel::{
        mpsc,
        mpsc::{Receiver, Sender},
    },
    future::poll_fn,
    stream::StreamExt,
};
use std::sync::{atomic::Ordering, Arc};
use tari_core::{
    blocks::{Block, BlockHeader},
    proof_of_work::Difficulty,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey},
    range_proof::RangeProofService,
};
use tari_transactions::{consensus::ConsensusRules, tari_amount::MicroTari, transaction::*, types::*};
use tari_utilities::byte_array::ByteArray;
use tokio_executor::threadpool::{blocking, ThreadPool};

/// TestNet Miner
pub struct Miner {
    // current working block
    block: Option<Block>,
    // prev block's solved difficulty, TODO check this
    difficulty: Difficulty,
    // current consensus rules
    rules: ConsensusRules,
    // Stop mining flag
    stop_mine: Arc<AtomicBool>,
    // Amount of threads the Miner can use
    thread_count: u8,
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum MinerError {
    // Could not construct the coinbase utxo and kernel for the block
    CoinbaseError,
    // No block provided to mine
    MissingBlock,
}

impl Miner {
    /// Create a new empty miner
    pub fn new(rules: ConsensusRules) -> Miner {
        Miner {
            block: None,
            difficulty: Difficulty::min(),
            rules,
            stop_mine: Arc::new(AtomicBool::new(false)),
            thread_count: 2,
        }
    }

    /// Add a block to be processed.  This function also add the coinbase tx
    pub fn add_block(&mut self, mut block: Block) -> Result<(), MinerError> {
        let coinbase_value = block.calculate_coinbase_and_fees(&self.rules);
        let (coinbase, kernel) = self.create_coinbase_tx(coinbase_value, &block);
        block.body.add_output(coinbase);
        block.body.add_kernel(kernel);
        self.block = Some(block);
        Ok(())
    }

    /// Temp code, this needs to come from the wallet.
    fn create_coinbase_tx(&self, coinbase_value: MicroTari, block: &Block) -> (TransactionOutput, TransactionKernel) {
        let mut rng = rand::OsRng::new().unwrap();
        let coinbase_key = PrivateKey::random(&mut rng);
        let new_commitment = COMMITMENT_FACTORY.commit(&coinbase_key, &coinbase_value.into());
        let rr = PROVER.construct_proof(&coinbase_key, coinbase_value.into()).unwrap();
        let coinbase = TransactionOutput {
            commitment: new_commitment,
            features: OutputFeatures::create_coinbase(block.header.height, &self.rules),
            proof: RangeProof::from_bytes(&rr).unwrap(),
        };
        let excess = COMMITMENT_FACTORY.commit(&coinbase_key, &(MicroTari(0)).into());
        let nonce = PrivateKey::random(&mut rng);
        let challenge = Miner::get_challenge(&PublicKey::from_secret_key(&nonce));
        let sig = Signature::sign(coinbase_key, nonce, &challenge).unwrap();
        let kernel = TransactionKernel {
            features: KernelFeatures::empty(),
            fee: 0.into(),
            lock_height: 0,
            excess,
            excess_sig: sig,
            meta_info: None,
            linked_kernel: None,
        };
        (coinbase, kernel)
    }

    async fn mining(
        difficulty: Difficulty,
        header: BlockHeader,
        stop_flag: Arc<AtomicBool>,
        mut tx: Sender<BlockHeader>,
    )
    {
        poll_fn(move |_| {
            blocking(|| {
                let result = TestBlakePow::mine(difficulty.clone(), header.clone(), stop_flag.clone());
                tx.try_send(result);
            })
        })
        .await
        .expect("Couldn't block");
    }

    /// This function will mine the nonce and fill out the header.
    // Todo convert into futures with multi threading for this function
    pub async fn mine(&mut self, old_header: BlockHeader, pool: &mut ThreadPool) -> Result<(), MinerError> {
        if self.block.is_none() {
            return Err(MinerError::MissingBlock);
        }
        let interval = self.block.as_ref().unwrap().header.timestamp.timestamp() - old_header.timestamp.timestamp();
        let difficulty = Difficulty::min(); // replace with new function: Difficulty::calculate_req_difficulty(interval, self.difficulty);

        let (tx, mut rx): (Sender<BlockHeader>, Receiver<BlockHeader>) = mpsc::channel(1);
        for _ in 0..self.thread_count {
            pool.spawn(Miner::mining(
                difficulty,
                self.block.clone().unwrap().header.clone(),
                self.stop_mine.clone(),
                tx.clone(),
            ));
        }
        let flag = self.stop_mine.clone();
        let recv_fut = async move {
            let rcvd = rx.next().await;
            flag.store(true, Ordering::Relaxed);
            rcvd.unwrap()
        };

        let _rcvd = recv_fut.await;
        self.difficulty = difficulty;
        Ok(())
    }

    /// This gets the arc pointer to the mining control flag
    pub fn get_mine_flag(&self) -> Arc<AtomicBool> {
        self.stop_mine.clone()
    }

    /// This function swaps out the current block with a newly created empty block
    pub fn get_block(&mut self) -> Option<Block> {
        if self.block.is_none() {
            return None;
        };
        std::mem::replace(&mut self.block, None)
    }

    /// This constructs a challenge for the coinbase tx
    fn get_challenge(nonce: &PublicKey) -> MessageHash {
        Challenge::new()
            .chain(nonce.as_bytes())
            .chain(&(0 as u64).to_le_bytes())
            .chain(&(0 as u64).to_le_bytes())
            .result()
            .to_vec()
    }
}
