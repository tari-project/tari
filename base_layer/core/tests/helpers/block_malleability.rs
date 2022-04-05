use tari_core::{
    blocks::Block,
    transactions::{
        tari_amount::T,
        test_helpers::schema_to_transaction,
    },
    txn_schema,
};
use tari_utilities::Hashable;

use super::test_blockchain::TestBlockchain;

enum MerkleMountainRangeField {
    Input,
    Output,
    Witness,
}

pub fn check_input_malleability(block_mod_fn: impl Fn(&mut Block) -> ()) {
    check_block_changes_are_detected(MerkleMountainRangeField::Input, block_mod_fn);
}

pub fn check_output_malleability(block_mod_fn: impl Fn(&mut Block) -> ()) {
    check_block_changes_are_detected(MerkleMountainRangeField::Output, block_mod_fn);
}

pub fn check_witness_malleability(block_mod_fn: impl Fn(&mut Block) -> ()) {
    check_block_changes_are_detected(MerkleMountainRangeField::Witness, block_mod_fn);
}

fn check_block_changes_are_detected(field: MerkleMountainRangeField, block_mod_fn: impl Fn(&mut Block) -> ()) {
    // create a blockchain with a couple of valid blocks
    let mut blockchain = TestBlockchain::with_genesis("GB");
    let blocks = blockchain.builder();

    let (_, output) = blockchain.add_block(blocks.new_block("A1").child_of("GB").difficulty(1));

    let (txs, _) = schema_to_transaction(&[txn_schema!(from: vec![output], to: vec![50 * T])]);
    blockchain.add_block(
        blocks
            .new_block("A2")
            .child_of("A1")
            .difficulty(1)
            .with_transactions(txs.into_iter().map(|tx| Clone::clone(&*tx)).collect()),
    );

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
    }

    assert_ne!(block.block().hash(), mod_block.hash());
}
