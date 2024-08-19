//  Copyright 2022. The Tari Project
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

use minotari_wallet::{
    error::{WalletError, WalletStorageError},
    output_manager_service::error::OutputManagerError,
    transaction_service::error::TransactionServiceError,
};
use tari_comms::connectivity::ConnectivityError;
use tari_contacts::contacts_service::error::ContactsServiceError;
use tari_utilities::hex::HexError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UiError {
    #[error(transparent)]
    TransactionService(#[from] TransactionServiceError),
    #[error(transparent)]
    OutputManager(#[from] OutputManagerError),
    #[error(transparent)]
    ContactsService(#[from] ContactsServiceError),
    #[error(transparent)]
    Connectivity(#[from] ConnectivityError),
    #[error("Conversion: `{0}`")]
    HexError(String),
    #[error(transparent)]
    WalletError(#[from] WalletError),
    #[error(transparent)]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Could not convert string into Public Key")]
    PublicKeyParseError,
    #[error("Could not convert string into Net Address")]
    AddressParseError,
    #[error("Peer did not include an address")]
    NoAddress,
    #[error("Specified burn proof file already exists")]
    BurntProofFileExists,
    #[error("Channel send error: `{0}`")]
    SendError(String),
    #[error("Transaction error: `{0}`")]
    TransactionError(String),
    #[error("Couldn't read wallet type")]
    WalletTypeError,
}

impl From<HexError> for UiError {
    fn from(err: HexError) -> Self {
        UiError::HexError(err.to_string())
    }
}
