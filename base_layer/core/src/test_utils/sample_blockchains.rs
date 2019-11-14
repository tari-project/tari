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
//

use crate::{
    blocks::Block,
    chain_storage::{BlockAddResult, BlockchainDatabase, ChainStorageError, MemoryDatabase},
    test_utils::builders::{
        add_block_and_update_header,
        chain_block,
        create_genesis_block,
        spend_utxos,
        TransactionSchema,
    },
};
use tari_transactions::{
    tari_amount::{mT, uT},
    transaction::{Transaction, UnblindedOutput},
    types::HashDigest,
};

/// Create a simple 6 block memory-backed database.
/// Genesis block:
///    10_000_100  -> utxo_0 (0.0)
/// Block 1:
///   0.0    -> 60_000 (1.0)
///          -> change     (1.1)
///          -> 100 fee/g
/// Block 2:
///   (1.0)  -> 20_000 (2.0)
///              5_000 (2.1)
///              1_000 (2.2)
///              change    (2.3)
///              120 fee/g
///   (1.1)  -> 15_000  (2.4)
///             change
///             75 fee/g
/// Block 3:
///  (2.1) + (2.2)  -> 6_000 - fee  (3.0)
///                    25 uT fee/g
///  (2.4) + (2.3)  -> 40_000       (3.1)
///                    change           (3.2)
///                    100 fee/g
/// Block 4:
///  (2.0) -> 1_000 (4.0)
///        -> 2_000 (4.1)
///        -> 3_000 (4.2)
///        -> 4_000_000 (4.3)
///        -> change    (4.4)
/// Block 5:
///  (4.3 + 3.1)-> 20_000 (5.0)
///             -> 21_000 (5.1)
///             -> change     (5.2)
///  (4.1)      -> 500    (5.3)
///             -> 1_30  (5.4)
///             -> change     (5.5)
/// (3.2)       -> 500    (5.6)
///             -> change     (5.7)
pub fn create_blockchain_db_no_cut_through() -> (
    BlockchainDatabase<MemoryDatabase<HashDigest>>,
    Vec<Block>,
    Vec<Vec<UnblindedOutput>>,
) {
    let (mut db, mut blocks, mut outputs) = create_new_blockchain();
    // Block 1
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![60*mT], fee: 100*uT)];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 2
    let txs = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![20*mT, 5*mT, 1*mT], fee: 120*uT),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![15*mT], fee: 75*uT),
    ];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 3
    let txs = vec![
        txn_schema!(from: vec![outputs[2][1].clone(), outputs[2][2].clone()], to: vec![]),
        txn_schema!(from: vec![outputs[2][4].clone(), outputs[2][3].clone()], to: vec![40*mT], fee: 100*uT),
    ];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 4
    let txs = vec![txn_schema!(
        from: vec![outputs[2][0].clone()],
        to: vec![1 * mT, 2 * mT, 3 * mT, 4 * mT]
    )];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 5
    let txs = vec![
        txn_schema!(
            from: vec![outputs[4][3].clone(), outputs[3][1].clone()],
            to: vec![20 * mT, 21 * mT]
        ),
        txn_schema!(from: vec![outputs[4][1].clone()], to: vec![500 * uT, 1_30 * uT]),
        txn_schema!(from: vec![outputs[3][2].clone()], to: vec![500 * uT]),
    ];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    (db, blocks, outputs)
}

/// Create a new blockchain database containing only the Genesis block
pub fn create_new_blockchain() -> (
    BlockchainDatabase<MemoryDatabase<HashDigest>>,
    Vec<Block>,
    Vec<Vec<UnblindedOutput>>,
) {
    let db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let mut outputs = Vec::new();
    let mut blocks = Vec::new();
    // Genesis Block
    let (mut block0, utxo) = create_genesis_block();
    block0 = add_block_and_update_header(&db, block0);
    blocks.push(block0);
    outputs.push(vec![utxo]);
    (db, blocks, outputs)
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
    let mut new_block = chain_block(&blocks.last().unwrap(), transactions);
    let result = db.add_new_block(new_block.clone());
    if let Ok(BlockAddResult::Ok(h)) = result.clone() {
        new_block.header = h;
    }
    blocks.push(new_block);
    result
}
