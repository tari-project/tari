use tari_utilities::ByteArrayError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Missing field: {0}")]
    MissingField(&'static str),
    #[error("Public key conversion error: {0}")]
    PublicKey(#[from] ByteArrayError),
    #[error("Secret key conversion error: {0}")]
    SecretKey(#[from] ByteArrayError),
}
