// Copyright 2022. The Tari Project
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

use tari_core::{
    blocks::Block,
    transactions::{
        tari_amount::T,
        test_helpers::schema_to_transaction,
        transaction_components::{TransactionInputVersion, TransactionOutputVersion},
    },
    txn_schema,
};

use super::test_blockchain::TestBlockchain;

#[allow(dead_code)]
enum MerkleMountainRangeField {
    Input,
    Output,
    Witness,
    Kernel,
}

#[allow(dead_code)]
pub async fn check_input_malleability(block_mod_fn: impl Fn(&mut Block)) {
    check_block_changes_are_detected(MerkleMountainRangeField::Input, block_mod_fn).await;
}

#[allow(dead_code)]
pub async fn check_output_malleability(block_mod_fn: impl Fn(&mut Block)) {
    check_block_changes_are_detected(MerkleMountainRangeField::Output, block_mod_fn).await;
}

#[allow(dead_code)]
pub async fn check_witness_malleability(block_mod_fn: impl Fn(&mut Block)) {
    check_block_changes_are_detected(MerkleMountainRangeField::Witness, block_mod_fn).await;
}

#[allow(dead_code)]
pub async fn check_kernel_malleability(block_mod_fn: impl Fn(&mut Block)) {
    check_block_changes_are_detected(MerkleMountainRangeField::Kernel, block_mod_fn).await;
}

#[allow(dead_code)]
async fn check_block_changes_are_detected(field: MerkleMountainRangeField, block_mod_fn: impl Fn(&mut Block)) {
    // create a blockchain with a couple of valid blocks
    let mut blockchain = TestBlockchain::with_genesis("GB").await;
    let blocks = blockchain.builder();

    let (_, output) = blockchain
        .add_block(blocks.new_block("A1").child_of("GB").difficulty(1))
        .await;

    let (txs, _) = schema_to_transaction(
        &[txn_schema!(
            from: vec![output],
            to: vec![50 * T],
            input_version: TransactionInputVersion::V0,
            output_version: TransactionOutputVersion::V0
        )],
        &blockchain.key_manager,
    )
    .await;
    blockchain
        .add_block(
            blocks
                .new_block("A2")
                .child_of("A1")
                .difficulty(1)
                .with_transactions(txs.into_iter().map(|tx| Clone::clone(&*tx)).collect()),
        )
        .await;

    // store the block metadata for later comparison
    let block = blockchain.get_block("A2").cloned().unwrap().block;

    // create an altered version of the last block, using the provided modification function
    let mut mod_block = block.block().clone();
    block_mod_fn(&mut mod_block);

    // calculate the metadata for the modified block
    blockchain
        .store()
        .rewind_to_height(mod_block.header.height - 1)
        .unwrap();
    let (mut mod_block, modded_root) = blockchain.store().calculate_mmr_roots(mod_block).unwrap();

    match field {
        MerkleMountainRangeField::Input => {
            assert_ne!(block.header().input_mr, modded_root.input_mr);
            mod_block.header.input_mr = modded_root.input_mr;
        },
        MerkleMountainRangeField::Output => {
            assert_ne!(block.header().output_mr, modded_root.output_mr);
            mod_block.header.output_mr = modded_root.output_mr;
        },
        MerkleMountainRangeField::Witness => {
            assert_ne!(block.header().witness_mr, modded_root.witness_mr);
            mod_block.header.witness_mr = modded_root.witness_mr;
        },
        MerkleMountainRangeField::Kernel => {
            assert_ne!(block.header().kernel_mr, modded_root.kernel_mr);
            mod_block.header.kernel_mr = modded_root.kernel_mr;
        },
    }

    assert_ne!(block.block().hash(), mod_block.hash());
}
