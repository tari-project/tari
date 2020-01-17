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

use tari_core::{
    blocks::{Block, BlockBuilder, BlockHeader, NewBlockTemplate},
    chain_storage::{BlockAddResult, BlockchainBackend, BlockchainDatabase, ChainStorageError, MemoryDatabase},
    consensus::emission::EmissionSchedule,
};
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_transactions::{
    helpers::{
        create_random_signature,
        create_random_signature_from_s_key,
        create_utxo,
        spend_utxos,
        TransactionSchema,
    },
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
    types::{Commitment, CryptoFactories, HashDigest, PublicKey},
};
use tari_utilities::{hash::Hashable, hex::Hex};

fn create_coinbase(
    factories: &CryptoFactories,
    value: MicroTari,
) -> (TransactionOutput, TransactionKernel, UnblindedOutput)
{
    let features = OutputFeatures::create_coinbase(100);
    let (mut utxo, key) = create_utxo(value, &factories);
    utxo.features = features.clone();
    let excess = Commitment::from_public_key(&PublicKey::from_secret_key(&key));
    let (_pk, sig) = create_random_signature(0.into(), 0);
    let kernel = KernelBuilder::new()
        .with_signature(&sig)
        .with_excess(&excess)
        .with_features(KernelFeatures::COINBASE_KERNEL)
        .build()
        .unwrap();
    let output = UnblindedOutput::new(value, key, Some(features));
    (utxo, kernel, output)
}

fn genesis_template(factories: &CryptoFactories, coinbase_value: MicroTari) -> (NewBlockTemplate, UnblindedOutput) {
    let header = BlockHeader::new(0);
    let (utxo, kernel, output) = create_coinbase(factories, coinbase_value);
    let block = NewBlockTemplate::from(
        BlockBuilder::new()
            .with_header(header)
            .with_coinbase_utxo(utxo, kernel)
            .build(),
    );
    (block, output)
}

// This is a helper function to generate and print out a block that can be used as the genesis block.
pub fn create_act_gen_block() {
    let factories = CryptoFactories::default();
    let mut header = BlockHeader::new(0);
    let emission_schedule = EmissionSchedule::new(10_000_000.into(), 0.999, 100.into());
    let value = emission_schedule.supply_at_block(0);
    let (mut utxo, key) = create_utxo(value, &factories);
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
    let block = BlockBuilder::new()
        .with_header(header)
        .with_coinbase_utxo(utxo, kernel)
        .build();
    println!("{}", &block);
    dbg!(&key.to_hex());
    dbg!(&block.body.outputs()[0].proof.to_hex());
    assert!(false); // this is so that the output is printed
}

/// Create a genesis block returning it with the spending key for the coinbase utxo
///
/// Right now this function does not use consensus rules to generate the block. The coinbase output has an arbitrary
/// value, and the maturity is zero.
pub fn create_genesis_block<B>(db: &BlockchainDatabase<B>, factories: &CryptoFactories) -> (Block, UnblindedOutput)
where B: BlockchainBackend {
    create_genesis_block_with_coinbase_value(db, factories, 100_000_000.into())
}

/// Create a genesis block with the specified coinbase value, returning it with the spending key for the coinbase utxo.
pub fn create_genesis_block_with_coinbase_value<B>(
    db: &BlockchainDatabase<B>,
    factories: &CryptoFactories,
    coinbase_value: MicroTari,
) -> (Block, UnblindedOutput)
where
    B: BlockchainBackend,
{
    let (block, output) = genesis_template(&factories, coinbase_value);
    let block = db
        .calculate_mmr_roots(block)
        .expect("Could not generate genesis block MMRs");
    (block, output)
}

/// Create a Genesis block with additional utxos that are immediately available for spending. This is useful for
/// writing tests without having to add blocks just so the coinbase output can mature.
pub fn create_genesis_block_with_utxos<B>(
    db: &BlockchainDatabase<B>,
    factories: &CryptoFactories,
    values: &[MicroTari],
) -> (Block, Vec<UnblindedOutput>)
where
    B: BlockchainBackend,
{
    let (mut template, coinbase) = genesis_template(&factories, 100_000_000.into());
    let outputs = values.iter().fold(vec![coinbase], |mut secrets, v| {
        let (t, k) = create_utxo(*v, factories);
        template.body.add_output(t);
        secrets.push(UnblindedOutput::new(v.clone(), k, None));
        secrets
    });
    let block = db
        .calculate_mmr_roots(template)
        .expect("Could not generate genesis block MMRs");
    (block, outputs)
}

/// Create a new block using the provided transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block(prev_block: &Block, transactions: Vec<Transaction>) -> NewBlockTemplate {
    let header = BlockHeader::from_previous(&prev_block.header);
    NewBlockTemplate::from(
        BlockBuilder::new()
            .with_header(header)
            .with_transactions(transactions)
            .build(),
    )
}

/// Create a new block using the provided coinbase and transactions that adds to the blockchain given in `prev_block`.
pub fn chain_block_with_coinbase(
    prev_block: &Block,
    transactions: Vec<Transaction>,
    coinbase_utxo: TransactionOutput,
    coinbase_kernel: TransactionKernel,
) -> NewBlockTemplate
{
    let header = BlockHeader::from_previous(&prev_block.header);
    NewBlockTemplate::from(
        BlockBuilder::new()
            .with_header(header)
            .with_transactions(transactions)
            .with_coinbase_utxo(coinbase_utxo, coinbase_kernel)
            .build(),
    )
}

/// Create a new block with the provided transactions. The new MMR roots are calculated, and then the new block is
/// added to the database. The newly created block is returned as the result.
pub fn append_block<B: BlockchainBackend>(
    db: &BlockchainDatabase<B>,
    prev_block: &Block,
    txns: Vec<Transaction>,
) -> Result<Block, ChainStorageError>
{
    let template = chain_block(prev_block, txns);
    let block = db.calculate_mmr_roots(template)?;
    db.add_block(block.clone())?;
    Ok(block)
}

/// Generate a new block using the given transaction schema and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub fn generate_new_block(
    db: &mut BlockchainDatabase<MemoryDatabase<HashDigest>>,
    blocks: &mut Vec<Block>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
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
    generate_block(db, blocks, txns)
}

/// Generate a new block using the given transaction schema and coinbase value and add it to the provided database.
/// The blocks and UTXO vectors are also updated with the info from the new block.
pub fn generate_new_block_with_coinbase(
    db: &mut BlockchainDatabase<MemoryDatabase<HashDigest>>,
    factories: &CryptoFactories,
    blocks: &mut Vec<Block>,
    outputs: &mut Vec<Vec<UnblindedOutput>>,
    schemas: Vec<TransactionSchema>,
    coinbase_value: MicroTari,
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
    let (coinbase_utxo, coinbase_kernel, coinbase_output) = create_coinbase(factories, coinbase_value);
    block_utxos.push(coinbase_output);

    outputs.push(block_utxos);
    generate_block_with_coinbase(db, blocks, txns, coinbase_utxo, coinbase_kernel)
}

/// Generate a block and add it to the database using the transactions provided. The header will be updated with the
/// correct MMR roots.
/// This function is not able to determine the unblinded outputs of a transaction, so if you are mixing using this
/// with [generate_new_block], you must update the unblinded UTXO vector  yourself.
pub fn generate_block(
    db: &mut BlockchainDatabase<MemoryDatabase<HashDigest>>,
    blocks: &mut Vec<Block>,
    transactions: Vec<Transaction>,
) -> Result<BlockAddResult, ChainStorageError>
{
    let template = chain_block(&blocks.last().unwrap(), transactions);
    let new_block = db.calculate_mmr_roots(template)?;
    let result = db.add_block(new_block.clone());
    if let Ok(BlockAddResult::Ok) = result {
        blocks.push(new_block);
    }
    result
}

/// Generate a block and add it to the database using the provided transactions and coinbase. The header will be updated
/// with the correct MMR roots.
pub fn generate_block_with_coinbase(
    db: &mut BlockchainDatabase<MemoryDatabase<HashDigest>>,
    blocks: &mut Vec<Block>,
    transactions: Vec<Transaction>,
    coinbase_utxo: TransactionOutput,
    coinbase_kernel: TransactionKernel,
) -> Result<BlockAddResult, ChainStorageError>
{
    let template = chain_block_with_coinbase(&blocks.last().unwrap(), transactions, coinbase_utxo, coinbase_kernel);
    let new_block = db.calculate_mmr_roots(template)?;
    let result = db.add_block(new_block.clone());
    if let Ok(BlockAddResult::Ok) = result {
        blocks.push(new_block);
    }
    result
}
