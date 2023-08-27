//  Copyright 2021, The Taiji Project
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

use std::io;

use super::{
    absolute_height::AbsoluteHeightFilter,
    and::AndFilter,
    field_eq::FieldEqFilter,
    fields_hashed_eq::FieldsHashedEqFilter,
    fields_preserved::FieldsPreservedFilter,
    identity::IdentityFilter,
    not::NotFilter,
    or::OrFilter,
    output_hash_eq::OutputHashEqFilter,
    xor::XorFilter,
};
use crate::covenants::{
    byte_codes,
    context::CovenantContext,
    decoder::CovenantDecodeError,
    encoder::CovenentWriteExt,
    error::CovenantError,
    output_set::OutputSet,
};

/// The filter trait is implemented by all covenant filters.
pub trait Filter {
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError>;
}

/// A covenant filter is a filter that can be used in a covenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CovenantFilter {
    Identity(IdentityFilter),
    And(AndFilter),
    Or(OrFilter),
    Xor(XorFilter),
    Not(NotFilter),
    OutputHashEq(OutputHashEqFilter),
    FieldsPreserved(FieldsPreservedFilter),
    FieldEq(FieldEqFilter),
    FieldsHashedEq(FieldsHashedEqFilter),
    AbsoluteHeight(AbsoluteHeightFilter),
}

impl CovenantFilter {
    /// Determine if the given byte code is a valid filter code.
    pub fn is_valid_code(code: u8) -> bool {
        byte_codes::is_valid_filter_code(code)
    }

    /// Write the filter to the given writer as byte code.
    pub fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_u8_fixed(self.as_byte_code())?;
        Ok(())
    }

    fn as_byte_code(&self) -> u8 {
        use byte_codes::*;
        #[allow(clippy::enum_glob_use)]
        use CovenantFilter::*;

        match self {
            Identity(_) => FILTER_IDENTITY,
            And(_) => FILTER_AND,
            Or(_) => FILTER_OR,
            Xor(_) => FILTER_XOR,
            Not(_) => FILTER_NOT,
            OutputHashEq(_) => FILTER_OUTPUT_HASH_EQ,
            FieldsPreserved(_) => FILTER_FIELDS_PRESERVED,
            FieldEq(_) => FILTER_FIELD_EQ,
            FieldsHashedEq(_) => FILTER_FIELDS_HASHED_EQ,
            AbsoluteHeight(_) => FILTER_ABSOLUTE_HEIGHT,
        }
    }

    /// Try to create a covenant filter from the given byte code.
    pub fn try_from_byte_code(code: u8) -> Result<Self, CovenantDecodeError> {
        use byte_codes::*;
        match code {
            FILTER_IDENTITY => Ok(Self::identity()),
            FILTER_AND => Ok(Self::and()),
            FILTER_OR => Ok(Self::or()),
            FILTER_XOR => Ok(Self::xor()),
            FILTER_NOT => Ok(Self::not()),
            FILTER_OUTPUT_HASH_EQ => Ok(Self::output_hash_eq()),
            FILTER_FIELDS_PRESERVED => Ok(Self::fields_preserved()),
            FILTER_FIELD_EQ => Ok(Self::field_eq()),
            FILTER_FIELDS_HASHED_EQ => Ok(Self::fields_hashed_eq()),
            FILTER_ABSOLUTE_HEIGHT => Ok(Self::absolute_height()),
            _ => Err(CovenantDecodeError::UnknownFilterByteCode { code }),
        }
    }

    /// Return the "identity" covenant filter.
    pub fn identity() -> Self {
        CovenantFilter::Identity(IdentityFilter)
    }

    /// Return the "and" covenant filter.
    pub fn and() -> Self {
        CovenantFilter::And(AndFilter)
    }

    /// Return the "or" covenant filter.
    pub fn or() -> Self {
        CovenantFilter::Or(OrFilter)
    }

    /// Return the "xor" covenant filter.
    pub fn xor() -> Self {
        CovenantFilter::Xor(XorFilter)
    }

    /// Return the "not" covenant filter.
    pub fn not() -> Self {
        CovenantFilter::Not(NotFilter)
    }

    /// Return the "output hash eq" covenant filter.
    pub fn output_hash_eq() -> Self {
        CovenantFilter::OutputHashEq(OutputHashEqFilter)
    }

    /// Return the "fields preserved" covenant filter.
    pub fn fields_preserved() -> Self {
        CovenantFilter::FieldsPreserved(FieldsPreservedFilter)
    }

    /// Return the "field eq" covenant filter.
    pub fn field_eq() -> Self {
        CovenantFilter::FieldEq(FieldEqFilter)
    }

    /// Return the "fields hashed eq" covenant filter.
    pub fn fields_hashed_eq() -> Self {
        CovenantFilter::FieldsHashedEq(FieldsHashedEqFilter)
    }

    /// Return the "absolute height" covenant filter.
    pub fn absolute_height() -> Self {
        CovenantFilter::AbsoluteHeight(AbsoluteHeightFilter)
    }
}

impl Filter for CovenantFilter {
    // Filter the given output set using the filter specified by the covenant context.
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        #[allow(clippy::enum_glob_use)]
        use CovenantFilter::*;
        match self {
            Identity(identity) => identity.filter(context, output_set),
            And(and) => and.filter(context, output_set),
            Or(or) => or.filter(context, output_set),
            Xor(xor) => xor.filter(context, output_set),
            Not(not) => not.filter(context, output_set),
            OutputHashEq(output_hash_eq) => output_hash_eq.filter(context, output_set),
            FieldsPreserved(fields_preserved) => fields_preserved.filter(context, output_set),
            FieldEq(fields_eq) => fields_eq.filter(context, output_set),
            FieldsHashedEq(fields_hashed_eq) => fields_hashed_eq.filter(context, output_set),
            AbsoluteHeight(abs_height) => abs_height.filter(context, output_set),
        }
    }
}
