// Copyright 2020. The Tari Project
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

use std::num::{ParseFloatError, ParseIntError};

use chrono_english::DateError;
use log::*;
use tari_app_utilities::utilities::ExitCodes;
use tari_core::transactions::tari_amount::MicroTariError;
use tari_wallet::{
    output_manager_service::error::OutputManagerError,
    transaction_service::error::TransactionServiceError,
};
use thiserror::Error;
use tokio::task::JoinError;

pub const LOG_TARGET: &str = "tari_console_wallet::error";

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Argument error - were they in the right order?")]
    Argument,
    #[error("Transaction service error `{0}`")]
    Transaction(#[from] TransactionServiceError),
    #[error("Output manager error: `{0}`")]
    OutputManagerError(#[from] OutputManagerError),
    #[error("Tokio join error `{0}`")]
    Join(#[from] JoinError),
    #[error("Config error `{0}`")]
    Config(String),
    #[error("Comms error `{0}`")]
    Comms(String),
}

impl From<CommandError> for ExitCodes {
    fn from(err: CommandError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        let msg = format!("Command error: {}", err);
        Self::CommandError(msg)
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Failed to parse wallet command at `{0}`.")]
    WalletCommand(String),
    #[error("Failed to parse Tari amount.")]
    MicroTariAmount(#[from] MicroTariError),
    #[error("Failed to parse public key or emoji id.")]
    PublicKey,
    #[error("Failed to parse a missing {0}.")]
    Empty(String),
    #[error("Failed to parse float.")]
    Float(#[from] ParseFloatError),
    #[error("Failed to parse int.")]
    Int(#[from] ParseIntError),
    #[error("Failed to parse date. {0}")]
    Date(#[from] DateError),
    #[error("Invalid combination of arguments.")]
    Invalid,
    #[error("Parsing not yet implemented for {0}.")]
    Unimplemented(String),
}

impl From<ParseError> for ExitCodes {
    fn from(err: ParseError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        let msg = format!("Failed to parse input file commands! {}", err);
        Self::InputError(msg)
    }
}
