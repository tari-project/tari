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

use crate::chain_storage::{MemoryDatabase, BlockchainDatabase};
use crate::types::{HashDigest, HashOutput};
use crate::test_utils::builders::{create_genesis_block, chain_block};
use crate::tari_amount::{uT, T};

/// Create a simple 5 block memory-backed database.
/// Genesis block:
///    100_000_000 -> utxo_0
/// Block 1:
///   utxo_0 -> 60_000_000 (A)
///          -> change     (F)
///          -> 100 fee/g
/// Block 2:
///   A      -> 20_000_000 (B)
///              5_000_000 (C)
///              1_000_000 (D)
///              change
///              120 fee/g
///   B     -> 15_000_000
///             change
///             75 fee/g
/// Block 3:
///  C + D  -> 6_000_000 - fee (E)
///            25 uT fee/g
///  E + F  -> 40_000_000  (G)
///            change
///            100 fee/g
/// Block 4:
///
pub fn create_blockchain_db() -> BlockchainDatabase<MemoryDatabase<HashDigest>> {
    let db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    // Genesis Block
    let (block0, utxo) = create_genesis_block();
    // Block 1
    let (tx, utxos_1, _) = spend!(vec![utxo], to: &[60*T], fee: 100*uT);
    let block1 = chain_block(&block0, vec![tx]);
    // Block 2
    let (tx1, utxos_2a, _) = spend!(vec![utxos_1[0].clone()], to: &[20*T, 5*T, 1*T], fee: 120*uT);
    let (tx2, utxos_2b, _) = spend!(vec![utxos_2a[0].clone()], to: &[15*T], fee: 75*uT);
    let block2 = chain_block(&block1, vec![tx1, tx2]);
    // Block 3
    let (tx1, utxos_3a, _) = spend!(vec![utxos_2a[1].clone(), utxos_2a[2].clone()], to: &[], fee: 75*uT);
    let (tx2, utxos_3b, _) = spend!(vec![utxos_3a[0].clone(), utxos_1[1].clone()], to: &[40*T], fee: 100*uT);
    // Block 4
    db.add_block(block0).expect("Could not create Genesis block");
    db.add_block(block1).expect("Could not save block 1");
    db.add_block(block2).expect("Could not save block 2");
    db
}