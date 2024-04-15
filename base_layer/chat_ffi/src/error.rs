// Copyright 2023. The Tari Project
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
use log::*;
use thiserror::Error;

const LOG_TARGET: &str = "chat_ffi::error";

#[derive(Debug, Error, PartialEq)]
pub enum InterfaceError {
    #[error("An error has occurred due to one of the parameters being null: `{0}`")]
    NullError(String),
    #[error("An error has occurred when trying to create the tokio runtime: `{0}`")]
    TokioError(String),
    #[error("Something about the argument is invalid: `{0}`")]
    InvalidArgument(String),
    #[error("An error has occurred when checking the length of the allocated object")]
    AllocationError,
    #[error("An error because the supplied position was out of range")]
    PositionInvalidError,
    #[error("Conversion error: `{0}`")]
    ConversionError(String),
    #[error("The client had an error communication with contact services")]
    ContactServiceError(String),
}

/// This struct is meant to hold an error for use by FFI client applications. The error has an integer code and string
/// message
#[derive(Debug, Clone)]
pub struct LibChatError {
    pub code: i32,
    pub message: String,
}

impl From<InterfaceError> for LibChatError {
    fn from(v: InterfaceError) -> Self {
        error!(target: LOG_TARGET, "{}", format!("{:?}", v));
        match v {
            InterfaceError::NullError(_) => Self {
                code: 1,
                message: format!("{:?}", v),
            },
            InterfaceError::TokioError(_) => Self {
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
            InterfaceError::InvalidArgument(_) => Self {
                code: 7,
                message: format!("{:?}", v),
            },
            InterfaceError::ContactServiceError(_) => Self {
                code: 8,
                message: format!("{:?}", v),
            },
            InterfaceError::ConversionError(_) => Self {
                code: 9,
                message: format!("{:?}", v),
            },
        }
    }
}
