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

use tari_common_types::MaxSizeVecError;

#[derive(Debug, thiserror::Error)]
pub enum CovenantError {
    #[error("Reached the end of tokens but another token was expected")]
    UnexpectedEndOfTokens,
    #[error("Expected an argument but got a filter")]
    ExpectedArgButGotFilter,
    #[error("Expected a filter but got an argument")]
    ExpectedFilterButGotArg,
    #[error("Encountered an unexpected argument. Expected {expected} but got {got}")]
    UnexpectedArgument { expected: &'static str, got: String },
    #[error("Covenant did not match any outputs")]
    NoMatchingOutputs,
    #[error("Covenant failed: unused tokens remain after execution")]
    RemainingTokens,
    #[error("Invalid argument for filter {filter}: {details}")]
    InvalidArgument { filter: &'static str, details: String },
    #[error("Max sized vector error: {0}")]
    MaxSizeVecError(#[from] MaxSizeVecError),
}
