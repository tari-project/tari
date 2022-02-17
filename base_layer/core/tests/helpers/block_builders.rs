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

use std::{iter::repeat_with, sync::Arc};

use croaring::Bitmap;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::types::{Commitment, HashDigest, HashOutput, PublicKey};
use tari_core::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader, NewBlockTemplate},
    chain_storage::{BlockAddResult, BlockchainBackend, BlockchainDatabase, ChainStorageError},
    consensus::{emission::Emission, ConsensusConstants, ConsensusManager, ConsensusManagerBuilder},
    covenants::Covenant,
    proof_of_work::{sha3_difficulty, AchievedTargetDifficulty, Difficulty},
    transactions::{
        tari_amount::MicroTari,
        test_helpers::{
            create_random_signature_from_s_key,
            create_signature,
            create_unblinded_output,
            create_utxo,
            spend_utxos,
            TestParams,
            TransactionSchema,
        },
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionKernel,
            TransactionOutput,
            UnblindedOutput,
        },
        CryptoFactories,
    },
};
use tari_crypto::{
    keys::PublicKey as PublicKeyTrait,
    script,
    tari_utilities::{hash::Hashable, hex::Hex},
};
use tari_mmr::MutableMmr;

pub fn create_coinbase(
    factories: &CryptoFactories,
    value: MicroTari,
    maturity_height: u64,
) -> (TransactionOutput, TransactionKernel, UnblindedOutput) {
    let p = TestParams::new();

    let excess = Commitment::from_public_key(&PublicKey::from_secret_key(&p.spend_key));
    let sig = create_signature(p.spend_key.clone(), 0.into(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    let unblinded_output =
        create_unblinded_output(script!(Nop), OutputFeatures::create_coinbase(maturity_height), p, value);
    let output = unblinded_output.as_transaction_output(factories).unwrap();

    (output, kernel, unblinded_output)
}

fn genesis_template(
    factories: &CryptoFactories,
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
) -> (NewBlockTemplate, UnblindedOutput) {
    let header = BlockHeader::new(consensus_constants.blockchain_version());
    let (utxo, kernel, output) = create_coinbase(factories, coinbase_value, consensus_constants.coinbase_lock_height());
    let block = NewBlockTemplate::from_block(
        header.into_builder().with_coinbase_utxo(utxo, kernel).build(),
        1.into(),
        coinbase_value,
    );
    (block, output)
}

#[test]
// #[ignore = "used to generate a new genesis block"]
/// This is a helper function to generate and print out a block that can be used as the genesis block.
/// 1. Pick a network
/// 1. Run `cargo test --package tari_core --test mempool -- helpers::block_builders::print_new_genesis_block --exact
/// --nocapture --ignored`
/// 1. The block and range proof will be printed
/// 1. Profit!
fn print_new_genesis_block() {
    let network = Network::Dibbler;

    let consensus_manager: ConsensusManager = ConsensusManagerBuilder::new(network).build();
    let factories = CryptoFactories::default();
    let mut header = BlockHeader::new(consensus_manager.consensus_constants(0).blockchain_version());
    let value = consensus_manager.emission_schedule().block_reward(0);
    let (utxo, key, _) = create_utxo(
        value,
        &factories,
        OutputFeatures::create_coinbase(60),
        &script![Nop],
        &Covenant::default(),
    );
    let (pk, sig) = create_random_signature_from_s_key(key.clone(), 0.into(), 0);
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    header.kernel_mr = kernel.hash();
    header.kernel_mmr_size += 1;
    header.output_mr = utxo.hash();
    header.witness_mr = utxo.witness_hash();
    header.output_mmr_size += 1;

    let block = header.into_builder().with_coinbase_utxo(utxo, kernel).build();
    println!("{}", block);
    println!("spending key: {}", key.to_hex());
    println!("range proof: {}", block.body.outputs()[0].proof.to_hex());
}

/// Create a genesis block returning it with the spending key for the coinbase utxo
///
/// Right now this function does not use consensus rules to generate the block. The coinbase output has an arbitrary
/// value, and the maturity is zero.
pub fn create_genesis_block(
    factories: &CryptoFactories,
    consensus_constants: &ConsensusConstants,
) -> (ChainBlock, UnblindedOutput) {
    create_genesis_block_with_coinbase_value(factories, consensus_constants.emission_amounts().0, consensus_constants)
}

// Calculate the MMR Merkle roots for the genesis block template and update the header.
fn update_genesis_block_mmr_roots(template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
    let NewBlockTemplate { header, mut body, .. } = template;
    // Make sure the body components are sorted. If they already are, this is a very cheap call.
    body.sort();
    let kernel_hashes: Vec<HashOutput> = body.kernels().iter().map(|k| k.hash()).collect();
    let out_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.hash()).collect();
    let rp_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.witness_hash()).collect();

    let mut header = BlockHeader::from(header);
    header.kernel_mr = MutableMmr::<HashDigest, _>::new(kernel_hashes, Bitmap::create())
        .unwrap()
        .get_merkle_root()?;
    header.output_mr = MutableMmr::<HashDigest, _>::new(out_hashes, Bitmap::create())
        .unwrap()
        .get_merkle_root()?;
    header.witness_mr = MutableMmr::<HashDigest, _>::new(rp_hashes, Bitmap::create())
        .unwrap()
        .get_merkle_root()?;
    Ok(Block { header, body })
}

/// Create a genesis block with the specified coinbase value, returning it with the spending key for the coinbase utxo.
pub fn create_genesis_block_with_coinbase_value(
    factories: &CryptoFactories,
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
) -> (ChainBlock, UnblindedOutput) {
    let (template, output) = genesis_template(factories, coinbase_value, consensus_constants);
    let mut block = update_genesis_block_mmr_roots(template).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    let hash = block.hash();
    (
        ChainBlock::try_construct(block.into(), BlockHeaderAccumulatedData {
            hash,
            total_kernel_offset: Default::default(),
            achieved_difficulty: 1.into(),
            total_accumulated_difficulty: 1,
            accumulated_monero_difficulty: 1.into(),
            accumulated_sha_difficulty: 1.into(),
            target_difficulty: 1.into(),
        })
        .unwrap(),
        output,
    )
}

/// Create a Genesis block with additional utxos that are immediately available for spending. This is useful for
/// writing tests without having to add blocks just so the coinbase output can mature.
#[allow(dead_code)]
pub fn create_genesis_block_with_utxos(
    factories: &CryptoFactories,
    values: &[MicroTari],
    consensus_constants: &ConsensusConstants,
) -> (ChainBlock, Vec<UnblindedOutput>) {
    let (mut template, coinbase) = genesis_template(factories, 100_000_000.into(), consensus_constants);
    let script = script!(Nop);
    let output_features = OutputFeatures::default();
    let outputs = values.iter().fold(vec![coinbase], |mut secrets, v| {
        let p = TestParams::new();
        let unblinded_output = create_unblinded_output(script.clone(), output_features.clone(), p, *v);
        secrets.push(unblinded_output.clone());
        let output = unblinded_output.as_transaction_output(factories).unwrap();
        template.body.add_output(output);
        secrets
    });
    let mut block = update_genesis_block_mmr_roots(template).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    let hash = block.hash();
    (
        ChainBlock::try_construct(block.into(), BlockHeaderAccumulatedData {
            hash,
            total_kernel_offset: Default::default(),
            achieved_difficulty: 1.into(),
            total_accumulated_difficulty: 1,
            accumulated_monero_difficulty: 1.into(),
            accumulated_sha_difficulty: 1.into(),
            target_difficulty: 1.into(),
        })
        .unwrap(),
        outputs,
    )
}

/// Create a new block using the provided transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
) -> NewBlockTemplate {
    let mut header = BlockHeader::from_previous(&prev_block.header);
    header.version = consensus.consensus_constants(header.height).blockchain_version();
    let height = header.height;
    let reward = consensus.get_block_reward_at(height);
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        &Default::default(),
        reward,
        consensus.consensus_constants(height).coinbase_lock_height(),
    );
    NewBlockTemplate::from_block(
        header
            .into_builder()
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .with_transactions(transactions)
            .build(),
        1.into(),
        reward,
    )
}

/// Create a new block using the provided coinbase and transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block_with_coinbase(
    prev_block: &ChainBlock,
    transactions: Vec<Transaction>,
    coinbase_utxo: TransactionOutput,
    coinbase_kernel: TransactionKernel,
    consensus: &ConsensusManager,
) -> NewBlockTemplate {
    let mut header = BlockHeader::from_previous(prev_block.header());
    header.version = consensus.consensus_constants(header.height).blockchain_version();
    let height = header.height;
    NewBlockTemplate::from_block(
        header
            .into_builder()
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
        1.into(),
        consensus.get_block_reward_at(height),
    )
}

/// Create a new block using the provided coinbase and transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block_with_new_coinbase(
    prev_block: &ChainBlock,
    transactions: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    factories: &CryptoFactories,
) -> (NewBlockTemplate, UnblindedOutput) {
    let height = prev_block.height() + 1;
    let mut coinbase_value = consensus_manager.emission_schedule().block_reward(height);
    coinbase_value += transactions
        .iter()
        .fold(MicroTari(0), |acc, x| acc + x.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(
        factories,
        coinbase_value,
        height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
    );
    let mut header = BlockHeader::from_previous(prev_block.header());
    header.height = height;
    header.version = consensus_manager
        .consensus_constants(header.height)
        .blockchain_version();
    let reward = consensus_manager.get_block_reward_at(header.height);
    let template = NewBlockTemplate::from_block(
        header
            .into_builder()
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
        1.into(),
        reward,
    );
    (template, coinbase_output)
}

/// Create a new block with the provided transactions. The new MMR roots are calculated, and then the new block is
/// added to the database. The newly created block is returned as the result.
pub fn append_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &ChainBlock,
    txns: Vec<Transaction>,
    consensus: &ConsensusManager,
    achieved_difficulty: Difficulty,
) -> Result<ChainBlock, ChainStorageError> {
    append_block_with_coinbase(
        &CryptoFactories::default(),
        db,
        prev_block,
        txns,
        consensus,
        achieved_difficulty,
    )
    .map(|(b, _)| b)
}

/// Create a new block with the provided transactions and add a coinbase output. The new MMR roots are calculated, and
/// then the new block is added to the database. The newly created block is returned as the result.
pub fn append_block_with_coinbase<B: BlockchainBackend>(
    factories: &CryptoFactories,
    db: &BlockchainDatabase<B>,
    prev_block: &ChainBlock,
    txns: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    achieved_difficulty: Difficulty,
) -> Result<(ChainBlock, UnblindedOutput), ChainStorageError> {
    let height = prev_block.height() + 1;
    let mut coinbase_value = consensus_manager.emission_schedule().block_reward(height);
    coinbase_value += txns.iter().fold(MicroTari(0), |acc, x| acc + x.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(
        factories,
        coinbase_value,
        height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
    );
    let template = chain_block_with_coinbase(prev_block, txns, coinbase_utxo, coinbase_kernel, consensus_manager);
    let mut block = db.prepare_new_block(template)?;
    block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut block.header, achieved_difficulty);
    let res = db.add_block(Arc::new(block))?;
    match res {
        BlockAddResult::Ok(b) => Ok((b.as_ref().clone(), coinbase_output)),
        BlockAddResult::BlockExists => Err(ChainStorageError::InvalidOperation("Block already exists".to_string())),
        BlockAddResult::OrphanBlock => Err(ChainStorageError::InvalidOperation("Block added as orphan".to_string())),
        BlockAddResult::ChainReorg { .. } => Err(ChainStorageError::InvalidOperation(
            "Chain reorged unexpectedly".to_string(),
        )),
    }
}

/// Generate a new block using the given transaction schema and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub fn generate_new_block<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let coinbase_value = consensus.emission_schedule().block_reward(db.get_height().unwrap() + 1);
    generate_new_block_with_coinbase(
        db,
        &CryptoFactories::default(),
        blocks,
        outputs,
        schemas,
        coinbase_value,
        consensus,
    )
}

#[allow(dead_code)]
pub fn generate_new_block_with_achieved_difficulty<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    achieved_difficulty: Difficulty,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    for schema in schemas {
        let (tx, mut utxos) = spend_utxos(schema);
        txns.push(tx);
        block_utxos.append(&mut utxos);
    }
    outputs.push(block_utxos);
    generate_block_with_achieved_difficulty(db, blocks, txns, achieved_difficulty, consensus)
}

/// Generate a new block using the given transaction schema and coinbase value and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub fn generate_new_block_with_coinbase<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    factories: &CryptoFactories,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    coinbase_value: MicroTari,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    let mut fees = MicroTari(0);
    for schema in schemas {
        let (tx, mut utxos) = spend_utxos(schema);
        fees += tx.body.get_total_fee();
        txns.push(tx);
        block_utxos.append(&mut utxos);
    }

    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(factories, coinbase_value + fees, 100);
    block_utxos.push(coinbase_output);

    outputs.push(block_utxos);
    generate_block_with_coinbase(db, blocks, txns, coinbase_utxo, coinbase_kernel, consensus)
}

pub fn find_header_with_achieved_difficulty(header: &mut BlockHeader, achieved_difficulty: Difficulty) {
    let mut num_tries = 0;

    while sha3_difficulty(header) != achieved_difficulty {
        header.nonce += 1;
        num_tries += 1;
        if num_tries > 10_000_000 {
            // Just in case we burn a hole in the CI server
            panic!("Could not find a nonce for achieved difficulty in time");
        }
    }
}

/// Generate a block and add it to the database using the transactions provided. The header will be updated with the
/// correct MMR roots.
/// This function is not able to determine the unblinded outputs of a transaction, so if you are mixing using this
/// with [generate_new_block], you must update the unblinded UTXO vector yourself.
#[allow(dead_code)]
pub fn generate_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let prev_block = blocks.last().unwrap();
    let template = chain_block_with_new_coinbase(prev_block, transactions, consensus, &CryptoFactories::default()).0;
    let new_block = db.prepare_new_block(template)?;
    let result = db.add_block(new_block.into());
    if let Ok(BlockAddResult::Ok(ref b)) = result {
        blocks.push(b.as_ref().clone());
    }
    result
}

#[allow(dead_code)]
pub fn generate_block_with_achieved_difficulty<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    transactions: Vec<Transaction>,
    achieved_difficulty: Difficulty,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let template = chain_block_with_new_coinbase(
        blocks.last().unwrap(),
        transactions,
        consensus,
        &CryptoFactories::default(),
    )
    .0;
    let mut new_block = db.prepare_new_block(template)?;
    new_block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut new_block.header, achieved_difficulty);
    let result = db.add_block(new_block.into());
    if let Ok(BlockAddResult::Ok(ref b)) = result {
        blocks.push(b.as_ref().clone());
    }
    result
}

/// Generate a block and add it to the database using the provided transactions and coinbase. The header will be updated
/// with the correct MMR roots.
pub fn generate_block_with_coinbase<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    transactions: Vec<Transaction>,
    coinbase_utxo: TransactionOutput,
    coinbase_kernel: TransactionKernel,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let template = chain_block_with_coinbase(
        blocks.last().unwrap(),
        transactions,
        coinbase_utxo,
        coinbase_kernel,
        consensus,
    );
    let new_block = db.prepare_new_block(template)?;
    let result = db.add_block(new_block.into())?;
    if let BlockAddResult::Ok(ref b) = result {
        blocks.push(b.as_ref().clone());
    }
    Ok(result)
}

#[allow(dead_code)]
pub fn construct_chained_blocks<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    block0: ChainBlock,
    consensus: &ConsensusManager,
    n: usize,
) -> Vec<ChainBlock> {
    let mut prev_block = block0;

    repeat_with(|| {
        let block = append_block(db, &prev_block, vec![], consensus, 1.into()).unwrap();
        prev_block = block.clone();
        block
    })
    .take(n)
    .collect()
}

#[allow(dead_code)]
pub fn create_chain_header(header: BlockHeader, prev_accum: &BlockHeaderAccumulatedData) -> ChainHeader {
    let achieved_target_diff = AchievedTargetDifficulty::try_construct(
        header.pow_algo(),
        prev_accum.target_difficulty,
        prev_accum.achieved_difficulty,
    )
    .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(prev_accum)
        .with_hash(header.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(header.total_kernel_offset.clone())
        .build()
        .unwrap();
    ChainHeader::try_construct(header, accumulated_data).unwrap()
}
