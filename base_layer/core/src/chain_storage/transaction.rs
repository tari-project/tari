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
    blocks::{block::Block, blockheader::BlockHeader},
    transaction::{TransactionInput, TransactionKernel, TransactionOutput},
    types::HashOutput,
};
use std::fmt::{Display, Error, Formatter};
use tari_utilities::{hex::to_hex, Hashable};

#[derive(Debug)]
pub struct DbTransaction {
    pub operations: Vec<WriteOperation>,
}

impl Default for DbTransaction {
    fn default() -> Self {
        DbTransaction {
            operations: Vec::with_capacity(128),
        }
    }
}

impl DbTransaction {
    /// Creates a new Database transaction. To commit the transactions call [BlockchainDatabase::execute] with the
    /// transaction as a parameter.
    pub fn new() -> Self {
        DbTransaction::default()
    }

    /// A general insert request. There are convenience functions for specific insert queries.
    pub fn insert(&mut self, insert: DbKeyValuePair) {
        self.operations.push(WriteOperation::Insert(insert));
    }

    /// A general insert request. There are convenience functions for specific delete queries.
    pub fn delete(&mut self, delete: DbKey) {
        self.operations.push(WriteOperation::Delete(delete));
    }

    pub fn insert_kernel(&mut self, kernel: TransactionKernel) {
        let hash = kernel.hash();
        self.insert(DbKeyValuePair::TransactionKernel(hash, Box::new(kernel)));
    }

    pub fn insert_header(&mut self, header: BlockHeader) {
        let height = header.height;
        self.insert(DbKeyValuePair::BlockHeader(height, Box::new(header)));
    }

    pub fn insert_utxo(&mut self, utxo: TransactionOutput) {
        let hash = utxo.hash();
        self.insert(DbKeyValuePair::UnspentOutput(hash, Box::new(utxo)));
    }

    pub fn insert_orphan(&mut self, orphan: Block) {
        let hash = orphan.hash();
        self.insert(DbKeyValuePair::OrphanBlock(hash, Box::new(orphan)));
    }

    /// Moves a UTXO. If the UTXO is not in the UTXO set, the transaction will fail with an `UnspendableOutput` error.
    pub fn move_utxo(&mut self, utxo_hash: HashOutput) {
        self.operations
            .push(WriteOperation::Move(DbKey::UnspentOutput(utxo_hash)));
    }

    /// Moves the given set of transaction inputs from the UTXO set to the STXO set. All the inputs *must* currently
    /// exist in the UTXO set, or the method will error with `ChainStorageError::UnspendableOutput`
    pub fn spend_inputs(&mut self, inputs: &[TransactionInput]) {
        for input in inputs {
            let input_hash = input.hash();
            self.move_utxo(input_hash);
        }
    }
}

#[derive(Debug)]
pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Delete(DbKey),
    Move(DbKey),
}

#[derive(Debug)]
pub enum DbKeyValuePair {
    Metadata(MetadataKey, MetadataValue),
    BlockHeader(u64, Box<BlockHeader>),
    UnspentOutput(HashOutput, Box<TransactionOutput>),
    SpentOutput(HashOutput, Box<TransactionOutput>),
    TransactionKernel(HashOutput, Box<TransactionKernel>),
    OrphanBlock(HashOutput, Box<Block>),
}

#[derive(Debug)]
pub enum MetadataKey {
    ChainHeight,
    AccumulatedWork,
}

#[derive(Debug)]
pub enum MetadataValue {
    ChainHeight(u64),
    AccumulatedWork(u64),
}

#[derive(Debug)]
pub enum DbKey {
    Metadata(MetadataKey),
    BlockHeader(u64),
    UnspentOutput(HashOutput),
    SpentOutput(HashOutput),
    TransactionKernel(HashOutput),
    OrphanBlock(HashOutput),
}

#[derive(Debug)]
pub enum DbValue {
    Metadata(MetadataValue),
    BlockHeader(Box<BlockHeader>),
    UnspentOutput(Box<TransactionOutput>),
    SpentOutput(Box<TransactionOutput>),
    TransactionKernel(Box<TransactionKernel>),
    OrphanBlock(Box<Block>),
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Metadata(MetadataValue::ChainHeight(_)) => f.write_str("Current chain height"),
            DbValue::Metadata(MetadataValue::AccumulatedWork(_)) => f.write_str("Total accumulated work"),
            DbValue::BlockHeader(_) => f.write_str("Block header"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent output"),
            DbValue::SpentOutput(_) => f.write_str("Spent output"),
            DbValue::TransactionKernel(_) => f.write_str("Transaction kernel"),
            DbValue::OrphanBlock(_) => f.write_str("Orphan block"),
        }
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::Metadata(MetadataKey::ChainHeight) => f.write_str("Current chain height"),
            DbKey::Metadata(MetadataKey::AccumulatedWork) => f.write_str("Total accumulated work"),
            DbKey::BlockHeader(v) => f.write_str(&format!("Block header (#{})", v)),
            DbKey::UnspentOutput(v) => f.write_str(&format!("Unspent output ({})", to_hex(v))),
            DbKey::SpentOutput(v) => f.write_str(&format!("Spent output ({})", to_hex(v))),
            DbKey::TransactionKernel(v) => f.write_str(&format!("Transaction kernel ({})", to_hex(v))),
            DbKey::OrphanBlock(v) => f.write_str(&format!("Orphan block hash ({})", to_hex(v))),
        }
    }
}
