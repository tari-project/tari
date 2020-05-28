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

//! This module is responsible for handling logic responsible for storing the blockchain state.
//!
//! It is structured in such a way that clients (e.g. base nodes) can configure the various components of the state
//! (kernels, utxos, etc) in whichever way they like. It's possible to have the UTXO set in memory, and the kernels
//! backed by LMDB, while the merkle trees are stored in flat files for example.

mod blockchain_database;
mod consts;
mod db_transaction;
mod error;
mod historical_block;
mod lmdb_db;
mod memory_db;
mod metadata;

// public modules
pub mod async_db;

// Public API exports
pub use blockchain_database::{
    calculate_mmr_roots,
    fetch_header,
    fetch_headers,
    fetch_target_difficulties,
    is_stxo,
    is_utxo,
    BlockAddResult,
    BlockchainBackend,
    BlockchainDatabase,
    BlockchainDatabaseConfig,
    MutableMmrState,
    Validators,
};
pub use db_transaction::{
    DbKey,
    DbKeyValuePair,
    DbTransaction,
    DbValue,
    MetadataKey,
    MetadataValue,
    MmrTree,
    WriteOperation,
};
pub use error::ChainStorageError;
pub use historical_block::HistoricalBlock;
pub use lmdb_db::{
    create_lmdb_database,
    LMDBDatabase,
    LMDB_DB_BLOCK_HASHES,
    LMDB_DB_HEADERS,
    LMDB_DB_KERNELS,
    LMDB_DB_KERNEL_MMR_CP_BACKEND,
    LMDB_DB_METADATA,
    LMDB_DB_ORPHANS,
    LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND,
    LMDB_DB_STXOS,
    LMDB_DB_UTXOS,
    LMDB_DB_UTXO_MMR_CP_BACKEND,
};
pub use memory_db::MemoryDatabase;
pub use metadata::ChainMetadata;
