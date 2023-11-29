// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::fmt::{Display, Formatter};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Deserialize_repr,
    Serialize_repr,
    PartialEq,
    Eq,
    FromPrimitive,
    BorshSerialize,
    BorshDeserialize,
)]
#[repr(u8)]
pub enum OutputType {
    /// An standard output.
    Standard = 0,
    /// Output is a coinbase output, must not be spent until maturity.
    Coinbase = 1,
    /// Output is a burned output and can not be spent ever.
    Burn = 2,
    /// Output defines a validator node registration
    ValidatorNodeRegistration = 3,
    /// Output defines a new re-usable code template.
    CodeTemplateRegistration = 4,
}

impl OutputType {
    /// Returns a single byte that represents this OutputType
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// Returns the OutputType that corresponds to this OutputType. If the byte does not correspond to any OutputType,
    /// None is returned.
    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }

    pub const fn all() -> &'static [Self] {
        &[
            OutputType::Standard,
            OutputType::Coinbase,
            OutputType::Burn,
            OutputType::ValidatorNodeRegistration,
            OutputType::CodeTemplateRegistration,
        ]
    }

    pub fn is_sidechain_type(&self) -> bool {
        matches!(
            self,
            OutputType::ValidatorNodeRegistration | OutputType::CodeTemplateRegistration | OutputType::Burn
        )
    }
}

impl Default for OutputType {
    fn default() -> Self {
        Self::Standard
    }
}

impl Display for OutputType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Debug "shortcut" works because variants do not have fields
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_converts_from_byte_to_output_type() {
        assert_eq!(OutputType::from_byte(0), Some(OutputType::Standard));
        assert_eq!(OutputType::from_byte(1), Some(OutputType::Coinbase));
        assert_eq!(OutputType::from_byte(2), Some(OutputType::Burn));
        assert_eq!(OutputType::from_byte(3), Some(OutputType::ValidatorNodeRegistration));
        assert_eq!(OutputType::from_byte(4), Some(OutputType::CodeTemplateRegistration));
        for i in 5..=255 {
            assert_eq!(OutputType::from_byte(i), None);
        }
    }
}
