use tari_common_types::types::FixedHashSizeError;
use thiserror::Error;

use crate::optional::IsNotFoundError;

#[derive(Error, Debug)]
pub enum GrpcBaseNodeError {
    #[error("Could not connect to base node")]
    ConnectionError,
    #[error("Connection error: {0}")]
    GrpcConnection(#[from] tonic::transport::Error),
    #[error("GRPC error: {0}")]
    GrpcStatus(#[from] tonic::Status),
    #[error("Peer sent an invalid message: {0}")]
    InvalidPeerMessage(String),
    #[error("Hash size error: {0}")]
    HashSizeError(#[from] FixedHashSizeError),
    #[error("Node not found: {0}")]
    NodeNotFound(String),
}

impl IsNotFoundError for GrpcBaseNodeError {
    fn is_not_found_error(&self) -> bool {
        if let Self::GrpcStatus(status) = self {
            status.code() == tonic::Code::NotFound
        } else {
            false
        }
    }
}
