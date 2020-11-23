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

mod lmdb;
#[allow(clippy::module_inception)]
mod lmdb_db;

use crate::transactions::{
    transaction::{TransactionInput, TransactionKernel, TransactionOutput},
    types::HashOutput,
};
pub use lmdb_db::{create_lmdb_database, create_recovery_lmdb_database, LMDBDatabase};
use serde::{Deserialize, Serialize};

pub const LMDB_DB_METADATA: &str = "metadata";
pub const LMDB_DB_HEADERS: &str = "headers";
pub const LMDB_DB_HEADER_ACCUMULATED_DATA: &str = "header_accumulated_data";
pub const LMDB_DB_BLOCK_ACCUMULATED_DATA: &str = "mmr_peak_data";
pub const LMDB_DB_BLOCK_HASHES: &str = "block_hashes";
pub const LMDB_DB_ORPHAN_PREV_HASH_INDEX: &str = "orphan_prev_hash_to_index";
pub const LMDB_DB_UTXOS: &str = "utxos";
pub const LMDB_DB_INPUTS: &str = "inputs";
pub const LMDB_DB_TXOS_HASH_TO_INDEX: &str = "txos_hash_to_index";
pub const LMDB_DB_KERNELS: &str = "kernels";
pub const LMDB_DB_ORPHANS: &str = "orphans";
pub const LMDB_DB_ORPHAN_CHAIN_TIPS: &str = "orphan_chain_tips";
pub const LMDB_DB_ORPHAN_PARENT_MAP_INDEX: &str = "orphan_parent_map_index";

#[derive(Serialize, Deserialize)]
pub(crate) struct TransactionOutputRowData {
    pub output: TransactionOutput,
    pub header_hash: HashOutput,
    pub mmr_position: u32,
    pub hash: HashOutput,
    pub range_proof_hash: HashOutput,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct TransactionInputRowData {
    pub input: TransactionInput,
    pub header_hash: HashOutput,
    pub mmr_position: u32,
    pub hash: HashOutput,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct TransactionKernelRowData {
    pub kernel: TransactionKernel,
    pub header_hash: HashOutput,
    pub mmr_position: u32,
    pub hash: HashOutput,
}
