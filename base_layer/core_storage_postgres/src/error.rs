use derive_error::Error;
use diesel::{result::Error as DieselError, ConnectionError};
use tari_core::chain_storage::ChainStorageError;
use tari_crypto::tari_utilities::hex::HexError;

#[derive(Debug, Error)]
pub enum PostgresChainStorageError {
    #[error(msg_embedded, non_std, no_from)]
    UpdateError(String),
    #[error(msg_embedded, non_std, no_from)]
    FetchError(String),
    ChainStorageError(ChainStorageError),
    DieselError(DieselError),
    HexError(HexError),
    ConnectionError(ConnectionError),
}

impl From<PostgresChainStorageError> for ChainStorageError {
    fn from(e: PostgresChainStorageError) -> Self {
        // TODO: Flesh this out better
        ChainStorageError::AccessError(format!("Postgres error:{}", e))
    }
}
