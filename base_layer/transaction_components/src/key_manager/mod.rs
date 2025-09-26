// Copyright 2023 The Tari Project
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

mod wrapper;
use blake2::Blake2b;
use digest::consts::U64;
pub use wrapper::TransactionKeyManagerWrapper;
mod interface;
pub use interface::{
    KeyManagerBranch,
    KeyManagerState,
    SecretTransactionKeyManagerInterface,
    SerializedKeyString,
    TariKeyAndId,
    TariKeyId,
    TransactionKeyManagerBackend,
    TransactionKeyManagerInterface,
    TxoStage,
};

mod initializer;
pub use initializer::TransactionKeyManagerInitializer;

mod inner;
/// This is a memory database implementation of the `TransactionKeyManager` trait.
pub use inner::TransactionKeyManagerInner;
pub use inner::LEDGER_NOT_SUPPORTED;

pub mod error;
pub use error::CoreKeyManagerError;
pub use tari_common_types::key_branches::TransactionKeyManagerBranch;

use crate::consensus::DomainSeparatedConsensusHasher;

pub mod memory_key_manager;
pub use memory_key_manager::{create_memory_key_manager, MemoryKeyManager};

pub mod tari_key_manager;

pub const HASHER_LABEL_DERIVE_KEY: &str = "derive_key";
use tari_hashing::ConfidentialOutputHashDomain;
/// Hasher used in the DAN to derive masks and encrypted value keys
pub type ConfidentialOutputHasher = DomainSeparatedConsensusHasher<ConfidentialOutputHashDomain, Blake2b<U64>>;

#[derive(Debug, PartialEq)]
pub enum AddResult {
    NewEntry,
    AlreadyExists,
}

// key manager key digest used
pub type KeyDigest = Blake2b<U64>;
