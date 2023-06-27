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

use tari_common_types::types::{Commitment, FixedHash, PublicKey};
use tari_script::TariScript;

use crate::{
    covenants::{
        arguments::CovenantArg,
        decoder::{CovenantDecodeError, CovenantReadExt},
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
    },
    transactions::transaction_components::OutputType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Covenant token. Either a filter covenant or a an argument covenant.
pub enum CovenantToken {
    Filter(CovenantFilter),
    Arg(Box<CovenantArg>),
}

impl CovenantToken {
    pub fn read_from(reader: &mut &[u8]) -> Result<Option<Self>, CovenantDecodeError> {
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
                Ok(Some(CovenantToken::Arg(Box::new(arg))))
            },
            code => Err(CovenantDecodeError::UnknownByteCode { code }),
        }
    }

    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
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
            CovenantToken::Arg(arg) => Some(&**arg),
        }
    }

    //---------------------------------- Macro helper functions --------------------------------------------//

    #[allow(dead_code)]
    pub fn identity() -> Self {
        CovenantFilter::Identity(IdentityFilter).into()
    }

    #[allow(dead_code)]
    pub fn and() -> Self {
        CovenantFilter::And(AndFilter).into()
    }

    #[allow(dead_code)]
    pub fn or() -> Self {
        CovenantFilter::Or(OrFilter).into()
    }

    #[allow(dead_code)]
    pub fn xor() -> Self {
        CovenantFilter::Xor(XorFilter).into()
    }

    #[allow(dead_code)]
    pub fn not() -> Self {
        CovenantFilter::Not(NotFilter).into()
    }

    #[allow(dead_code)]
    pub fn output_hash_eq() -> Self {
        CovenantFilter::OutputHashEq(OutputHashEqFilter).into()
    }

    #[allow(dead_code)]
    pub fn fields_preserved() -> Self {
        CovenantFilter::FieldsPreserved(FieldsPreservedFilter).into()
    }

    #[allow(dead_code)]
    pub fn field_eq() -> Self {
        CovenantFilter::FieldEq(FieldEqFilter).into()
    }

    #[allow(dead_code)]
    pub fn fields_hashed_eq() -> Self {
        CovenantFilter::FieldsHashedEq(FieldsHashedEqFilter).into()
    }

    #[allow(dead_code)]
    pub fn absolute_height() -> Self {
        CovenantFilter::AbsoluteHeight(AbsoluteHeightFilter).into()
    }

    #[allow(dead_code)]
    pub fn hash(hash: FixedHash) -> Self {
        CovenantArg::Hash(hash).into()
    }

    #[allow(dead_code)]
    pub fn public_key(public_key: PublicKey) -> Self {
        CovenantArg::PublicKey(public_key).into()
    }

    #[allow(dead_code)]
    pub fn commitment(commitment: Commitment) -> Self {
        CovenantArg::Commitment(commitment).into()
    }

    #[allow(dead_code)]
    pub fn script(script: TariScript) -> Self {
        CovenantArg::TariScript(script).into()
    }

    #[allow(dead_code)]
    pub fn covenant(covenant: Covenant) -> Self {
        CovenantArg::Covenant(covenant).into()
    }

    #[allow(dead_code)]
    pub fn uint(val: u64) -> Self {
        CovenantArg::Uint(val).into()
    }

    #[allow(dead_code)]
    pub fn output_type(output_type: OutputType) -> Self {
        CovenantArg::OutputType(output_type).into()
    }

    #[allow(dead_code)]
    pub fn field(field: OutputField) -> Self {
        CovenantArg::OutputField(field).into()
    }

    #[allow(dead_code)]
    pub fn fields(fields: Vec<OutputField>) -> Self {
        CovenantArg::OutputFields(fields.into()).into()
    }

    #[allow(dead_code)]
    pub fn bytes(bytes: Vec<u8>) -> Self {
        CovenantArg::Bytes(bytes).into()
    }
}

impl From<CovenantArg> for CovenantToken {
    fn from(arg: CovenantArg) -> Self {
        CovenantToken::Arg(Box::new(arg))
    }
}

impl From<CovenantFilter> for CovenantToken {
    fn from(filter: CovenantFilter) -> Self {
        CovenantToken::Filter(filter)
    }
}

#[derive(Debug, Clone, Default)]
/// `CovenantTokenCollection` structure. It wraps a collection of `CovenantToken`'s.
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
