// Copyright 2020. The Tari Project
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

use std::{collections::HashMap, sync::Arc};

use log::*;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_core::{
    blocks::Block,
    chain_storage::{BlockAddResult, BlockchainDatabase, ChainStorageError},
    consensus::ConsensusManager,
    test_helpers::blockchain::TempDatabase,
    transactions::{transaction_components::UnblindedOutput, CryptoFactories},
};
use tari_crypto::tari_utilities::Hashable;

use crate::helpers::{
    block_builders::{chain_block_with_new_coinbase, find_header_with_achieved_difficulty},
    block_proxy::BlockProxy,
    sample_blockchains::create_new_blockchain,
    test_block_builder::{TestBlockBuilder, TestBlockBuilderInner},
};

const LOG_TARGET: &str = "tari_core::tests::helpers::test_blockchain";

pub struct TestBlockchain {
    store: BlockchainDatabase<TempDatabase>,
    blocks: HashMap<String, BlockProxy>,
    hash_to_block: HashMap<Vec<u8>, String>,
    consensus_manager: ConsensusManager,
    outputs: Vec<Vec<UnblindedOutput>>,
}

#[allow(dead_code)]
impl TestBlockchain {
    pub fn with_genesis(genesis_name: &'static str) -> Self {
        let network = Network::LocalNet;
        let (store, mut b, outputs, consensus_manager) = create_new_blockchain(network);

        let name = genesis_name.to_string();
        let mut blocks = HashMap::new();
        let genesis_block = b.pop().unwrap();
        let mut hash_to_block = HashMap::new();
        hash_to_block.insert(genesis_block.hash().clone(), name.clone());
        blocks.insert(name.clone(), BlockProxy::new(name, genesis_block));

        Self {
            store,
            blocks,
            hash_to_block,
            consensus_manager,
            outputs,
        }
    }

    pub fn store(&self) -> &BlockchainDatabase<TempDatabase> {
        &self.store
    }

    pub fn consensus_manager(&self) -> &ConsensusManager {
        &self.consensus_manager
    }

    pub fn build_block(&self, block: TestBlockBuilderInner) -> (Block, UnblindedOutput) {
        debug!(target: LOG_TARGET, "Adding block '{}' to test block chain", block.name);
        let prev_block = self.blocks.get(&block.child_of.unwrap());
        let prev_block = prev_block.map(|b| &b.block).unwrap();
        let (template, output) = chain_block_with_new_coinbase(
            prev_block,
            block.transactions,
            &self.consensus_manager,
            &CryptoFactories::default(),
        );

        let mut new_block = self.store.prepare_new_block(template).unwrap();
        new_block.header.nonce = OsRng.next_u64();
        find_header_with_achieved_difficulty(&mut new_block.header, block.difficulty.unwrap_or(1).into());

        (new_block, output)
    }

    pub fn add_block(&mut self, block: TestBlockBuilderInner) -> (BlockAddResult, UnblindedOutput) {
        let block_name = block.name.clone();
        let (block, output) = self.build_block(block);
        self.outputs.push(vec![output.clone()]);
        let res = self.add_raw_block(&block_name, block).unwrap();
        (res, output)
    }

    pub fn add_raw_block(&mut self, block_name: &str, block: Block) -> Result<BlockAddResult, ChainStorageError> {
        let res = self.store.add_block(Arc::new(block))?;
        if let BlockAddResult::Ok(ref b) = res {
            self.hash_to_block.insert(b.hash().clone(), block_name.to_string());
            self.blocks.insert(
                block_name.to_string(),
                BlockProxy::new(block_name.to_string(), b.as_ref().clone()),
            );
        }

        Ok(res)
    }

    pub fn outputs_at(&self, height: u64) -> &[UnblindedOutput] {
        self.outputs.get(height as usize).unwrap()
    }

    pub fn builder(&mut self) -> TestBlockBuilder {
        TestBlockBuilder {}
    }

    pub fn orphan_count(&self) -> usize {
        self.store.orphan_count().unwrap()
    }

    pub fn tip(&self) -> &BlockProxy {
        let tip = self.store.fetch_tip_header().unwrap();
        self.get_block_by_hash(tip.hash()).unwrap()
    }

    pub fn get_block(&self, name: &str) -> Option<&BlockProxy> {
        self.blocks.get(name)
    }

    pub fn get_block_by_hash(&self, hash: &[u8]) -> Option<&BlockProxy> {
        let block_name = self.hash_to_block.get(hash);
        block_name.map(|bn| self.blocks.get(bn).unwrap())
    }

    pub fn chain(&self) -> Vec<&str> {
        let mut result = vec![];
        let (mut tip, _) = self.store.fetch_tip_header().unwrap().into_parts();

        while tip.height > 0 {
            result.push(self.get_block_by_hash(&tip.hash()).unwrap().name.as_str());
            tip = self.store.fetch_header_by_block_hash(tip.prev_hash).unwrap().unwrap();
        }
        result.push(self.get_block_by_hash(&tip.hash()).unwrap().name.as_str());

        result.reverse();
        result
    }
}
