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

use chrono::{DateTime, Duration, Utc};
use derive_error::Error;
use digest::Digest;
use rand::OsRng;
use tari_core::{
    blocks::{Block, BlockHeader},
    consensus::ConsensusRules,
    proof_of_work::*,
    tari_amount::MicroTari,
    transaction::*,
    types::*,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey},
    range_proof::RangeProofService,
};
use tari_utilities::byte_array::ByteArray;

/// TestNet Miner
pub struct Miner {
    // current working block
    block: Option<Block>,
    // prev block's solved difficulty, TODO check this
    difficulty: Difficulty,
    // current consensus rules
    rules: ConsensusRules,
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

    /// This function will mine the nonce and fill out the header.
    // Todo convert into futures with multi threading for this function
    pub fn mine(&mut self, old_header: BlockHeader) -> Result<(), MinerError> {
        if self.block.is_none() {
            return Err(MinerError::MissingBlock);
        }
        let interval = self.block.as_ref().unwrap().header.timestamp.timestamp() - old_header.timestamp.timestamp();
        let difficulty = Difficulty::calculate_req_difficulty(interval, self.difficulty);
        let nonce = BlakePow::mine(difficulty, &self.block.as_ref().unwrap().header);
        self.block.as_mut().unwrap().header.nonce = nonce;
        self.difficulty = difficulty;
        Ok(())
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
