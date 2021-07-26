use tari_comms::connectivity::ConnectivityError;
use tari_crypto::tari_utilities::hex::HexError;
use tari_wallet::{
    contacts_service::error::ContactsServiceError,
    error::{WalletError, WalletStorageError},
    output_manager_service::error::OutputManagerError,
    transaction_service::error::TransactionServiceError,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UiError {
    #[error(transparent)]
    TransactionServiceError(#[from] TransactionServiceError),
    #[error(transparent)]
    OutputManagerError(#[from] OutputManagerError),
    #[error(transparent)]
    ContactsServiceError(#[from] ContactsServiceError),
    #[error(transparent)]
    ConnectivityError(#[from] ConnectivityError),
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
    NoAddressError,
}
