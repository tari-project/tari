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
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

// this file is used for all blockchain error types
use crate::blocks::block::BlockValidationError;
use derive_error::Error;
use merklemountainrange::{error::MerkleMountainRangeError, merkle_storage::MerkleStorageError};
use tari_storage::keyvalue_store::*;

/// The ChainError is used to present all generic chain error of the actual blockchain
#[derive(Debug, Error)]
pub enum ChainError {
    // Could not initialise state
    InitStateError(DatastoreError),
    // Some kind of processing error in the state
    StateProcessingError(StateError),
}

/// The chainstate is used to present all generic chain error of the actual blockchain state
#[derive(Debug, Error)]
pub enum StateError {
    // could not create a database
    StoreError(DatastoreError),
    // MerklestorageError
    StorageError(MerkleStorageError),
    // Unkown commitment spent
    SpentUnknownCommitment(MerkleMountainRangeError),
    // provided mmr states in headers mismatch
    HeaderStateMismatch,
    // block is not correctly constructed
    InvalidBlock(BlockValidationError),
    // block is orphaned
    OrphanBlock,
    // Duplicate block
    DuplicateBlock,
}
