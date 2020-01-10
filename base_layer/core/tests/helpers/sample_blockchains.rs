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

use crate::helpers::block_builders::{create_genesis_block, generate_new_block};
use tari_core::{
    blocks::Block,
    chain_storage::{BlockchainDatabase, MemoryDatabase, Validators},
    validation::mocks::MockValidator,
};
use tari_transactions::{
    tari_amount::{uT, T},
    transaction::UnblindedOutput,
    txn_schema,
    types::{CryptoFactories, HashDigest},
};

/// Create a simple 6 block memory-backed database.
/// Genesis block:
///    100_000_000 -> utxo_0 (0.0)
/// Block 1:
///   0.0    -> 60_000_000 (1.0)
///          -> change     (1.1)
///          -> 100 fee/g
/// Block 2:
///   (1.0)  -> 20_000_000 (2.0)
///              5_000_000 (2.1)
///              1_000_000 (2.2)
///              change    (2.3)
///              120 fee/g
///   (1.1)  -> 15_000_000  (2.4)
///             change
///             75 fee/g
/// Block 3:
///  (2.1) + (2.2)  -> 6_000_000 - fee  (3.0)
///                    25 uT fee/g
///  (2.4) + (2.3)  -> 40_000_000       (3.1)
///                    change           (3.2)
///                    100 fee/g
/// Block 4:
///  (2.0) -> 1_000_000 (4.0)
///        -> 2_000_000 (4.1)
///        -> 3_000_000 (4.2)
///        -> 4_000_000 (4.3)
///        -> change    (4.4)
/// Block 5:
///  (4.3 + 3.1)-> 20_000_000 (5.0)
///             -> 21_000_000 (5.1)
///             -> change     (5.2)
///  (4.1)      -> 500_000    (5.3)
///             -> 1_300_00   (5.4)
///             -> change     (5.5)
/// (3.2)       -> 500_000    (5.6)
///             -> change     (5.7)
pub fn create_blockchain_db_no_cut_through() -> (
    BlockchainDatabase<MemoryDatabase<HashDigest>>,
    Vec<Block>,
    Vec<Vec<UnblindedOutput>>,
) {
    let (mut db, mut blocks, mut outputs) = create_new_blockchain();
    // Block 1
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![60*T], fee: 100*uT)];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 2
    let txs = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![20*T, 5*T, 1*T], fee: 120*uT),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![15*T], fee: 75*uT),
    ];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 3
    let txs = vec![
        txn_schema!(from: vec![outputs[2][1].clone(), outputs[2][2].clone()], to: vec![]),
        txn_schema!(from: vec![outputs[2][4].clone(), outputs[2][3].clone()], to: vec![40*T], fee: 100*uT),
    ];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 4
    let txs = vec![txn_schema!(
        from: vec![outputs[2][0].clone()],
        to: vec![1 * T, 2 * T, 3 * T, 4 * T]
    )];
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, txs).is_ok());
    // Block 5
    let txs = vec![
        txn_schema!(
            from: vec![outputs[4][3].clone(), outputs[3][1].clone()],
            to: vec![20 * T, 21 * T]
        ),
        txn_schema!(
            from: vec![outputs[4][1].clone()],
            to: vec![500_000 * uT, 1_300_000 * uT]
        ),
        txn_schema!(from: vec![outputs[3][2].clone()], to: vec![500_000 * uT]),
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
    let factories = CryptoFactories::default();
    // We may need move this to the parameters to provide more fine-grained validator control
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(db).unwrap();
    db.add_validators(validators);
    let mut outputs = Vec::new();
    let mut blocks = Vec::new();
    // Genesis Block
    let (block0, utxo) = create_genesis_block(&db, &factories);
    db.add_block(block0.clone()).unwrap();
    blocks.push(block0);
    outputs.push(vec![utxo]);
    (db, blocks, outputs)
}
