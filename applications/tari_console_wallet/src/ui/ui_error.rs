use tari_wallet::transaction_service::error::TransactionServiceError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UiError {
    #[error(transparent)]
    WalletError(#[from] TransactionServiceError),
}
