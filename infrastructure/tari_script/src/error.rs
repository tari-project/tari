// Copyright 2020. The Tari Project
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::num::TryFromIntError;

use serde::{Deserialize, Serialize};
use tari_utilities::ByteArrayError;
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptError {
    #[error("The script failed with an explicit Return")]
    Return,
    #[error("The stack cannot exceed MAX_STACK_SIZE items")]
    StackOverflow,
    #[error("The script completed execution with a stack size other than one")]
    NonUnitLengthStack,
    #[error("Tried to pop an element off an empty stack")]
    StackUnderflow,
    #[error("An operand was applied to incompatible types")]
    IncompatibleTypes,
    #[error("A script opcode resulted in a value that exceeded the maximum or minimum value")]
    ValueExceedsBounds,
    #[error("The script encountered an invalid opcode")]
    InvalidOpcode,
    #[error("The script is missing closing opcodes (Else or EndIf)")]
    MissingOpcode,
    #[error("The script contained an invalid signature")]
    InvalidSignature,
    #[error("The serialised stack contained invalid input")]
    InvalidInput,
    #[error("The script contained invalid data")]
    InvalidData,
    #[error("A verification opcode failed, aborting the script immediately")]
    VerifyFailed,
    #[error("as_hash requires a Digest function that returns at least 32 bytes")]
    InvalidDigest,
    #[error("A compare opcode failed, aborting the script immediately")]
    CompareFailed,
}

impl From<TryFromIntError> for ScriptError {
    fn from(_err: TryFromIntError) -> ScriptError {
        ScriptError::ValueExceedsBounds
    }
}

impl From<ByteArrayError> for ScriptError {
    fn from(_err: ByteArrayError) -> ScriptError {
        ScriptError::InvalidData
    }
}
