//  Copyright 2020, The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::convert::TryInto;

use taiji_core::{
    blocks::{Block, BlockHeader, NewBlockTemplate},
    consensus::{emission::Emission, ConsensusManager},
    proof_of_work::Difficulty,
    transactions::{taiji_amount::MicroMinotaiji, test_helpers::TestKeyManager, transaction_components::Transaction},
};

use crate::helpers::block_builders::create_coinbase;

/// Create a partially constructed block using the provided set of transactions
/// is chain_block, or rename it to `create_orphan_block` and drop the prev_block argument
#[allow(dead_code)]
pub async fn create_orphan_block(
    block_height: u64,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Block {
    let mut coinbase_value = consensus.emission_schedule().block_reward(block_height);
    let lock_height = consensus.consensus_constants(block_height).coinbase_min_maturity();
    coinbase_value += transactions
        .iter()
        .fold(MicroMinotaiji(0), |acc, x| acc + x.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, _coinbase_output) =
        create_coinbase(coinbase_value, block_height + lock_height, None, key_manager).await;
    let mut header = BlockHeader::new(consensus.consensus_constants(block_height).blockchain_version());
    header.prev_hash = Vec::from([1u8; 32]).try_into().unwrap(); // Random
    header.height = block_height;

    let template = NewBlockTemplate::from_block(
        header
            .into_builder()
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
        Difficulty::min(),
        coinbase_value,
    );
    Block::new(template.header.into(), template.body)
}

#[allow(dead_code)]
pub fn create_block(block_version: u16, block_height: u64, transactions: Vec<Transaction>) -> Block {
    let mut header = BlockHeader::new(block_version);
    header.height = block_height;
    header.into_builder().with_transactions(transactions).build()
}
