use tari_wallet::{contacts_service::error::ContactsServiceError, transaction_service::error::TransactionServiceError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UiError {
    #[error(transparent)]
    TransactionServiceError(#[from] TransactionServiceError),
    #[error(transparent)]
    ContactsServiceError(#[from] ContactsServiceError),
    #[error("Could not convert string into Public Key")]
    PublicKeyParseError,
}
