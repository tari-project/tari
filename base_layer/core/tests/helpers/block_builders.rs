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

use std::{convert::TryFrom, ops::Deref, sync::Arc};

use croaring::Bitmap;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::types::FixedHash;
use tari_core::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader, NewBlockTemplate},
    chain_storage::{
        calculate_validator_node_mr,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
    },
    consensus::{emission::Emission, ConsensusConstants, ConsensusManager, ConsensusManagerBuilder},
    proof_of_work::{sha3x_difficulty, AchievedTargetDifficulty, Difficulty},
    test_helpers::blockchain::{create_store_with_consensus, TempDatabase},
    transactions::{
        tari_amount::MicroTari,
        test_helpers::{
            create_wallet_output_with_data,
            schema_to_transaction,
            spend_utxos,
            TestKeyManager,
            TestParams,
            TransactionSchema,
        },
        transaction_components::{OutputFeatures, Transaction, TransactionKernel, TransactionOutput, WalletOutput},
        CoinbaseBuilder,
    },
    txn_schema,
    KernelMmr,
    KernelMmrHasherBlake256,
    MutableOutputMmr,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_mmr::{Hash, MutableMmr};
use tari_script::script;

pub async fn create_coinbase_with_coinbase_builder(
    constants: &ConsensusConstants,
    block_emission: MicroTari,
    block_height: u64,
    fees: MicroTari,
    extra: Option<Vec<u8>>,
    key_manager: &TestKeyManager,
) -> (TransactionOutput, TransactionKernel, WalletOutput) {
    let p = TestParams::new(key_manager).await;

    let builder = CoinbaseBuilder::new(key_manager.clone())
        .with_block_height(block_height)
        .with_fees(fees)
        .with_spend_key_id(p.spend_key_id.clone())
        .with_script_key_id(p.script_key_id.clone());
    let builder = match extra {
        Some(extra) => builder.with_extra(extra),
        None => builder,
    };
    let (coinbase_transaction, coinbase_output) = builder.build_with_emission(constants, block_emission).await.unwrap();

    (
        coinbase_transaction.body.outputs()[0].clone(),
        coinbase_transaction.body.kernels()[0].clone(),
        coinbase_output,
    )
}

async fn genesis_template(
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (NewBlockTemplate, WalletOutput) {
    let header = BlockHeader::new(consensus_constants.blockchain_version());
    let (utxo, kernel, output) = create_coinbase_with_coinbase_builder(
        consensus_constants,
        coinbase_value,
        0,
        0.into(),
        Some(b"The big bang".to_vec()),
        key_manager,
    )
    .await;
    let block = NewBlockTemplate::from_block(
        header.into_builder().with_coinbase_utxo(utxo, kernel).build(),
        Difficulty::min(),
        coinbase_value,
    );
    (block, output)
}

#[test]
fn print_new_genesis_block_values() {
    let vn_mr = calculate_validator_node_mr(&[]);
    let validator_node_mr = FixedHash::try_from(vn_mr).unwrap();

    // Note: An em empty MMR will have a root of `MerkleMountainRange::<D, B>::null_hash()`
    let kernel_mr = KernelMmr::new(Vec::new()).get_merkle_root().unwrap();
    let output_mr = MutableOutputMmr::new(Vec::new(), Bitmap::create())
        .unwrap()
        .get_merkle_root()
        .unwrap();

    // Note: This is printed in the same order as needed for 'fn get_xxxx_genesis_block_raw()'
    println!();
    println!("Genesis block constants");
    println!();
    println!("header output_mr:           {}", output_mr.to_hex());
    println!("header output_mmr_size:     0");
    println!("header kernel_mr:           {}", kernel_mr.to_hex());
    println!("header kernel_mmr_size:     0");
    println!("header validator_node_mr:   {}", validator_node_mr.to_hex());
    println!("header total_kernel_offset: {}", FixedHash::zero().to_hex());
    println!("header total_script_offset: {}", FixedHash::zero().to_hex());
}

// Calculate the MMR Merkle roots for the genesis block template and update the header.
fn update_genesis_block_mmr_roots(template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
    type BaseLayerKernelMutableMmr = MutableMmr<KernelMmrHasherBlake256, Vec<Hash>>;

    let NewBlockTemplate { header, mut body, .. } = template;
    // Make sure the body components are sorted. If they already are, this is a very cheap call.
    body.sort();
    let kernel_hashes: Vec<Vec<u8>> = body.kernels().iter().map(|k| k.hash().to_vec()).collect();
    let out_hashes: Vec<Vec<u8>> = body.outputs().iter().map(|out| out.hash().to_vec()).collect();

    let mut header = BlockHeader::from(header);
    header.kernel_mr = FixedHash::try_from(
        BaseLayerKernelMutableMmr::new(kernel_hashes, Bitmap::create())
            .unwrap()
            .get_merkle_root()?,
    )
    .unwrap();
    header.output_mr = FixedHash::try_from(
        MutableOutputMmr::new(out_hashes, Bitmap::create())
            .unwrap()
            .get_merkle_root()?,
    )
    .unwrap();
    Ok(Block { header, body })
}

pub async fn create_blockchain_with_spendable_coinbase(
    key_manager: &TestKeyManager,
    network: Network,
    consensus_constants: &Option<ConsensusConstants>,
) -> (
    ChainBlock,
    WalletOutput,
    ConsensusManager,
    BlockchainDatabase<TempDatabase>,
) {
    let mut builder = ConsensusManagerBuilder::new(network);
    if let Some(consensus_constants) = consensus_constants {
        builder = builder.add_consensus_constants(consensus_constants.clone());
    }
    let consensus_manager = builder.build().unwrap();
    let genesis_block = consensus_manager.get_genesis_block().unwrap();
    let blockchain_db = create_store_with_consensus(consensus_manager.clone()).unwrap();
    // Add 1st block to get hold of the coinbase
    let (mut tip_block, first_coinbase) = append_block_with_coinbase(
        &blockchain_db,
        &genesis_block,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        key_manager,
    )
    .await
    .unwrap();
    for _i in 0..=consensus_manager.consensus_constants(0).coinbase_min_maturity() {
        (tip_block, _) = append_block_with_coinbase(
            &blockchain_db,
            &tip_block.clone(),
            vec![],
            &consensus_manager,
            Difficulty::min(),
            key_manager,
        )
        .await
        .unwrap();
    }
    (tip_block, first_coinbase, consensus_manager, blockchain_db)
}

pub async fn create_blockchain_with_utxos(
    values: &[MicroTari],
    key_manager: &TestKeyManager,
) -> (
    ChainBlock,
    Vec<WalletOutput>,
    ConsensusManager,
    BlockchainDatabase<TempDatabase>,
) {
    let (block_1, coinbase_1, consensus_manager, blockchain_db) =
        create_blockchain_with_spendable_coinbase(key_manager, Network::LocalNet, &None).await;
    // Spend the coinbase to the required outputs
    let (txns, wallet_outputs) = schema_to_transaction(
        &[txn_schema!(from: vec![coinbase_1], to: values.to_vec(), fee: 1.into())],
        key_manager,
    )
    .await;
    let txns = txns
        .into_iter()
        .map(|t| t.deref().clone())
        .collect::<Vec<Transaction>>();
    // Add 2nd block to generate the outputs
    let (block_2, _coinbase_2) = append_block_with_coinbase(
        &blockchain_db,
        &block_1,
        txns,
        &consensus_manager,
        Difficulty::min(),
        key_manager,
    )
    .await
    .unwrap();
    (block_2, wallet_outputs, consensus_manager, blockchain_db)
}

/// Create a Genesis block with additional utxos that are immediately available for spending. This is useful for
/// writing tests without having to add blocks just so the coinbase output can mature.
#[allow(dead_code)]
pub async fn create_genesis_block_with_utxos(
    values: &[MicroTari],
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (ChainBlock, Vec<WalletOutput>) {
    let (mut template, coinbase) = genesis_template(100_000_000.into(), consensus_constants, key_manager).await;
    let script = script!(Nop);
    let output_features = OutputFeatures::default();
    let mut outputs = Vec::new();
    outputs.push(coinbase);
    for value in values {
        let p = TestParams::new(key_manager).await;
        let wallet_output =
            create_wallet_output_with_data(script.clone(), output_features.clone(), &p, *value, key_manager)
                .await
                .unwrap();
        outputs.push(wallet_output.clone());
        let output = wallet_output.to_transaction_output(key_manager).await.unwrap();
        template.body.add_output(output);
    }
    let mut block = update_genesis_block_mmr_roots(template).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from_u64(1).unwrap());
    let hash = block.hash();
    (
        ChainBlock::try_construct(block.into(), BlockHeaderAccumulatedData {
            hash,
            total_kernel_offset: Default::default(),
            achieved_difficulty: Difficulty::min(),
            total_accumulated_difficulty: 1,
            accumulated_randomx_difficulty: Difficulty::min(),
            accumulated_sha3x_difficulty: Difficulty::min(),
            target_difficulty: Difficulty::min(),
        })
        .unwrap(),
        outputs,
    )
}

/// Create a new block using the provided transactions that adds to the blockchain given in `prev_block`.
// This function is used, unclear why clippy says it isn't.
#[allow(dead_code)]
pub async fn chain_block(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> NewBlockTemplate {
    let mut header = BlockHeader::from_previous(&prev_block.header);
    let height = header.height;
    header.version = consensus.consensus_constants(height).blockchain_version();
    let fees = transactions
        .iter()
        .fold(MicroTari::zero(), |total, txn| total + txn.body.get_total_fee());
    let emission = consensus.get_block_emission_at(height);
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase_with_coinbase_builder(
        consensus.consensus_constants(height),
        emission,
        height,
        fees,
        None,
        key_manager,
    )
    .await;
    NewBlockTemplate::from_block(
        header
            .into_builder()
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .with_transactions(transactions)
            .build(),
        Difficulty::min(),
        emission,
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
        Difficulty::min(),
        consensus.get_block_emission_at(height),
    )
}

/// Create a new block using the provided coinbase and transactions that adds to the blockchain given in `prev_block`.
pub async fn chain_block_with_new_coinbase(
    prev_block: &ChainBlock,
    transactions: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    extra: Option<Vec<u8>>,
    key_manager: &TestKeyManager,
) -> (NewBlockTemplate, WalletOutput) {
    let height = prev_block.height() + 1;
    let fees = transactions
        .iter()
        .fold(MicroTari::zero(), |total, txn| total + txn.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase_with_coinbase_builder(
        consensus_manager.consensus_constants(height),
        consensus_manager.emission_schedule().block_emission(height),
        height + consensus_manager.consensus_constants(height).coinbase_min_maturity(),
        fees,
        extra,
        key_manager,
    )
    .await;
    let mut header = BlockHeader::from_previous(prev_block.header());
    header.height = height;
    header.version = consensus_manager
        .consensus_constants(header.height)
        .blockchain_version();
    let reward = consensus_manager.get_block_emission_at(header.height);
    let template = NewBlockTemplate::from_block(
        header
            .into_builder()
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
        Difficulty::min(),
        reward,
    );
    (template, coinbase_output)
}

/// Create a new block with the provided transactions. The new MMR roots are calculated, and then the new block is
/// added to the database. The newly created block is returned as the result.
pub async fn append_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &ChainBlock,
    txns: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    achieved_difficulty: Difficulty,
    key_manager: &TestKeyManager,
) -> Result<ChainBlock, ChainStorageError> {
    append_block_with_coinbase(
        db,
        prev_block,
        txns,
        consensus_manager,
        achieved_difficulty,
        key_manager,
    )
    .await
    .map(|(b, _)| b)
}

/// Create a new block with the provided transactions and add a coinbase output. The new MMR roots are calculated, and
/// then the new block is added to the database. The newly created block is returned as the result.
pub async fn append_block_with_coinbase<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &ChainBlock,
    transactions: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    achieved_difficulty: Difficulty,
    key_manager: &TestKeyManager,
) -> Result<(ChainBlock, WalletOutput), ChainStorageError> {
    let height = prev_block.height() + 1;
    let coinbase_value = consensus_manager.emission_schedule().block_emission(height);
    let fees = transactions
        .iter()
        .fold(MicroTari::zero(), |total, txn| total + txn.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase_with_coinbase_builder(
        consensus_manager.consensus_constants(height),
        coinbase_value,
        height + consensus_manager.consensus_constants(height).coinbase_min_maturity(),
        fees,
        None,
        key_manager,
    )
    .await;
    let template = chain_block_with_coinbase(
        prev_block,
        transactions,
        coinbase_utxo,
        coinbase_kernel,
        consensus_manager,
    );
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
pub async fn generate_new_block<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<WalletOutput>>,
    schemas: Vec<TransactionSchema>,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let coinbase_value = consensus
        .emission_schedule()
        .block_emission(db.get_height().unwrap() + 1);
    generate_new_block_with_coinbase(db, blocks, outputs, schemas, coinbase_value, consensus, key_manager).await
}

#[allow(dead_code)]
pub async fn generate_new_block_with_achieved_difficulty<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<WalletOutput>>,
    schemas: Vec<TransactionSchema>,
    achieved_difficulty: Difficulty,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    for schema in schemas {
        let (tx, mut utxos) = spend_utxos(schema, key_manager).await;
        txns.push(tx);
        block_utxos.append(&mut utxos);
    }
    outputs.push(block_utxos);
    generate_block_with_achieved_difficulty(db, blocks, txns, achieved_difficulty, consensus, key_manager).await
}

/// Generate a new block using the given transaction schema and coinbase value and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub async fn generate_new_block_with_coinbase<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<WalletOutput>>,
    schemas: Vec<TransactionSchema>,
    coinbase_value: MicroTari,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    let mut fees = MicroTari(0);
    for schema in schemas {
        let (tx, mut utxos) = spend_utxos(schema, key_manager).await;
        fees += tx.body.get_total_fee();
        txns.push(tx);
        block_utxos.append(&mut utxos);
    }

    let height = blocks.last().unwrap().height();
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase_with_coinbase_builder(
        consensus.consensus_constants(height),
        coinbase_value,
        height + consensus.consensus_constants(height).coinbase_min_maturity(),
        fees,
        None,
        key_manager,
    )
    .await;

    block_utxos.push(coinbase_output);

    outputs.push(block_utxos);
    generate_block_with_coinbase(db, blocks, txns, coinbase_utxo, coinbase_kernel, consensus)
}

pub fn find_header_with_achieved_difficulty(header: &mut BlockHeader, achieved_difficulty: Difficulty) {
    let mut num_tries = 0;

    while sha3x_difficulty(header).unwrap() != achieved_difficulty {
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
/// This function is not able to determine the wallet outputs of a transaction, so if you are mixing using this
/// with [generate_new_block], you must update the unblinded UTXO vector yourself.
#[allow(dead_code)]
pub async fn generate_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let prev_block = blocks.last().unwrap();
    let template = chain_block_with_new_coinbase(prev_block, transactions, consensus, None, key_manager)
        .await
        .0;
    let new_block = db.prepare_new_block(template)?;
    let result = db.add_block(new_block.into());
    if let Ok(BlockAddResult::Ok(ref b)) = result {
        blocks.push(b.as_ref().clone());
    }
    result
}

#[allow(dead_code)]
pub async fn generate_block_with_achieved_difficulty<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    transactions: Vec<Transaction>,
    achieved_difficulty: Difficulty,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let template = chain_block_with_new_coinbase(blocks.last().unwrap(), transactions, consensus, None, key_manager)
        .await
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
pub async fn construct_chained_blocks<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    block0: ChainBlock,
    consensus: &ConsensusManager,
    n: usize,
    key_manager: &TestKeyManager,
) -> Vec<ChainBlock> {
    let mut prev_block = block0;
    let mut blocks = Vec::new();
    for _i in 0..n {
        let block = append_block(db, &prev_block, vec![], consensus, Difficulty::min(), key_manager)
            .await
            .unwrap();
        prev_block = block.clone();
        blocks.push(block);
    }
    blocks
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
