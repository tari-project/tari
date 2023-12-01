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
use serde::{Deserialize, Serialize};

/// The type of range proof used in the output
#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Deserialize, Serialize, Eq, BorshSerialize, FromPrimitive, BorshDeserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
#[borsh(use_discriminant = true)]
pub enum RangeProofType {
    /// Range proof is a BulletProofPlus
    BulletProofPlus = 0,
    /// Range proof is a revealed value
    RevealedValue = 1,
}

impl RangeProofType {
    /// Returns a single byte that represents this RangeProofType
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// Returns the RangeProofType that corresponds to this RangeProofType. If the byte does not correspond to any
    /// RangeProofType, None is returned.
    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }

    pub const fn all() -> &'static [Self] {
        &[RangeProofType::BulletProofPlus, RangeProofType::RevealedValue]
    }
}

impl Default for RangeProofType {
    fn default() -> Self {
        Self::BulletProofPlus
    }
}

impl Display for RangeProofType {
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
        assert_eq!(RangeProofType::default(), RangeProofType::BulletProofPlus);
        assert_eq!(RangeProofType::all(), [
            RangeProofType::BulletProofPlus,
            RangeProofType::RevealedValue,
        ]);
        assert_eq!(RangeProofType::from_byte(0), Some(RangeProofType::BulletProofPlus));
        assert_eq!(RangeProofType::from_byte(1), Some(RangeProofType::RevealedValue));
        assert_eq!(RangeProofType::from_byte(2), None);
        assert_eq!(RangeProofType::BulletProofPlus.as_byte(), 0);
        assert_eq!(RangeProofType::RevealedValue.as_byte(), 1);
        assert_eq!(RangeProofType::BulletProofPlus.to_string(), "BulletProofPlus");
        assert_eq!(RangeProofType::RevealedValue.to_string(), "RevealedValue");
    }

    #[derive(Clone, Serialize, Deserialize, Debug)]
    #[allow(clippy::struct_excessive_bools)]
    struct TestConfig {
        name: String,
        range_proof_type: RangeProofType,
    }

    #[test]
    fn it_deserializes_enums() {
        let config_str_1 = r#"
            name = "blockchain champion"
            range_proof_type = "revealed_value"
        "#;
        let config_1 = toml::from_str::<TestConfig>(config_str_1).unwrap();
        let config_str_2 = r#"
            name = "blockchain champion"
            range_proof_type = "bullet_proof_plus"
        "#;
        let config_2 = toml::from_str::<TestConfig>(config_str_2).unwrap();

        // Enums in the config
        assert_eq!(config_1.range_proof_type, RangeProofType::RevealedValue);
        assert_eq!(config_2.range_proof_type, RangeProofType::BulletProofPlus);
    }
}
