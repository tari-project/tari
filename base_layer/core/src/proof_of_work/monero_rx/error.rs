//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use randomx_rs::RandomXError;
use tari_utilities::hex::HexError;

use crate::proof_of_work::DifficultyError;

/// Errors that can occur when merging Monero PoW data with Tari PoW data
#[derive(Debug, thiserror::Error)]
pub enum MergeMineError {
    #[error("Serialization error: {0}")]
    SerializeError(String),
    #[error("Error deserializing Monero data: {0}")]
    DeserializeError(String),
    #[error("Hashing of Monero data failed: {0}")]
    HashingError(String),
    #[error("RandomX error: {0}")]
    RandomXError(#[from] RandomXError),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Hex conversion error: {0}")]
    HexError(#[from] HexError),
    #[error("Monero PoW data did not contain a valid merkle root")]
    InvalidMerkleRoot,
    #[error("Invalid difficulty: {0}")]
    DifficultyError(#[from] DifficultyError),
}
