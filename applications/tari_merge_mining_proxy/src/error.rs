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

use hex::FromHexError;
use std::io;
use tari_common::{ConfigError, ConfigurationError};
use tari_core::{proof_of_work::monero_rx::MergeMineError, transactions::CoinbaseBuildError};
use thiserror::Error;
use tonic::transport;

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
    #[error("Missing data:{0}")]
    MissingDataError(String),
    #[error("An IO error occurred: {0}")]
    IoError(#[from] io::Error),
    #[error("Tonic transport error: {0}")]
    TonicTransportError(#[from] transport::Error),
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
}
