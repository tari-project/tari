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
use tari_crypto::tari_utilities::hex::HexError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum InterfaceError {
    #[error("An error has occurred due to one of the parameters being null: `{0}`")]
    NullError(String),
    #[error("An error has occurred due to conversion failing for: `{0}`")]
    ConversionError(String),
    #[error("An error has occurred due to validation failing for: `{0}`")]
    InvalidHashError(String),
    #[error("An error has occurred due to difficulty being too low for share: `{0}`")]
    LowDifficultyError(String),
}

/// This struct is meant to hold an error for use by Miningcore. The error has an integer code and string
/// message
#[derive(Debug, Clone)]
pub struct MiningcoreError {
    pub code: i32,
    pub message: String,
}

impl From<InterfaceError> for MiningcoreError {
    fn from(v: InterfaceError) -> Self {
        match v {
            InterfaceError::NullError(_) => Self {
                code: 1,
                message: format!("{:?}", v),
            },
            InterfaceError::ConversionError(_) => Self {
                code: 2,
                message: format!("{:?}", v),
            },
            InterfaceError::InvalidHashError(_) => Self {
                code: 3,
                message: format!("{:?}", v),
            },
            InterfaceError::LowDifficultyError(_) => Self {
                code: 4,
                message: format!("{:?}", v),
            },
        }
    }
}

/// This implementation maps the internal HexError to a set of MiningcoreErrors.
/// The mapping is explicitly managed here.
impl From<HexError> for MiningcoreError {
    fn from(h: HexError) -> Self {
        match h {
            HexError::HexConversionError => Self {
                code: 404,
                message: format!("{:?}", h),
            },
            HexError::LengthError => Self {
                code: 501,
                message: format!("{:?}", h),
            },
            HexError::InvalidCharacter(_) => Self {
                code: 503,
                message: format!("{:?}", h),
            },
        }
    }
}
