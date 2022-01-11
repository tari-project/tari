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

use std::{collections::VecDeque, io, iter::FromIterator};

use tari_common_types::types::{Commitment, PublicKey};
use tari_crypto::script::TariScript;

use crate::covenants::{
    arguments::{CovenantArg, Hash},
    decoder::{CovenantDecodeError, CovenentReadExt},
    fields::OutputField,
    filters::{
        AbsoluteHeightFilter,
        AndFilter,
        CovenantFilter,
        FieldEqFilter,
        FieldsHashedEqFilter,
        FieldsPreservedFilter,
        IdentityFilter,
        NotFilter,
        OrFilter,
        OutputHashEqFilter,
        XorFilter,
    },
    Covenant,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CovenantToken {
    Filter(CovenantFilter),
    Arg(CovenantArg),
}

impl CovenantToken {
    pub fn read_from<R: io::Read>(reader: &mut R) -> Result<Option<Self>, CovenantDecodeError> {
        let code = match reader.read_next_byte_code()? {
            Some(c) => c,
            // Nothing further to read
            None => return Ok(None),
        };
        match code {
            code if CovenantFilter::is_valid_code(code) => {
                let filter = CovenantFilter::try_from_byte_code(code)?;
                Ok(Some(CovenantToken::Filter(filter)))
            },
            code if CovenantArg::is_valid_code(code) => {
                let arg = CovenantArg::read_from(reader, code)?;
                Ok(Some(CovenantToken::Arg(arg)))
            },
            code => Err(CovenantDecodeError::UnknownByteCode { code }),
        }
    }

    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        match self {
            CovenantToken::Filter(filter) => filter.write_to(writer),
            CovenantToken::Arg(arg) => arg.write_to(writer),
        }
    }

    pub fn as_filter(&self) -> Option<&CovenantFilter> {
        match self {
            CovenantToken::Filter(filter) => Some(filter),
            CovenantToken::Arg(_) => None,
        }
    }

    pub fn as_arg(&self) -> Option<&CovenantArg> {
        match self {
            CovenantToken::Filter(_) => None,
            CovenantToken::Arg(arg) => Some(arg),
        }
    }

    //---------------------------------- Macro helper functions --------------------------------------------//

    #[allow(dead_code)]
    pub(super) fn identity() -> Self {
        CovenantToken::Filter(CovenantFilter::Identity(IdentityFilter))
    }

    #[allow(dead_code)]
    pub(super) fn and() -> Self {
        CovenantToken::Filter(CovenantFilter::And(AndFilter))
    }

    #[allow(dead_code)]
    pub(super) fn or() -> Self {
        CovenantToken::Filter(CovenantFilter::Or(OrFilter))
    }

    #[allow(dead_code)]
    pub(super) fn xor() -> Self {
        CovenantToken::Filter(CovenantFilter::Xor(XorFilter))
    }

    #[allow(dead_code)]
    pub(super) fn not() -> Self {
        CovenantToken::Filter(CovenantFilter::Not(NotFilter))
    }

    #[allow(dead_code)]
    pub(super) fn output_hash_eq() -> Self {
        CovenantToken::Filter(CovenantFilter::OutputHashEq(OutputHashEqFilter))
    }

    #[allow(dead_code)]
    pub(super) fn fields_preserved() -> Self {
        CovenantToken::Filter(CovenantFilter::FieldsPreserved(FieldsPreservedFilter))
    }

    #[allow(dead_code)]
    pub(super) fn field_eq() -> Self {
        CovenantToken::Filter(CovenantFilter::FieldEq(FieldEqFilter))
    }

    #[allow(dead_code)]
    pub(super) fn fields_hashed_eq() -> Self {
        CovenantToken::Filter(CovenantFilter::FieldsHashedEq(FieldsHashedEqFilter))
    }

    #[allow(dead_code)]
    pub(super) fn absolute_height() -> Self {
        CovenantToken::Filter(CovenantFilter::AbsoluteHeight(AbsoluteHeightFilter))
    }

    #[allow(dead_code)]
    pub(super) fn hash(hash: Hash) -> Self {
        CovenantToken::Arg(CovenantArg::Hash(hash))
    }

    #[allow(dead_code)]
    pub(super) fn public_key(public_key: PublicKey) -> Self {
        CovenantToken::Arg(CovenantArg::PublicKey(public_key))
    }

    #[allow(dead_code)]
    pub(super) fn commitment(commitment: Commitment) -> Self {
        CovenantToken::Arg(CovenantArg::Commitment(commitment))
    }

    #[allow(dead_code)]
    pub(super) fn script(script: TariScript) -> Self {
        CovenantToken::Arg(CovenantArg::TariScript(script))
    }

    #[allow(dead_code)]
    pub(super) fn covenant(covenant: Covenant) -> Self {
        CovenantToken::Arg(CovenantArg::Covenant(covenant))
    }

    #[allow(dead_code)]
    pub(super) fn uint(val: u64) -> Self {
        CovenantToken::Arg(CovenantArg::Uint(val))
    }

    #[allow(dead_code)]
    pub(super) fn field(field: OutputField) -> Self {
        CovenantToken::Arg(CovenantArg::OutputField(field))
    }

    #[allow(dead_code)]
    pub(super) fn fields(fields: Vec<OutputField>) -> Self {
        CovenantToken::Arg(CovenantArg::OutputFields(fields.into()))
    }

    #[allow(dead_code)]
    pub(super) fn bytes(bytes: Vec<u8>) -> Self {
        CovenantToken::Arg(CovenantArg::Bytes(bytes))
    }
}

#[derive(Debug, Clone, Default)]
pub struct CovenantTokenCollection {
    tokens: VecDeque<CovenantToken>,
}

impl CovenantTokenCollection {
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn next(&mut self) -> Option<CovenantToken> {
        self.tokens.pop_front()
    }
}

impl FromIterator<CovenantToken> for CovenantTokenCollection {
    fn from_iter<T: IntoIterator<Item = CovenantToken>>(iter: T) -> Self {
        Self {
            tokens: iter.into_iter().collect(),
        }
    }
}

impl From<Vec<CovenantToken>> for CovenantTokenCollection {
    fn from(tokens: Vec<CovenantToken>) -> Self {
        Self { tokens: tokens.into() }
    }
}
