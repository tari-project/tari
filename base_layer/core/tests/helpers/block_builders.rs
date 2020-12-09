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

use croaring::Bitmap;
use rand::{rngs::OsRng, RngCore};
use std::{iter::repeat_with, sync::Arc};
use tari_core::{
    blocks::{Block, BlockHeader, NewBlockTemplate},
    chain_storage::{BlockAddResult, BlockchainBackend, BlockchainDatabase, ChainStorageError},
    consensus::{ConsensusConstants, ConsensusManager, ConsensusManagerBuilder, Network},
    proof_of_work::Difficulty,
    transactions::{
        helpers::{create_random_signature_from_s_key, create_signature, create_utxo, spend_utxos, TransactionSchema},
        tari_amount::MicroTari,
        transaction::{
            KernelBuilder,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionKernel,
            TransactionOutput,
            UnblindedOutput,
        },
        types::{Commitment, CryptoFactories, HashDigest, HashOutput, PublicKey},
    },
};
use tari_crypto::{
    keys::PublicKey as PublicKeyTrait,
    tari_utilities::{hash::Hashable, hex::Hex},
};
use tari_mmr::MutableMmr;

const _MAINNET: Network = Network::MainNet;
const _RIDCULLY: Network = Network::Ridcully;

pub fn create_coinbase(
    factories: &CryptoFactories,
    value: MicroTari,
    maturity_height: u64,
) -> (TransactionOutput, TransactionKernel, UnblindedOutput)
{
    let features = OutputFeatures::create_coinbase(maturity_height);
    let (mut utxo, key) = create_utxo(value, &factories, None);
    utxo.features = features.clone();
    let excess = Commitment::from_public_key(&PublicKey::from_secret_key(&key));
    let sig = create_signature(key.clone(), 0.into(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    let output = UnblindedOutput::new(value, key, Some(features));
    (utxo, kernel, output)
}

fn _genesis_template(
    factories: &CryptoFactories,
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
) -> (NewBlockTemplate, UnblindedOutput)
{
    let header = BlockHeader::new(consensus_constants.blockchain_version());
    let (utxo, kernel, output) = create_coinbase(factories, coinbase_value, consensus_constants.coinbase_lock_height());
    let block = NewBlockTemplate::from(header.into_builder().with_coinbase_utxo(utxo, kernel).build());
    (block, output)
}

// This is a helper function to generate and print out a block that can be used as the genesis block.
// #[test]
pub fn _create_act_gen_block() {
    let network = _RIDCULLY;
    let consensus_manager: ConsensusManager = ConsensusManagerBuilder::new(network).build();
    let factories = CryptoFactories::default();
    let mut header = BlockHeader::new(consensus_manager.consensus_constants(0).blockchain_version());
    let value = consensus_manager.emission_schedule().block_reward(0);
    let (mut utxo, key) = create_utxo(value, &factories, None);
    utxo.features = OutputFeatures::create_coinbase(1);
    let (pk, sig) = create_random_signature_from_s_key(key.clone(), 0.into(), 0);
    let excess = Commitment::from_public_key(&pk);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();

    let utxo_hash = utxo.hash();
    let rp = utxo.proof().hash();
    let kern = kernel.hash();
    header.kernel_mr = kern;
    header.output_mr = utxo_hash;
    header.range_proof_mr = rp;
    let block = header.into_builder().with_coinbase_utxo(utxo, kernel).build();
    println!("{}", &block);
    dbg!(&key.to_hex());
    dbg!(&block.body.outputs()[0].proof.to_hex());
    assert!(false); // this is so that the output is printed
}

/// Create a genesis block returning it with the spending key for the coinbase utxo
///
/// Right now this function does not use consensus rules to generate the block. The coinbase output has an arbitrary
/// value, and the maturity is zero.
pub fn create_genesis_block(
    factories: &CryptoFactories,
    consensus_constants: &ConsensusConstants,
) -> (Block, UnblindedOutput)
{
    create_genesis_block_with_coinbase_value(factories, consensus_constants.emission_amounts().0, consensus_constants)
}

// Calculate the MMR Merkle roots for the genesis block template and update the header.
fn _update_genesis_block_mmr_roots(template: NewBlockTemplate) -> Result<Block, ChainStorageError> {
    let NewBlockTemplate { header, mut body } = template;
    // Make sure the body components are sorted. If they already are, this is a very cheap call.
    body.sort();
    let kernel_hashes: Vec<HashOutput> = body.kernels().iter().map(|k| k.hash()).collect();
    let out_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.hash()).collect();
    let rp_hashes: Vec<HashOutput> = body.outputs().iter().map(|out| out.proof().hash()).collect();

    let mut header = BlockHeader::from(header);
    header.kernel_mr = MutableMmr::<HashDigest, _>::new(kernel_hashes, Bitmap::create())
        .unwrap()
        .get_merkle_root()?;
    header.output_mr = MutableMmr::<HashDigest, _>::new(out_hashes, Bitmap::create())
        .unwrap()
        .get_merkle_root()?;
    header.range_proof_mr = MutableMmr::<HashDigest, _>::new(rp_hashes, Bitmap::create())
        .unwrap()
        .get_merkle_root()?;
    Ok(Block { header, body })
}

/// Create a genesis block with the specified coinbase value, returning it with the spending key for the coinbase utxo.
pub fn create_genesis_block_with_coinbase_value(
    factories: &CryptoFactories,
    coinbase_value: MicroTari,
    consensus_constants: &ConsensusConstants,
) -> (Block, UnblindedOutput)
{
    let (template, output) = _genesis_template(&factories, coinbase_value, consensus_constants);
    let mut block = _update_genesis_block_mmr_roots(template).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    (block, output)
}

/// Create a Genesis block with additional utxos that are immediately available for spending. This is useful for
/// writing tests without having to add blocks just so the coinbase output can mature.
pub fn create_genesis_block_with_utxos(
    factories: &CryptoFactories,
    values: &[MicroTari],
    consensus_constants: &ConsensusConstants,
) -> (Block, Vec<UnblindedOutput>)
{
    let (mut template, coinbase) = _genesis_template(&factories, 100_000_000.into(), consensus_constants);
    let outputs = values.iter().fold(vec![coinbase], |mut secrets, v| {
        let (t, k) = create_utxo(*v, factories, None);
        template.body.add_output(t);
        secrets.push(UnblindedOutput::new(v.clone(), k, None));
        secrets
    });
    let mut block = _update_genesis_block_mmr_roots(template).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    (block, outputs)
}

/// Create a new block using the provided transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
) -> NewBlockTemplate
{
    let mut header = BlockHeader::from_previous(&prev_block.header).unwrap();
    header.version = consensus.consensus_constants(header.height).blockchain_version();
    NewBlockTemplate::from(header.into_builder().with_transactions(transactions).build())
}

/// Create a new block using the provided coinbase and transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block_with_coinbase(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    coinbase_utxo: TransactionOutput,
    coinbase_kernel: TransactionKernel,
    consensus: &ConsensusManager,
) -> NewBlockTemplate
{
    let mut header = BlockHeader::from_previous(&prev_block.header).unwrap();
    header.version = consensus.consensus_constants(header.height).blockchain_version();
    NewBlockTemplate::from(
        header
            .into_builder()
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
    )
}

/// Create a new block using the provided coinbase and transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block_with_new_coinbase(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    factories: &CryptoFactories,
) -> (NewBlockTemplate, UnblindedOutput)
{
    let height = prev_block.header.height + 1;
    let coinbase_value = consensus_manager.emission_schedule().block_reward(height);
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(
        &factories,
        coinbase_value,
        height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
    );
    let mut header = BlockHeader::from_previous(&prev_block.header).unwrap();
    header.version = consensus_manager
        .consensus_constants(header.height)
        .blockchain_version();
    let template = NewBlockTemplate::from(
        header
            .into_builder()
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
    );
    (template, coinbase_output)
}

/// Create a new block with the provided transactions. The new MMR roots are calculated, and then the new block is
/// added to the database. The newly created block is returned as the result.
pub fn append_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &Block,
    txns: Vec<Transaction>,
    consensus: &ConsensusManager,
    achieved_difficulty: Difficulty,
) -> Result<Block, ChainStorageError>
{
    let template = chain_block(prev_block, txns, consensus);
    let mut block = db.prepare_block_merkle_roots(template)?;
    block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut block.header, achieved_difficulty);
    db.add_block(Arc::new(block.clone()))?;
    Ok(block)
}

/// Create a new block with the provided transactions and add a coinbase output. The new MMR roots are calculated, and
/// then the new block is added to the database. The newly created block is returned as the result.
pub fn append_block_with_coinbase<B: BlockchainBackend>(
    factories: &CryptoFactories,
    db: &BlockchainDatabase<B>,
    prev_block: &Block,
    txns: Vec<Transaction>,
    consensus_manager: &ConsensusManager,
    achieved_difficulty: Difficulty,
) -> Result<(Block, UnblindedOutput), ChainStorageError>
{
    let height = prev_block.header.height + 1;
    let coinbase_value = consensus_manager.emission_schedule().block_reward(height);
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(
        &factories,
        coinbase_value,
        height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
    );
    let template = chain_block_with_coinbase(prev_block, txns, coinbase_utxo, coinbase_kernel, consensus_manager);
    let mut block = db.prepare_block_merkle_roots(template)?;
    block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut block.header, achieved_difficulty);
    db.add_block(Arc::new(block.clone()))?;
    Ok((block, coinbase_output))
}

/// Generate a new block using the given transaction schema and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub fn generate_new_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    blocks: &mut Vec<Block>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError>
{
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    let mut keys = Vec::new();
    for schema in schemas {
        let (tx, mut utxos, param) = spend_utxos(schema);
        txns.push(tx);
        block_utxos.append(&mut utxos);
        keys.push(param);
    }
    outputs.push(block_utxos);
    generate_block(db, blocks, txns, consensus)
}

pub fn generate_new_block_with_achieved_difficulty<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<Block>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    achieved_difficulty: Difficulty,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError>
{
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    for schema in schemas {
        let (tx, mut utxos, _) = spend_utxos(schema);
        txns.push(tx);
        block_utxos.append(&mut utxos);
    }
    outputs.push(block_utxos);
    _generate_block_with_achieved_difficulty(db, blocks, txns, achieved_difficulty, consensus)
}

/// Generate a new block using the given transaction schema and coinbase value and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub fn _generate_new_block_with_coinbase<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    factories: &CryptoFactories,
    blocks: &mut Vec<Block>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    coinbase_value: MicroTari,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError>
{
    let mut txns = Vec::new();
    let mut block_utxos = Vec::new();
    let mut keys = Vec::new();
    for schema in schemas {
        let (tx, mut utxos, param) = spend_utxos(schema);
        txns.push(tx);
        block_utxos.append(&mut utxos);
        keys.push(param);
    }
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(factories, coinbase_value, 100);
    block_utxos.push(coinbase_output);

    outputs.push(block_utxos);
    _generate_block_with_coinbase(db, blocks, txns, coinbase_utxo, coinbase_kernel, consensus)
}

pub fn find_header_with_achieved_difficulty(header: &mut BlockHeader, achieved_difficulty: Difficulty) {
    let mut num_tries = 0;
    while header.achieved_difficulty().unwrap() != achieved_difficulty {
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
pub fn generate_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    blocks: &mut Vec<Block>,
    transactions: Vec<Transaction>,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError>
{
    let prev_block = blocks.last().unwrap();
    let template = chain_block(prev_block, transactions, consensus);
    let new_block = db.prepare_block_merkle_roots(template)?;
    let result = db.add_block(new_block.clone().into());
    if let Ok(BlockAddResult::Ok) = result {
        blocks.push(new_block);
    }
    result
}

pub fn _generate_block_with_achieved_difficulty<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<Block>,
    transactions: Vec<Transaction>,
    achieved_difficulty: Difficulty,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError>
{
    let template = chain_block(&blocks.last().unwrap(), transactions, consensus);
    let mut new_block = db.prepare_block_merkle_roots(template)?;
    new_block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut new_block.header, achieved_difficulty);
    let result = db.add_block(new_block.clone().into());
    if let Ok(BlockAddResult::Ok) = result {
        blocks.push(new_block);
    }
    result
}

/// Generate a block and add it to the database using the provided transactions and coinbase. The header will be updated
/// with the correct MMR roots.
pub fn _generate_block_with_coinbase<B: BlockchainBackend>(
    db: &mut BlockchainDatabase<B>,
    blocks: &mut Vec<Block>,
    transactions: Vec<Transaction>,
    coinbase_utxo: TransactionOutput,
    coinbase_kernel: TransactionKernel,
    consensus: &ConsensusManager,
) -> Result<BlockAddResult, ChainStorageError>
{
    let template = chain_block_with_coinbase(
        &blocks.last().unwrap(),
        transactions,
        coinbase_utxo,
        coinbase_kernel,
        consensus,
    );
    let new_block = db.prepare_block_merkle_roots(template)?;
    let result = db.add_block(new_block.clone().into());
    if let Ok(BlockAddResult::Ok) = result {
        blocks.push(new_block);
    }
    result
}

pub fn construct_chained_blocks<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    block0: Block,
    consensus: &ConsensusManager,
    n: usize,
) -> Vec<Block>
{
    let mut prev_block = block0;

    repeat_with(|| {
        let block = append_block(db, &prev_block, vec![], consensus, 1.into()).unwrap();
        prev_block = block.clone();
        block
    })
    .take(n)
    .collect()
}
