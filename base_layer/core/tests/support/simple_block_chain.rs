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

use merklemountainrange::mmr::*;
use rand::OsRng;
use tari_core::{
    block::*,
    blockheader::*,
    fee::Fee,
    tari_amount::*,
    transaction::*,
    transaction_protocol::{sender::*, single_receiver::SingleReceiverTransactionProtocol},
    types::*,
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, common::Blake256, keys::SecretKey, ristretto::*};
use tari_utilities::hash::Hashable;

/// This struct is used to keep track of what the value and private key of a UTXO is.
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
pub struct SimpleBlockChain {
    blocks: Vec<Block>,
    spending_keys: Vec<Vec<SpendInfo>>,
    headers: MerkleMountainRange<BlockHeader, SignatureHash>,
    utxos: MerkleMountainRange<TransactionInput, SignatureHash>,
    kernels: MerkleMountainRange<TransactionKernel, SignatureHash>,
}
impl SimpleBlockChain {
    /// This will create a new test block_chain with empty blocks
    pub fn new(block_amount: u64) -> SimpleBlockChain {
        let mut chain = SimpleBlockChain::default();

        let mut rng = OsRng::new().unwrap();
        // create gen block
        let priv_key = PrivateKey::random(&mut rng);
        chain.spending_keys.push(vec![SpendInfo::new(
            priv_key.clone(),
            calculate_coinbase(0),
            OutputFeatures::COINBASE_OUTPUT,
        )]);
        let block = BlockBuilder::new().create_coinbase(priv_key).build_with_pow();
        chain.processes_new_block(block);

        // lets mine some more blocks
        for i in 1..(block_amount) {
            let priv_key = PrivateKey::random(&mut rng);
            chain.spending_keys.push(vec![SpendInfo::new(
                priv_key.clone(),
                calculate_coinbase(i.into()),
                OutputFeatures::COINBASE_OUTPUT,
            )]);
            let header = chain.generate_new_header();
            let block = BlockBuilder::new()
                .with_header(header)
                .create_coinbase(priv_key)
                .build_with_pow();
            chain.processes_new_block(block);
        }
        chain
    }

    /// This will add empty blocks to the chain
    pub fn add(&mut self, block_amount: u64) {
        let mut rng = OsRng::new().unwrap();
        for i in 0..(block_amount) {
            let priv_key = PrivateKey::random(&mut rng);
            self.spending_keys.push(vec![SpendInfo::new(
                priv_key.clone(),
                calculate_coinbase(i),
                OutputFeatures::COINBASE_OUTPUT,
            )]);
            let header = self.generate_new_header();
            let block = BlockBuilder::new()
                .with_header(header)
                .create_coinbase(priv_key)
                .build_with_pow();
            self.processes_new_block(block);
        }
    }

    /// This will create a new test block_chain with random txs spending all the utxo's at the spend height
    pub fn new_with_spending(block_amount: u64, spending_height: u64) -> SimpleBlockChain {
        let mut chain = SimpleBlockChain::default();

        let mut rng = OsRng::new().unwrap();
        // create gen block
        let priv_key = PrivateKey::random(&mut rng);
        chain.spending_keys.push(vec![SpendInfo::new(
            priv_key.clone(),
            calculate_coinbase(0),
            OutputFeatures::COINBASE_OUTPUT,
        )]);
        let block = BlockBuilder::new().create_coinbase(priv_key).build_with_pow();
        chain.processes_new_block(block);

        // lets mine some empty blocks
        for i in 1..(spending_height) {
            let priv_key = PrivateKey::random(&mut rng);
            chain.spending_keys.push(vec![SpendInfo::new(
                priv_key.clone(),
                calculate_coinbase(i),
                OutputFeatures::COINBASE_OUTPUT,
            )]);
            let header = chain.generate_new_header();
            let block = BlockBuilder::new()
                .with_header(header)
                .create_coinbase(priv_key)
                .build_with_pow();
            chain.processes_new_block(block);
        }

        // lets mine some more blocks, but spending the utxo's in the older blocks
        for i in spending_height..(block_amount) {
            let priv_key = PrivateKey::random(&mut rng);
            let header = chain.generate_new_header();
            chain.spending_keys.push(Vec::new());
            let (tx, mut spends) = chain.spend_block_utxos((i - spending_height) as usize);
            let block = BlockBuilder::new()
                .with_header(header)
                .with_transactions(tx)
                .create_coinbase(priv_key.clone())
                .build_with_pow();
            let fee = block.body.get_total_fee();
            chain.processes_new_block(block);
            spends.push(SpendInfo::new(
                priv_key,
                calculate_coinbase(i) + fee,
                OutputFeatures::COINBASE_OUTPUT,
            ));
            chain.spending_keys[i as usize].append(&mut spends);
        }
        chain
    }

    /// This will blocks to the chain with random txs spending all the utxo's at the spend height
    pub fn add_with_spending(&mut self, block_amount: u64, spending_height: u64) {
        let mut rng = OsRng::new().unwrap();
        let len = self.blocks.len() as u64;
        let mut blocks_added = 0;
        if len < spending_height {
            self.add(spending_height - len);
            blocks_added += 1;
        };
        // lets mine some more blocks, but spending the utxo's in the older blocks
        let len = self.blocks.len() as u64;
        for i in len..(len + block_amount - blocks_added) {
            let priv_key = PrivateKey::random(&mut rng);
            let header = self.generate_new_header();
            self.spending_keys.push(Vec::new());
            let (tx, mut spends) = self.spend_block_utxos((i - spending_height) as usize);
            let block = BlockBuilder::new()
                .with_header(header)
                .with_transactions(tx)
                .create_coinbase(priv_key.clone())
                .build_with_pow();
            let fee = block.body.get_total_fee();
            self.processes_new_block(block);
            spends.push(SpendInfo::new(
                priv_key,
                calculate_coinbase(i) + fee,
                OutputFeatures::COINBASE_OUTPUT,
            ));
            self.spending_keys[i as usize].append(&mut spends);
        }
    }

    /// This function will just add the content of the block to the MMR's
    fn processes_new_block(&mut self, block: Block) {
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
            self.utxos
                .push(output.clone().into())
                .expect("failed to add outputs to test chain");
        }
        self.blocks.push(block);
    }

    /// This function will generate a new header, assuming it will follow on the last created block.
    fn generate_new_header(&self) -> BlockHeader {
        let counter = self.blocks.len() - 1;
        let mut hash = [0; 32];
        hash.copy_from_slice(&self.blocks[counter].header.hash());
        BlockHeader {
            version: BLOCKCHAIN_VERSION,
            height: self.blocks[counter].header.height + 1,
            prev_hash: hash,
            timestamp: self.blocks[counter]
                .header
                .timestamp
                .clone()
                .checked_add_signed(Duration::minutes(1))
                .unwrap(),
            output_mr: [0; 32],
            range_proof_mr: [0; 32],
            kernel_mr: [0; 32],
            total_kernel_offset: RistrettoSecretKey::from(0),
            pow: POW::default(),
        }
    }

    /// This function will spend the utxo's in the mentioned block
    fn spend_block_utxos(&mut self, block_index: usize) -> (Vec<Transaction>, Vec<SpendInfo>) {
        let amount_of_utxo = self.spending_keys[block_index as usize].len();
        let mut txs = Vec::new();
        let mut spends = Vec::new();
        for i in 0..amount_of_utxo {
            let result = self.create_tx(block_index, i);
            if result.is_some() {
                let (tx, mut spending_info) = result.unwrap();
                txs.push(tx);
                spends.append(&mut spending_info)
            }
        }
        (txs, spends)
    }

    /// This function will create a new transaction, spending the utxo specified by the block and utxo index
    fn create_tx(&self, block_index: usize, utxo_index: usize) -> Option<(Transaction, Vec<SpendInfo>)> {
        let mut rng = OsRng::new().unwrap();
        let mut spend_info = Vec::new();

        // create keys
        let old_spend_key = self.spending_keys[block_index][utxo_index].key.clone();
        let new_spend_key = PrivateKey::random(&mut rng);
        let new_spend_key2 = PrivateKey::random(&mut rng);
        // create values
        let old_value = self.spending_keys[block_index][utxo_index].value;
        if old_value <= MicroTari(100) || utxo_index > 4 {
            // we dont want to keep dividing for ever on a single utxo, or create very large blocks
            return None;
        }
        let new_value = self.spending_keys[block_index][utxo_index].value / 2;
        let fee = Fee::calculate(20.into(), 1, 2);
        let new_value2 = old_value - new_value - fee;

        // save spend info
        spend_info.push(SpendInfo::new(
            new_spend_key.clone(),
            new_value,
            OutputFeatures::empty(),
        ));
        spend_info.push(SpendInfo::new(
            new_spend_key2.clone(),
            new_value2,
            OutputFeatures::empty(),
        ));

        // recreate input commitment
        let v_input = PrivateKey::from(old_value);
        let commitment_in = COMMITMENT_FACTORY.commit(&old_spend_key, &v_input);
        let input = TransactionInput::new(self.spending_keys[block_index][utxo_index].features, commitment_in);
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
            OutputFeatures::empty(),
            &PROVER,
            &COMMITMENT_FACTORY,
        )
        .unwrap();

        sender
            .add_single_recipient_info(receiver_info.clone(), &PROVER)
            .unwrap();
        match sender.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY) {
            Ok(true) => (),
            _ => return None,
        };

        let tx = sender.get_transaction().unwrap().clone();
        Some((tx, spend_info))
    }
}

impl Default for SimpleBlockChain {
    fn default() -> Self {
        SimpleBlockChain {
            blocks: Vec::new(),
            spending_keys: Vec::new(),
            headers: MerkleMountainRange::new(),
            utxos: MerkleMountainRange::new(),
            kernels: MerkleMountainRange::new(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn create_simple_block_chain() {
        let mut chain = SimpleBlockChain::new_with_spending(5, 1);
        assert_eq!(chain.blocks.len(), 5);
        chain.add(5);
        assert_eq!(chain.blocks.len(), 10);

        assert_eq!(chain.blocks[0].header.height, 0);
        for i in 1..10 {
            let mut hash = [0; 32];
            hash.copy_from_slice(&chain.blocks[i - 1].header.hash());
            assert_eq!(chain.blocks[i].header.prev_hash, hash);
            assert_eq!(chain.blocks[i].header.height, i as u64);
        }
    }

    // todo fix the performance issues with issue #473
    // #[test]
    fn create_simple_block_chain_with_spend() {
        let mut chain = SimpleBlockChain::new_with_spending(5, 1);
        assert_eq!(chain.blocks.len(), 5);
        chain.add_with_spending(5, 1);
        assert_eq!(chain.blocks.len(), 10);

        assert_eq!(chain.blocks[0].header.height, 0);
        for i in 1..10 {
            let mut hash = [0; 32];
            hash.copy_from_slice(&chain.blocks[i - 1].header.hash());
            assert_eq!(chain.blocks[i].header.prev_hash, hash);
            assert_eq!(chain.blocks[i].header.height, i as u64);
            for input in &chain.blocks[i].body.inputs {
                assert!(chain.blocks[i - 1]
                    .body
                    .outputs
                    .iter()
                    .any(|x| x.commitment == input.commitment));
            }
        }
    }
}
