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

pub use lmdb_db::{
    create_lmdb_database,
    create_lmdb_database_with_stats_channel,
    create_readonly_lmdb_environment,
    create_recovery_lmdb_database,
    get_all_database_names,
    AccumulatedDataRebuildStatus,
    BlockchainCheckRequest,
    BlockchainCheckStatus,
    CheckFailure,
    LMDBDatabase,
    PayrefRebuildStatus,
    BREATHING_TIME_MS_MAX,
    BREATHING_TIME_MS_MIN,
};
pub use row_data::transaction_input::{TransactionInputRowData, TransactionInputRowDataRef};
pub use row_data::transaction_kernel::TransactionKernelRowData;
pub use row_data::transaction_output::TransactionOutputRowData;
pub use stats_collector::DatabaseStats;
use tari_crypto::hash_domain;

mod composite_key;
pub(crate) mod cursors;
pub(crate) mod helpers;
mod lmdb;
#[allow(clippy::module_inception)]
mod lmdb_db;
pub mod lmdb_tree_reader;
pub(crate) mod lmdb_tree_writer;
pub mod row_data;
mod stats_collector;
mod validator_node_store;

hash_domain!(CoreChainStorageHashDomain, "com.tari.base_layer.core.lmdb_db", 1);
