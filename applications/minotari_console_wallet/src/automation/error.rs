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

use std::{
    io,
    num::{ParseFloatError, ParseIntError},
};

use log::*;
use minotari_app_grpc::tls::error::GrpcTlsError;
use minotari_wallet::{
    error::{WalletError, WalletStorageError},
    output_manager_service::error::OutputManagerError,
    transaction_service::error::TransactionServiceError,
};
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_types::types::FixedHashSizeError;
use tari_core::transactions::{tari_amount::MicroMinotariError, transaction_components::TransactionError};
use tari_crypto::signatures::SchnorrSignatureError;
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use tari_utilities::{hex::HexError, ByteArrayError};
use thiserror::Error;
use tokio::task::JoinError;

pub const LOG_TARGET: &str = "wallet::automation::error";

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum CommandError {
    #[error("Argument error - were they in the right order?")]
    Argument,
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Minotari value error `{0}`")]
    MicroMinotariError(#[from] MicroMinotariError),
    #[error("Transaction service error `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Transaction service error `{0}`")]
    TransactionServiceError(#[from] TransactionServiceError),
    #[error("Output manager error: `{0}`")]
    OutputManagerError(#[from] OutputManagerError),
    #[error("Key manager error: `{0}`")]
    KeyManagerError(#[from] KeyManagerServiceError),
    #[error("Tokio join error `{0}`")]
    Join(#[from] JoinError),
    #[error("Config error `{0}`")]
    Config(String),
    #[error("Comms error `{0}`")]
    Comms(String),
    #[error("CSV file error `{0}`")]
    CSVFile(String),
    #[error("Wallet error `{0}`")]
    WalletError(#[from] WalletError),
    #[error("Wallet storage error `{0}`")]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Hex error `{0}`")]
    HexError(String),
    #[error("Error `{0}`")]
    ShaError(String),
    #[error("JSON file error `{0}`")]
    JsonFile(String),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("General error: {0}")]
    General(String),
    #[error("Pre-mine error: {0}")]
    PreMine(String),
    #[error("FixedHash size error `{0}`")]
    FixedHashSizeError(#[from] FixedHashSizeError),
    #[error("ByteArrayError {0}")]
    ByteArrayError(String),
    #[error("gRPC TLS cert error {0}")]
    GrpcTlsError(#[from] GrpcTlsError),
    #[error("Invalid signature: `{0}`")]
    FailedSignature(#[from] SchnorrSignatureError),
}

impl From<HexError> for CommandError {
    fn from(err: HexError) -> Self {
        CommandError::HexError(err.to_string())
    }
}

impl From<ByteArrayError> for CommandError {
    fn from(e: ByteArrayError) -> Self {
        CommandError::ByteArrayError(e.to_string())
    }
}

impl From<CommandError> for ExitError {
    fn from(err: CommandError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        Self::new(ExitCode::CommandError, err.to_string())
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Failed to parse wallet command at `{0}`.")]
    WalletCommand(String),
    #[error("Failed to parse Minotari amount.")]
    MicroMinotariAmount(#[from] MicroMinotariError),
    #[error("Failed to parse public key or emoji id.")]
    PublicKey,
    #[error("Failed to parse hash")]
    Hash,
    #[error("Failed to parse a missing {0}")]
    Empty(String),
    #[error("Failed to parse float.")]
    Float(#[from] ParseFloatError),
    #[error("Failed to parse int.")]
    Int(#[from] ParseIntError),
    #[error("Failed to parse date. {0}")]
    Date(#[from] chrono::ParseError),
    #[error("Failed to parse a net address.")]
    Address,
    #[error("Invalid combination of arguments ({0}).")]
    Invalid(String),
    #[error("Parsing not yet implemented for {0}.")]
    Unimplemented(String),
}

impl From<ParseError> for ExitError {
    fn from(err: ParseError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        let msg = format!("Failed to parse input file commands! {}", err);
        Self::new(ExitCode::InputError, msg)
    }
}
