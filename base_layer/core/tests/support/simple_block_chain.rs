// Copyright 2019 The Tari Project
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

use chrono::Duration;

use digest::Digest;
use merklemountainrange::mmr::*;
use rand::{CryptoRng, OsRng, Rng};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::prelude::*};
use tari_core::{
    blocks::{block::*, blockheader::*},
    consensus::ConsensusRules,
    fee::Fee,
    tari_amount::MicroTari,
    transaction::*,
    transaction_protocol::{
        build_challenge,
        sender::*,
        single_receiver::SingleReceiverTransactionProtocol,
        TransactionMetadata,
    },
    types::*,
};

use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    common::Blake256,
    keys::{PublicKey, SecretKey},
    range_proof::RangeProofService,
    ristretto::*,
};
use tari_utilities::{hash::Hashable, ByteArray};

/// This struct is used to keep track of what the value and private key of a UTXO is.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SpendInfo {
    pub key: PrivateKey,
    pub value: MicroTari,
    pub features: OutputFeatures,
}

impl SpendInfo {
    pub fn new(key: PrivateKey, value: MicroTari, features: OutputFeatures) -> SpendInfo {
        SpendInfo { key, value, features }
    }
}

/// This is used to represent a block chain in memory for testing purposes
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SimpleBlockChain {
    pub blocks: Vec<Block>,
    pub spending_keys: Vec<Vec<SpendInfo>>,
}

/// This is used to represent a block chain in memory for testing purposes
pub struct SimpleBlockChainBuilder {
    blockchain: SimpleBlockChain,
    headers: MerkleMountainRange<BlockHeader, SignatureHasher>,
    utxos: MerkleMountainRange<TransactionInput, SignatureHasher>,
    kernels: MerkleMountainRange<TransactionKernel, SignatureHasher>,
    rangeproofs: MerkleMountainRange<RangeProof, SignatureHasher>,
    rules: ConsensusRules,
}

impl SimpleBlockChainBuilder {
    /// This will create a new test block_chain with a Genesis block
    pub fn new() -> SimpleBlockChainBuilder {
        let mut chain = SimpleBlockChainBuilder {
            blockchain: Default::default(),
            headers: Default::default(),
            utxos: Default::default(),
            kernels: Default::default(),
            rangeproofs: Default::default(),
            rules: ConsensusRules::current(),
        };
        let mut rng = OsRng::new().unwrap();
        // create Genesis block
        chain.add_block(&mut rng, Vec::new());
        chain
    }

    /// This will create a new test block_chain with random txs spending all the utxo's at the spend height
    pub fn new_with_spending(block_amount: u64, spending_height: u64) -> SimpleBlockChainBuilder {
        let mut chain = SimpleBlockChainBuilder::new();

        let mut rng = OsRng::new().unwrap();
        // create gen block
        let priv_key = PrivateKey::random(&mut rng);
        chain.blockchain.spending_keys.push(vec![SpendInfo::new(
            priv_key.clone(),
            chain.rules.emission_schedule().block_reward(0),
            OutputFeatures::create_coinbase(0, &chain.rules),
        )]);
        let (cb_utxo, cb_kernel) = create_coinbase(priv_key, 0, 0.into(), &chain.rules);
        let block = BlockBuilder::new().with_coinbase_utxo(cb_utxo, cb_kernel).build();
        chain.processes_new_block(block);

        // lets mine some empty blocks
        if spending_height > 1 {
            chain.add_empty_blocks(&mut rng, spending_height - 1);
        }

        // lets mine some more blocks, but spending the utxo's in the older blocks
        for i in spending_height..(block_amount) {
            chain.blockchain.spending_keys.push(Vec::new());
            let (tx, mut spends) = chain.spend_block_utxos((i - spending_height) as usize);
            chain.add_block(&mut rng, tx);
            chain.blockchain.spending_keys[i as usize].append(&mut spends);
        }
        chain
    }

    /// This will add empty blocks to the chain
    pub fn add_empty_blocks<R: Rng + CryptoRng>(&mut self, rng: &mut R, count: u64) {
        for _ in 0..count {
            self.add_block(rng, Vec::new())
        }
    }

    /// Add a block to the chain with the given metadata
    fn add_block<R: Rng + CryptoRng>(&mut self, rng: &mut R, tx: Vec<Transaction>) {
        let priv_key = PrivateKey::random(rng);
        let height = self.blockchain.blocks.len() as u64;
        let header = if height > 0 {
            self.generate_new_header()
        } else {
            self.generate_genesis_block_header()
        };
        let total_fee = tx
            .iter()
            .fold(MicroTari::default(), |tot, tx| tot + tx.get_body().get_total_fee());
        let (cb_utxo, cb_kernel) = create_coinbase(priv_key.clone(), header.height, total_fee, &self.rules);
        self.blockchain.spending_keys.push(vec![SpendInfo::new(
            priv_key,
            self.rules.emission_schedule().block_reward(height) + total_fee,
            OutputFeatures::create_coinbase(height, &self.rules),
        )]);
        let block = BlockBuilder::new()
            .with_header(header)
            .with_coinbase_utxo(cb_utxo, cb_kernel)
            .with_transactions(tx)
            .build();
        self.processes_new_block(block);
    }

    /// This will blocks to the chain with random txs spending all the utxo's at the spend height
    pub fn add_with_spending(&mut self, block_amount: u64, spending_height: u64) {
        let mut rng = OsRng::new().unwrap();
        let len = self.blockchain.blocks.len() as u64;
        let mut blocks_added = 0;
        if len < spending_height {
            self.add_empty_blocks(&mut rng, spending_height - len);
            blocks_added += 1;
        };
        // lets mine some more blocks, but spending the utxo's in the older blocks
        let len = self.blockchain.blocks.len() as u64;
        for i in len..(len + block_amount - blocks_added) {
            self.blockchain.spending_keys.push(Vec::new());
            let (tx, mut spends) = self.spend_block_utxos((i - spending_height) as usize);
            self.add_block(&mut rng, tx);
            self.blockchain.spending_keys[i as usize].append(&mut spends);
        }
    }

    /// This function will just add the content of the block to the MMR's
    fn processes_new_block(&mut self, block: Block) {
        println!("Proc block nr: {:?}", self.blockchain.blocks.len());
        self.headers
            .push(block.header.clone())
            .expect("failed to add header to test chain");
        self.kernels
            .append(block.body.kernels.clone())
            .expect("failed to add kernels to test chain");

        for input in &block.body.inputs {
            let hash = input.clone().hash();
            self.utxos
                .prune_object_hash(&hash)
                .expect("failed to add pruned inputs");
        }
        for output in &block.body.outputs {
            self.rangeproofs
                .push(output.clone().proof)
                .expect("failed to add proofs to test chain");
            self.utxos
                .push(output.clone().into())
                .expect("failed to add outputs to test chain");
        }
        self.blockchain.blocks.push(block);
    }

    fn generate_genesis_block_header(&self) -> BlockHeader {
        BlockHeader::new(self.rules.blockchain_version())
    }

    /// This function will generate a new header, assuming it will follow on the last created block.
    fn generate_new_header(&self) -> BlockHeader {
        let counter = self.blockchain.blocks.len() - 1;
        let mut hash = [0; 32];
        hash.copy_from_slice(&self.blockchain.blocks[counter].header.hash());
        let mut hasher = SignatureHasher::new();
        hasher.input(&self.utxos.get_merkle_root()[..]);
        hasher.input(&self.utxos.get_unpruned_hash());
        let output_mr = hasher.result().to_vec();
        let kernal_mmr = self.kernels.get_merkle_root();
        let rr_mmr = self.rangeproofs.get_merkle_root();
        BlockHeader {
            version: self.rules.blockchain_version(),
            height: self.blockchain.blocks[counter].header.height + 1,
            prev_hash: hash,
            timestamp: self.blockchain.blocks[counter]
                .header
                .timestamp
                .clone()
                .checked_add_signed(Duration::minutes(1))
                .unwrap(),
            output_mr: array_ref!(output_mr, 0, 32).clone(),
            range_proof_mr: array_ref!(rr_mmr, 0, 32).clone(),
            kernel_mr: array_ref!(kernal_mmr, 0, 32).clone(),
            total_kernel_offset: RistrettoSecretKey::from(0),
            pow: ProofOfWork {
                work: self.blockchain.blocks[counter].header.pow.work + 1,
            },
        }
    }

    /// This function will spend the utxo's in the mentioned block
    fn spend_block_utxos(&mut self, block_index: usize) -> (Vec<Transaction>, Vec<SpendInfo>) {
        let utxo_count = self.blockchain.spending_keys[block_index as usize].len();
        let mut txs = Vec::new();
        let mut spends = Vec::new();
        let mut counter = 0;
        for i in 0..utxo_count {
            let result = self.create_tx(block_index, i, &mut counter);
            if result.is_some() {
                let (tx, mut spending_info) = result.unwrap();
                txs.push(tx);
                spends.append(&mut spending_info)
            }
        }
        (txs, spends)
    }

    /// This function will create a new transaction, spending the utxo specified by the block and utxo index
    fn create_tx(
        &self,
        block_index: usize,
        utxo_index: usize,
        counter: &mut usize,
    ) -> Option<(Transaction, Vec<SpendInfo>)>
    {
        let mut rng = OsRng::new().unwrap();
        let mut spend_info = Vec::new();

        // create keys
        let old_spend_key = self.blockchain.spending_keys[block_index][utxo_index].key.clone();
        let new_spend_key = PrivateKey::random(&mut rng);
        let new_spend_key2 = PrivateKey::random(&mut rng);
        // create values
        let old_value = self.blockchain.spending_keys[block_index][utxo_index].value;
        if old_value <= MicroTari(100) || *counter > 4 {
            // we dont want to keep dividing for ever on a single utxo, or create very large blocks
            return None;
        }
        let new_value = self.blockchain.spending_keys[block_index][utxo_index].value / 2;
        let fee = Fee::calculate(20.into(), 1, 2);
        if (new_value + fee + MicroTari(100)) >= (old_value) {
            // we dont want values smaller than 100 micro tari
            return None;
        }
        let new_value2 = old_value - new_value - fee;

        // save spend info
        spend_info.push(SpendInfo::new(
            new_spend_key.clone(),
            new_value,
            OutputFeatures::default(),
        ));
        spend_info.push(SpendInfo::new(
            new_spend_key2.clone(),
            new_value2,
            OutputFeatures::default(),
        ));

        // recreate input commitment
        let v_input = PrivateKey::from(old_value);
        let commitment_in = COMMITMENT_FACTORY.commit(&old_spend_key, &v_input);
        let input = TransactionInput::new(
            self.blockchain.spending_keys[block_index][utxo_index].features.clone(),
            commitment_in,
        );
        // create unblinded value
        let old_value = UnblindedOutput::new(old_value, old_spend_key, None);

        // generate kernel stuff
        let sender_offset = PrivateKey::random(&mut rng);
        let sender_r = PrivateKey::random(&mut rng);
        let receiver_r = PrivateKey::random(&mut rng);
        let mut builder = SenderTransactionProtocol::builder(1);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(MicroTari(20))
            .with_offset(sender_offset)
            .with_private_nonce(sender_r)
            .with_change_secret(new_spend_key.clone())
            .with_input(input, old_value)
            .with_amount(0, new_value2);
        let mut sender = builder.build::<Blake256>(&PROVER, &COMMITMENT_FACTORY).unwrap();

        let msg = sender.build_single_round_message().unwrap();
        let receiver_info = SingleReceiverTransactionProtocol::create(
            &msg,
            receiver_r,
            new_spend_key2,
            OutputFeatures::default(),
            &PROVER,
            &COMMITMENT_FACTORY,
        )
        .unwrap();

        sender
            .add_single_recipient_info(receiver_info.clone(), &PROVER)
            .unwrap();
        match sender.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
            Ok(true) => (),
            _ => {
                return None;
            },
        };

        let tx = sender.get_transaction().unwrap().clone();
        *counter += 1;
        Some((tx, spend_info))
    }
}

impl Default for SimpleBlockChain {
    fn default() -> Self {
        SimpleBlockChain {
            blocks: Vec::new(),
            spending_keys: Vec::new(),
        }
    }
}

/// This function will create a coinbase from the provided secret key. The coinbase will be added to the outputs and
/// kernels.
fn create_coinbase(
    key: PrivateKey,
    height: u64,
    total_fee: MicroTari,
    rules: &ConsensusRules,
) -> (TransactionOutput, TransactionKernel)
{
    let mut rng = rand::OsRng::new().unwrap();
    // build output
    let amount = total_fee + rules.emission_schedule().block_reward(height);
    let v = PrivateKey::from(u64::from(amount));
    let commitment = COMMITMENT_FACTORY.commit(&key, &v);
    let rr = PROVER.construct_proof(&key, amount.into()).unwrap();
    let output = TransactionOutput::new(
        OutputFeatures::create_coinbase(height, rules),
        commitment,
        RangeProof::from_bytes(&rr).unwrap(),
    );

    // create kernel
    let tx_meta = TransactionMetadata {
        fee: 0.into(),
        lock_height: 0,
    };
    let r = PrivateKey::random(&mut rng);
    let e = build_challenge(&PublicKey::from_secret_key(&r), &tx_meta);
    let s = Signature::sign(key.clone(), r, &e).unwrap();
    let excess = COMMITMENT_FACTORY.commit_value(&key, 0);
    let kernel = KernelBuilder::new()
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .with_fee(0.into())
        .with_lock_height(0)
        .with_excess(&excess)
        .with_signature(&s)
        .build()
        .unwrap();
    (output, kernel)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;

    //#[test]
    fn create_simple_block_chain() {
        let mut rng = rand::OsRng::new().unwrap();
        let mut chain = SimpleBlockChainBuilder::new();
        assert_eq!(chain.blockchain.blocks.len(), 1);
        chain.add_empty_blocks(&mut rng, 5);
        assert_eq!(chain.blockchain.blocks.len(), 6);

        // Check that the blocks form a  chain
        assert_eq!(chain.blockchain.blocks[0].header.height, 0);
        for i in 1..chain.blockchain.blocks.len() {
            let mut hash = [0; 32];
            hash.copy_from_slice(&chain.blockchain.blocks[i - 1].header.hash());
            assert_eq!(chain.blockchain.blocks[i].header.prev_hash, hash);
            assert_eq!(chain.blockchain.blocks[i].header.height, i as u64);
        }
    }

    // we dont want to run this function function every time as it basically tests, test code and it runs slow.
    // #[test]
    #[allow(dead_code)]
    fn create_simple_block_chain_with_spend() {
        let mut chain = SimpleBlockChainBuilder::new_with_spending(5, 1);
        assert_eq!(chain.blockchain.blocks.len(), 5);
        chain.add_with_spending(5, 1);
        assert_eq!(chain.blockchain.blocks.len(), 10);

        assert_eq!(chain.blockchain.blocks[0].header.height, 0);
        for i in 1..10 {
            let mut hash = [0; 32];
            hash.copy_from_slice(&chain.blockchain.blocks[i - 1].header.hash());
            assert_eq!(chain.blockchain.blocks[i].header.prev_hash, hash);
            assert_eq!(chain.blockchain.blocks[i].header.height, i as u64);
            for input in &chain.blockchain.blocks[i].body.inputs {
                assert!(chain.blockchain.blocks[i - 1]
                    .body
                    .outputs
                    .iter()
                    .any(|x| x.commitment == input.commitment));
            }
        }
    }

    // we dont want to run this function function every time as it basically tests, test code and it runs slow.
    //#[test]
    #[allow(dead_code)]
    fn test_json_file() {
        let mut chain = SimpleBlockChainBuilder::new_with_spending(5, 1);
        chain.add_with_spending(5, 1);
        let mut file = File::create("tests/chain/test_chain.json").unwrap();
        let json = serde_json::to_string_pretty(&chain.blockchain).unwrap();
        file.write_all(json.as_bytes()).unwrap();
        let read_json = fs::read_to_string("tests/chain/test_chain.json").unwrap();
        let blockchain: SimpleBlockChain = serde_json::from_str(&read_json).unwrap();
        assert_eq!(blockchain, chain.blockchain);
        fs::remove_file("tests/chain/test_chain.json").unwrap();
    }

    // we dont want to run this function function every time as it create a test file for use in testing
    //#[test]
    #[allow(dead_code)]
    fn create_json_file() {
        let mut chain = SimpleBlockChainBuilder::new_with_spending(5, 1);
        chain.add_with_spending(45, 1);
        let mut file = File::create("tests/chain/chain.json").unwrap();
        let json = serde_json::to_string_pretty(&chain.blockchain).unwrap();
        file.write_all(json.as_bytes()).unwrap();
    }
}
