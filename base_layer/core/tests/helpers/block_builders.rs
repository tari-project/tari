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

use std::{convert::TryFrom, sync::Arc};

use croaring::Bitmap;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::types::{Commitment, FixedHash};
use tari_core::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader, NewBlockTemplate},
    chain_storage::{
        calculate_validator_node_mr,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
    },
    consensus::{emission::Emission, ConsensusConstants, ConsensusManager},
    proof_of_work::{sha3x_difficulty, AchievedTargetDifficulty, Difficulty},
    transactions::{
        key_manager::{TransactionKeyManagerBranch, TransactionKeyManagerInterface, TxoStage},
        tari_amount::MicroTari,
        test_helpers::{create_wallet_output_with_data, spend_utxos, TestKeyManager, TestParams, TransactionSchema},
        transaction_components::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            WalletOutput,
        },
    },
    KernelMmr,
    KernelMmrHasherBlake256,
    MutableOutputMmr,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_mmr::{Hash, MutableMmr};
use tari_script::script;

pub async fn create_coinbase(
    value: MicroTari,
    maturity_height: u64,
    extra: Option<Vec<u8>>,
    key_manager: &TestKeyManager,
) -> (TransactionOutput, TransactionKernel, WalletOutput) {
    let p = TestParams::new(key_manager).await;
    let public_exess = key_manager.get_public_key_at_key_id(&p.spend_key_id).await.unwrap();
    let (nonce, public_nonce) = key_manager
        .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
        .await
        .unwrap();

    let excess = Commitment::from_public_key(&public_exess);
    let kernel_features = KernelFeatures::create_coinbase();
    let kernel_message = TransactionKernel::build_kernel_signature_message(
        &TransactionKernelVersion::get_current_version(),
        0.into(),
        0,
        &kernel_features,
        &None,
    );

    let sig = key_manager
        .get_partial_txo_kernel_signature(
            &p.spend_key_id,
            &nonce,
            &public_nonce,
            &public_exess,
            &TransactionKernelVersion::get_current_version(),
            &kernel_message,
            &kernel_features,
            TxoStage::Output,
        )
        .await
        .unwrap();
    let kernel = KernelBuilder::new()
        .with_signature(sig)
        .with_excess(&excess)
        .with_features(kernel_features)
        .build()
        .unwrap();

    let wallet_output = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::create_coinbase(maturity_height, extra),
        &p,
        value,
        key_manager,
    )
    .await
    .unwrap();
    let output = wallet_output.as_transaction_output(key_manager).await.unwrap();

    (output, kernel, wallet_output)
}

async fn genesis_template(
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (NewBlockTemplate, WalletOutput) {
    let header = BlockHeader::new(consensus_constants.blockchain_version());
    let (utxo, kernel, output) = create_coinbase(
        coinbase_value,
        consensus_constants.coinbase_lock_height(),
        Some(b"The big bang".to_vec()),
        key_manager,
    )
    .await;
    let block = NewBlockTemplate::from_block(
        header.into_builder().with_coinbase_utxo(utxo, kernel).build(),
        1.into(),
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

/// Create a genesis block returning it with the spending key for the coinbase utxo
///
/// Right now this function does not use consensus rules to generate the block. The coinbase output has an arbitrary
/// value, and the maturity is zero.
pub async fn create_genesis_block(
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (ChainBlock, WalletOutput) {
    create_genesis_block_with_coinbase_value(
        consensus_constants.emission_amounts().0,
        consensus_constants,
        key_manager,
    )
    .await
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

/// Create a genesis block with the specified coinbase value, returning it with the spending key for the coinbase utxo.
pub async fn create_genesis_block_with_coinbase_value(
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (ChainBlock, WalletOutput) {
    let (template, output) = genesis_template(coinbase_value, consensus_constants, key_manager).await;
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
        let output = wallet_output.as_transaction_output(key_manager).await.unwrap();
        template.body.add_output(output);
    }
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
// This function is used, unclear why clippy says it isn't.
#[allow(dead_code)]
pub async fn chain_block(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> NewBlockTemplate {
    let mut header = BlockHeader::from_previous(&prev_block.header);
    header.version = consensus.consensus_constants(header.height).blockchain_version();
    let height = header.height;
    let reward = consensus.get_block_reward_at(height);
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        reward,
        consensus.consensus_constants(height).coinbase_lock_height(),
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
pub async fn chain_block_with_new_coinbase(
    prev_block: &ChainBlock,
    transactions: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    extra: Option<Vec<u8>>,
    key_manager: &TestKeyManager,
) -> (NewBlockTemplate, WalletOutput) {
    let height = prev_block.height() + 1;
    let mut coinbase_value = consensus_manager.emission_schedule().block_reward(height);
    coinbase_value += transactions
        .iter()
        .fold(MicroTari(0), |acc, x| acc + x.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(
        coinbase_value,
        height + consensus_manager.consensus_constants(height).coinbase_lock_height(),
        extra,
        key_manager,
    )
    .await;
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
pub async fn append_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &ChainBlock,
    txns: Vec<Transaction>,
    consensus: &ConsensusManager,
    achieved_difficulty: Difficulty,
    key_manager: &TestKeyManager,
) -> Result<ChainBlock, ChainStorageError> {
    append_block_with_coinbase(db, prev_block, txns, consensus, achieved_difficulty, key_manager)
        .await
        .map(|(b, _)| b)
}

/// Create a new block with the provided transactions and add a coinbase output. The new MMR roots are calculated, and
/// then the new block is added to the database. The newly created block is returned as the result.
pub async fn append_block_with_coinbase<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &ChainBlock,
    txns: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    achieved_difficulty: Difficulty,
    key_manager: &TestKeyManager,
) -> Result<(ChainBlock, WalletOutput), ChainStorageError> {
    let height = prev_block.height() + 1;
    let mut coinbase_value = consensus_manager.emission_schedule().block_reward(height);
    coinbase_value += txns.iter().fold(MicroTari(0), |acc, x| acc + x.body.get_total_fee());
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(
        coinbase_value,
        height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
        None,
        key_manager,
    )
    .await;
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
pub async fn generate_new_block<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<ChainBlock>,
    outputs: &mut Vec<Vec<WalletOutput>>,
    schemas: Vec<TransactionSchema>,
    consensus: &ConsensusManager,
    key_manager: &TestKeyManager,
) -> Result<BlockAddResult, ChainStorageError> {
    let coinbase_value = consensus.emission_schedule().block_reward(db.get_height().unwrap() + 1);
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

    let (coinbase_utxo, coinbase_kernel, coinbase_output) =
        create_coinbase(coinbase_value + fees, 100, None, key_manager).await;
    block_utxos.push(coinbase_output);

    outputs.push(block_utxos);
    generate_block_with_coinbase(db, blocks, txns, coinbase_utxo, coinbase_kernel, consensus)
}

pub fn find_header_with_achieved_difficulty(header: &mut BlockHeader, achieved_difficulty: Difficulty) {
    let mut num_tries = 0;

    while sha3x_difficulty(header) != achieved_difficulty {
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
        let block = append_block(db, &prev_block, vec![], consensus, 1.into(), key_manager)
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
