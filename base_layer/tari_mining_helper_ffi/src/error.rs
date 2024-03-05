// Copyright 2021. The Tari Project
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
use tari_utilities::hex::HexError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum InterfaceError {
    #[error("An error has occurred due to one of the parameters being null: `{0}`")]
    NullError(String),
    #[error("An error has occurred due to conversion failing for: `{0}`")]
    Conversion(String),
    #[error("An error has occurred due to validation failing for: `{0}`")]
    InvalidHash(String),
    #[error("An error has occurred due to difficulty being too low for share: `{0}`")]
    LowDifficulty(String),
    #[error("An error has occurred when checking the length of the allocated object")]
    AllocationError,
    #[error("An error because the supplied position was out of range")]
    PositionInvalidError,
    #[error("An error has occurred when trying to create the tokio runtime: `{0}`")]
    TokioError(String),
    #[error("An error has occurred when trying to create the a coinbase: `{0}`")]
    CoinbaseBuildError(String),
    #[error("An invalid address was passed in: `{0}`")]
    InvalidAddress(String),
    #[error("An invalid network was passed in: `{0}`")]
    InvalidNetwork(String),
}

/// This struct is meant to hold an error for use by Miningcore. The error has an integer code and string
/// message
#[derive(Debug, Clone)]
pub struct MiningHelperError {
    pub code: i32,
    pub message: String,
}

impl From<InterfaceError> for MiningHelperError {
    fn from(v: InterfaceError) -> Self {
        match v {
            InterfaceError::NullError(_) => Self {
                code: 1,
                message: format!("{:?}", v),
            },
            InterfaceError::Conversion(_) => Self {
                code: 2,
                message: format!("{:?}", v),
            },
            InterfaceError::InvalidHash(_) => Self {
                code: 3,
                message: format!("{:?}", v),
            },
            InterfaceError::LowDifficulty(_) => Self {
                code: 4,
                message: format!("{:?}", v),
            },
            InterfaceError::AllocationError => Self {
                code: 5,
                message: format!("{:?}", v),
            },
            InterfaceError::PositionInvalidError => Self {
                code: 6,
                message: format!("{:?}", v),
            },
            InterfaceError::TokioError(_) => Self {
                code: 7,
                message: format!("{:?}", v),
            },
            InterfaceError::CoinbaseBuildError(_) => Self {
                code: 8,
                message: format!("{:?}", v),
            },
            InterfaceError::InvalidAddress(_) => Self {
                code: 9,
                message: format!("{:?}", v),
            },
            InterfaceError::InvalidNetwork(_) => Self {
                code: 10,
                message: format!("{:?}", v),
            },
        }
    }
}

/// This implementation maps the internal HexError to a set of StratumTranscoderErrors.
/// The mapping is explicitly managed here.
impl From<HexError> for MiningHelperError {
    fn from(h: HexError) -> Self {
        match h {
            HexError::HexConversionError {} => Self {
                code: 404,
                message: format!("{:?}", h),
            },
            HexError::LengthError {} => Self {
                code: 501,
                message: format!("{:?}", h),
            },
            HexError::InvalidCharacter {} => Self {
                code: 503,
                message: format!("{:?}", h),
            },
        }
    }
}
