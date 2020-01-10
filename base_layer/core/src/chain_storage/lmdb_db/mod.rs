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
mod lmdb_db;
mod lmdb_vec;

// Public API exports
pub use lmdb_db::{create_lmdb_database, LMDBDatabase};
pub use lmdb_vec::LMDBVec;

pub const LMDB_DB_METADATA: &str = "metadata";
pub const LMDB_DB_HEADERS: &str = "headers";
pub const LMDB_DB_BLOCK_HASHES: &str = "block_hashes";
pub const LMDB_DB_UTXOS: &str = "utxos";
pub const LMDB_DB_TXOS_HASH_TO_INDEX: &str = "txos_hash_to_index";
pub const LMDB_DB_STXOS: &str = "stxos";
pub const LMDB_DB_KERNELS: &str = "kernels";
pub const LMDB_DB_ORPHANS: &str = "orphans";
pub const LMDB_DB_UTXO_MMR_BASE_BACKEND: &str = "utxo_mmr_base_backend";
pub const LMDB_DB_UTXO_MMR_CP_BACKEND: &str = "utxo_mmr_cp_backend";
pub const LMDB_DB_KERNEL_MMR_BASE_BACKEND: &str = "kernel_mmr_base_backend";
pub const LMDB_DB_KERNEL_MMR_CP_BACKEND: &str = "kernel_mmr_cp_backend";
pub const LMDB_DB_RANGE_PROOF_MMR_BASE_BACKEND: &str = "range_proof_mmr_base_backend";
pub const LMDB_DB_RANGE_PROOF_MMR_CP_BACKEND: &str = "range_proof_mmr_cp_backend";
