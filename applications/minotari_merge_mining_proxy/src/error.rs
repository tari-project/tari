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

//! All errors that can occur in `Merge mining proxy`.

use std::io;

use hex::FromHexError;
use hyper::header::InvalidHeaderValue;
use minotari_app_utilities::parse_miner_input::ParseInputError;
use minotari_wallet_grpc_client::BasicAuthError;
use tari_common::{ConfigError, ConfigurationError};
use tari_core::{
    consensus::ConsensusBuilderError,
    proof_of_work::{monero_rx::MergeMineError, DifficultyError},
    transactions::{key_manager::CoreKeyManagerError, CoinbaseBuildError},
};
use tari_key_manager::key_manager_service::KeyManagerServiceError;
use thiserror::Error;
use tonic::{codegen::http::uri::InvalidUri, transport};

#[derive(Debug, Error)]
pub enum MmProxyError {
    #[error("Configuration error: {0}")]
    ConfigurationError(#[from] ConfigurationError),
    #[error("Configuration error: {0}")]
    ConfigError(#[from] ConfigError),
    #[error("Merge mining error: {source}")]
    MergeMiningError {
        #[from]
        source: MergeMineError,
    },
    #[error("Invalid URI: {0}")]
    InvalidUriError(#[from] InvalidUri),
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Missing data:{0}")]
    MissingDataError(String),
    #[error("An IO error occurred: {0}")]
    IoError(#[from] io::Error),
    #[error("Tonic transport error: {0}")]
    TonicTransportError(#[from] transport::Error),
    #[error("Grpc authentication error: {0}")]
    GRPCAuthenticationError(#[from] BasicAuthError),
    #[error("GRPC response did not contain the expected field: `{0}`")]
    GrpcResponseMissingField(&'static str),
    #[error("Hyper error: {0}")]
    HyperError(#[from] hyper::Error),
    #[error("Invalid monerod response: {0}")]
    InvalidMonerodResponse(String),
    #[error("Failed to send request to monerod: {0}")]
    MonerodRequestFailed(reqwest::Error),
    #[error("GRPC request failed with `{status}` {details}")]
    GrpcRequestError {
        #[source]
        status: tonic::Status,
        details: String,
    },
    #[error("HTTP error: {0}")]
    HttpError(#[from] hyper::http::Error),
    #[error("Could not parse URL: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Bincode error: {0}")]
    BincodeError(#[from] bincode::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Hex error: {0}")]
    HexError(#[from] FromHexError),
    #[error("Coinbase builder error: {0}")]
    CoinbaseBuilderError(#[from] CoinbaseBuildError),
    #[error("Unexpected Minotari base node response: {0}")]
    UnexpectedTariBaseNodeResponse(String),
    #[error("Invalid header value")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error("Block was lost due to a failed precondition, and should be retried")]
    FailedPreconditionBlockLostRetry,
    #[error("Could not convert data:{0}")]
    ConversionError(String),
    #[error("No reachable servers in configuration")]
    ServersUnavailable,
    #[error("Invalid difficulty: {0}")]
    DifficultyError(#[from] DifficultyError),
    #[error("TLS connection error: {0}")]
    TlsConnectionError(String),
    #[error("Key manager service error: `{0}`")]
    KeyManagerServiceError(String),
    #[error("Key manager error: {0}")]
    CoreKeyManagerError(#[from] CoreKeyManagerError),
    #[error("Consensus build error: {0}")]
    ConsensusBuilderError(#[from] ConsensusBuilderError),
    #[error("Consensus build error: {0}")]
    ParseInputError(#[from] ParseInputError),
    #[error("Base node not responding to gRPC requests: {0}")]
    BaseNodeNotResponding(String),
    #[error("Unexpected missing data: {0}")]
    UnexpectedMissingData(String),
    #[error("Failed to get block template: {0}")]
    FailedToGetBlockTemplate(String),
}

impl From<tonic::Status> for MmProxyError {
    fn from(status: tonic::Status) -> Self {
        Self::GrpcRequestError {
            details: String::from_utf8_lossy(status.details()).to_string(),
            status,
        }
    }
}

impl From<KeyManagerServiceError> for MmProxyError {
    fn from(err: KeyManagerServiceError) -> Self {
        MmProxyError::KeyManagerServiceError(err.to_string())
    }
}

#[cfg(test)]
pub mod test {
    use tonic::Code;

    use super::*;

    #[test]
    pub fn test_from() {
        let status = tonic::Status::new(Code::Unknown, "message");
        let error = MmProxyError::from(status);
        assert!(matches!(error, MmProxyError::GrpcRequestError {
            status: _,
            details: _
        }));
    }
}
