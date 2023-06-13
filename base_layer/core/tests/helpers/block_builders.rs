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
use tari_common::configuration::Network;
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
    consensus::{emission::Emission, ConsensusConstants, ConsensusManager, ConsensusManagerBuilder},
    covenants::Covenant,
    proof_of_work::{sha3x_difficulty, AchievedTargetDifficulty, Difficulty},
    test_helpers::{create_test_core_key_manager_with_memory_db, TestKeyManager},
    transaction_key_manager::{BaseLayerKeyManagerInterface, CoreKeyManagerBranch, TxoStage},
    transactions::{
        tari_amount::MicroTari,
        test_helpers::{
            create_key_manager_output_with_data,
            create_random_signature_from_secret_key,
            create_utxo,
            spend_utxos,
            TestParams,
            TransactionSchema,
        },
        transaction_components::{
            transaction_output::batch_verify_range_proofs,
            KernelBuilder,
            KernelFeatures,
            KeyManagerOutput,
            OutputFeatures,
            Transaction,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
        },
        CryptoFactories,
    },
    KernelMmr,
    KernelMmrHasherBlake256,
    MutableOutputMmr,
    WitnessMmr,
    WitnessMmrHasherBlake256,
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
) -> (TransactionOutput, TransactionKernel, KeyManagerOutput) {
    let p = TestParams::new(key_manager).await;
    let public_exess = key_manager.get_public_key_at_key_id(&p.spend_key_id).await.unwrap();
    let (nonce, public_nonce) = key_manager
        .get_next_key(CoreKeyManagerBranch::Nonce.get_branch_key())
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
        .get_txo_kernel_signature(
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

    let key_manager_output = create_key_manager_output_with_data(
        script!(Nop),
        OutputFeatures::create_coinbase(maturity_height, extra),
        &p,
        value,
        key_manager,
    )
    .await
    .unwrap();
    let output = key_manager_output.as_transaction_output(key_manager).await.unwrap();

    (output, kernel, key_manager_output)
}

async fn genesis_template(
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (NewBlockTemplate, KeyManagerOutput) {
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

// #[ignore = "used to generate a new nextnet genesis block"]
/// This is a helper function to generate and print out a block that can be used as the genesis block.
/// 1. Run `cargo test --package tari_core --test mempool -- helpers::block_builders::print_new_genesis_block_nextnet
/// --exact --nocapture`
/// 1. The block and range proof will be printed
/// 1. Profit!
#[tokio::test]
async fn print_new_genesis_block_nextnet() {
    print_new_genesis_block(Network::NextNet, "Mathematical proof that something happened").await;
}

// #[ignore = "used to generate a new stagenet genesis block"]
/// This is a helper function to generate and print out a block that can be used as the genesis block.
/// 1. Run `cargo test --package tari_core --test mempool -- helpers::block_builders::print_new_genesis_block_stagenet
/// --exact --nocapture`
/// 1. The block and range proof will be printed
/// 1. Profit!
#[tokio::test]
async fn print_new_genesis_block_stagenet() {
    print_new_genesis_block(Network::StageNet, "Tokenized and connected").await;
}

// #[ignore = "used to generate a new esmeralda genesis block"]
/// This is a helper function to generate and print out a block that can be used as the genesis block.
/// 1. Run `cargo test --package tari_core --test mempool -- helpers::block_builders::print_new_genesis_block_esmeralda
/// --exact --nocapture`
/// 1. The block and range proof will be printed
/// 1. Profit!
#[tokio::test]
async fn print_new_genesis_block_esmeralda() {
    print_new_genesis_block(Network::Esmeralda, "Queues happen to other people").await;
}

// #[ignore = "used to generate a new igor genesis block"]
/// This is a helper function to generate and print out a block that can be used as the genesis block.
/// 1. Run `cargo test --package tari_core --test mempool -- helpers::block_builders::print_new_genesis_block_igor
/// --exact --nocapture`
/// 1. The block and range proof will be printed
/// 1. Profit!
#[tokio::test]
async fn print_new_genesis_block_igor() {
    print_new_genesis_block(Network::Igor, "Hello, Igor").await;
}

async fn print_new_genesis_block(network: Network, extra: &str) {
    let consensus_manager: ConsensusManager = ConsensusManagerBuilder::new(network).build();
    let mut header = BlockHeader::new(consensus_manager.consensus_constants(0).blockchain_version());
    let value = consensus_manager.emission_schedule().block_reward(0);
    let lock_height = consensus_manager.consensus_constants(0).coinbase_lock_height();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (utxo, spending_key_id, _) = create_utxo(
        value,
        &key_manager,
        &OutputFeatures::create_coinbase(lock_height, Some(extra.as_bytes().to_vec())),
        &script![Nop],
        &Covenant::default(),
        MicroTari::zero(),
    )
    .await;
    let (pk, sig) = create_random_signature_from_secret_key(
        &key_manager,
        spending_key_id,
        0.into(),
        0,
        KernelFeatures::COINBASE_KERNEL,
        TxoStage::Output,
    )
    .await;
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    let mut kernel_mmr = KernelMmr::new(Vec::new());
    kernel_mmr.push(kernel.hash().to_vec()).unwrap();

    let mut witness_mmr = WitnessMmr::new(Vec::new());
    witness_mmr.push(utxo.witness_hash().to_vec()).unwrap();
    let mut output_mmr = MutableOutputMmr::new(Vec::new(), Bitmap::create()).unwrap();
    output_mmr.push(utxo.hash().to_vec()).unwrap();
    let vn_mr = calculate_validator_node_mr(&[]);

    header.kernel_mr = FixedHash::try_from(kernel_mmr.get_merkle_root().unwrap()).unwrap();
    header.kernel_mmr_size += 1;
    header.output_mr = FixedHash::try_from(output_mmr.get_merkle_root().unwrap()).unwrap();
    header.witness_mr = FixedHash::try_from(witness_mmr.get_merkle_root().unwrap()).unwrap();
    header.output_mmr_size += 1;
    header.validator_node_mr = FixedHash::try_from(vn_mr).unwrap();

    // header.kernel_mr = kernel.hash();
    // header.kernel_mmr_size += 1;
    // header.output_mr = utxo.hash();
    // header.witness_mr = utxo.witness_hash();
    // header.output_mmr_size += 1;

    let block = header.into_builder().with_coinbase_utxo(utxo, kernel).build();

    for kernel in block.body.kernels() {
        kernel.verify_signature().unwrap();
    }
    for output in block.body.outputs() {
        output.verify_metadata_signature().unwrap();
    }
    let outputs = block.body.outputs().iter().collect::<Vec<_>>();
    batch_verify_range_proofs(&CryptoFactories::default().range_proof, &outputs).unwrap();

    // Note: This is printed in the same order as needed for 'fn get_dibbler_genesis_block_raw()'
    println!();
    println!("{} genesis block", network);
    println!();
    println!("extra: '{}'", extra);
    println!(
        "kernel excess_sig: public_nonce {} signature {}",
        block.body.kernels()[0].excess_sig.get_public_nonce().to_hex(),
        block.body.kernels()[0].excess_sig.get_signature().to_hex()
    );
    println!();
    println!(
        "Coinbase metasig: ephemeral_commitment {} ephemeral_public_key {} signature_u_a {} signature_u_x {} \
         signature_u_y {}",
        block.body.outputs()[0]
            .metadata_signature
            .ephemeral_commitment()
            .to_hex(),
        block.body.outputs()[0].metadata_signature.ephemeral_pubkey().to_hex(),
        block.body.outputs()[0].metadata_signature.u_a().to_hex(),
        block.body.outputs()[0].metadata_signature.u_x().to_hex(),
        block.body.outputs()[0].metadata_signature.u_y().to_hex(),
    );
    println!();
    println!("Genesis coinbase maturity: {}", lock_height);
    println!("UTXO commitment: {}", block.body.outputs()[0].commitment.to_hex());
    println!("UTXO range_proof: {}", block.body.outputs()[0].proof_hex_display(true));
    println!(
        "Encrypted data: {}",
        block.body.outputs()[0].encrypted_data.hex_display(true)
    );
    println!(
        "UTXO sender offset pubkey: {}",
        block.body.outputs()[0].sender_offset_public_key.to_hex()
    );
    println!();
    println!("kernel excess: {}", block.body.kernels()[0].excess.to_hex());
    println!();
    println!("header output_mr: {}", block.header.output_mr.to_hex());
    println!("header witness_mr: {}", block.header.witness_mr.to_hex());
    println!("header kernel_mr: {}", block.header.kernel_mr.to_hex());
    println!("header validator_node_mr: {}", block.header.validator_node_mr.to_hex());
    println!(
        "header total_kernel_offset: {}",
        block.header.total_kernel_offset.to_hex()
    );
    println!(
        "header total_script_offset: {}",
        block.header.total_script_offset.to_hex()
    );
}

/// Create a genesis block returning it with the spending key for the coinbase utxo
///
/// Right now this function does not use consensus rules to generate the block. The coinbase output has an arbitrary
/// value, and the maturity is zero.
pub async fn create_genesis_block(
    consensus_constants: &ConsensusConstants,
    key_manager: &TestKeyManager,
) -> (ChainBlock, KeyManagerOutput) {
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
    type BaseLayerWitnessMutableMmr = MutableMmr<WitnessMmrHasherBlake256, Vec<Hash>>;

    let NewBlockTemplate { header, mut body, .. } = template;
    // Make sure the body components are sorted. If they already are, this is a very cheap call.
    body.sort();
    let kernel_hashes: Vec<Vec<u8>> = body.kernels().iter().map(|k| k.hash().to_vec()).collect();
    let out_hashes: Vec<Vec<u8>> = body.outputs().iter().map(|out| out.hash().to_vec()).collect();
    let rp_hashes: Vec<Vec<u8>> = body.outputs().iter().map(|out| out.witness_hash().to_vec()).collect();

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
    header.witness_mr = FixedHash::try_from(
        BaseLayerWitnessMutableMmr::new(rp_hashes, Bitmap::create())
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
) -> (ChainBlock, KeyManagerOutput) {
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
) -> (ChainBlock, Vec<KeyManagerOutput>) {
    let (mut template, coinbase) = genesis_template(100_000_000.into(), consensus_constants, key_manager).await;
    let script = script!(Nop);
    let output_features = OutputFeatures::default();
    let mut outputs = Vec::new();
    outputs.push(coinbase);
    for value in values {
        let p = TestParams::new(key_manager).await;
        let key_manager_output =
            create_key_manager_output_with_data(script.clone(), output_features.clone(), &p, *value, key_manager)
                .await
                .unwrap();
        outputs.push(key_manager_output.clone());
        let output = key_manager_output.as_transaction_output(key_manager).await.unwrap();
        template.body.add_output(output);
    }
    // let outputs = values.iter().fold(vec![coinbase], |mut secrets, v| {
    //     let p = TestParams::new(key_manager).await;
    //     let unblinded_output =
    //         create_key_manager_output_with_data(script.clone(), output_features.clone(), &p, *v).await.unwrap();
    //     secrets.push(unblinded_output.clone());
    //     let output = unblinded_output.as_transaction_output(factories).unwrap();
    //     template.body.add_output(output);
    //     secrets
    // });
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
) -> (NewBlockTemplate, KeyManagerOutput) {
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
) -> Result<(ChainBlock, KeyManagerOutput), ChainStorageError> {
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
    outputs: &mut Vec<Vec<KeyManagerOutput>>,
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
    outputs: &mut Vec<Vec<KeyManagerOutput>>,
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
    outputs: &mut Vec<Vec<KeyManagerOutput>>,
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
/// This function is not able to determine the unblinded outputs of a transaction, so if you are mixing using this
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
