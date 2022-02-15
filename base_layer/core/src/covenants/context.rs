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

use crate::{
    covenants::{
        arguments::CovenantArg,
        error::CovenantError,
        filters::CovenantFilter,
        token::{CovenantToken, CovenantTokenCollection},
    },
    transactions::transaction_components::TransactionInput,
};

pub struct CovenantContext<'a> {
    input: &'a TransactionInput,
    tokens: CovenantTokenCollection,
    block_height: u64,
}

impl<'a> CovenantContext<'a> {
    pub fn new(tokens: CovenantTokenCollection, input: &'a TransactionInput, block_height: u64) -> Self {
        Self {
            input,
            tokens,
            block_height,
        }
    }

    pub fn has_more_tokens(&self) -> bool {
        !self.tokens.is_empty()
    }

    pub fn next_arg(&mut self) -> Result<CovenantArg, CovenantError> {
        match self.tokens.next().ok_or(CovenantError::UnexpectedEndOfTokens)? {
            CovenantToken::Arg(arg) => Ok(arg),
            CovenantToken::Filter(_) => Err(CovenantError::ExpectedArgButGotFilter),
        }
    }

    // Only happens to be used in tests for now
    #[cfg(test)]
    pub fn next_filter(&mut self) -> Option<CovenantFilter> {
        match self.tokens.next()? {
            CovenantToken::Filter(filter) => Some(filter),
            CovenantToken::Arg(_) => None,
        }
    }

    pub fn require_next_filter(&mut self) -> Result<CovenantFilter, CovenantError> {
        match self.tokens.next().ok_or(CovenantError::UnexpectedEndOfTokens)? {
            CovenantToken::Filter(filter) => Ok(filter),
            CovenantToken::Arg(_) => Err(CovenantError::ExpectedFilterButGotArg),
        }
    }

    pub fn block_height(&self) -> u64 {
        self.block_height
    }

    pub fn input(&self) -> &TransactionInput {
        self.input
    }
}
