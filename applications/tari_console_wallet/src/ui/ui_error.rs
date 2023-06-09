// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_comms::connectivity::ConnectivityError;
use tari_contacts::contacts_service::error::ContactsServiceError;
use tari_utilities::hex::HexError;
use tari_wallet::{
    error::{WalletError, WalletStorageError},
    output_manager_service::error::OutputManagerError,
    transaction_service::error::TransactionServiceError,
};
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
    #[error(transparent)]
    HexError(#[from] HexError),
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
    #[error("Channel send error: `{0}`")]
    SendError(String),
}
