//  Copyright 2024. The Tari Project
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
use minotari_app_grpc::authentication::BasicAuthError;
use minotari_app_utilities::parse_miner_input::ParseInputError;
use tari_max_size::MaxSizeBytesError;
use thiserror::Error;
use tonic::codegen::http::uri::InvalidUri;

#[derive(Debug, Error)]
pub enum MinerError {
    #[error("I/O error")]
    IOError(#[from] std::io::Error),
    #[error("gRPC error: {0}")]
    GrpcStatus(#[from] tonic::Status),
    #[error("Connection error: {0}")]
    GrpcConnection(#[from] tonic::transport::Error),
    #[error("Node not ready")]
    NodeNotReady,
    #[error("Blockchain reached specified height {0}, mining will be stopped")]
    MineUntilHeightReached(u64),
    #[error("Block height {0} already mined")]
    MinerLostBlock(u64),
    #[error("Expected non empty {0}")]
    EmptyObject(String),
    #[error("Invalid block header {0}")]
    BlockHeader(String),
    #[error("Conversion error: {0}")]
    Conversion(String),
    #[error("Invalid gRPC credentials: {0}")]
    BasicAuthError(#[from] BasicAuthError),
    #[error("Invalid gRPC url: {0}")]
    InvalidUri(#[from] InvalidUri),
    #[error("TLS connection error: {0}")]
    TlsConnectionError(String),
    #[error("Coinbase error: {0}")]
    CoinbaseError(String),
    #[error("Consensus build error: {0}")]
    ParseInputError(#[from] ParseInputError),
    #[error("Base node not responding to gRPC requests: {0}")]
    BaseNodeNotResponding(String),
    #[error("Limit error :{0}")]
    MaxSizeBytesError(#[from] MaxSizeBytesError),
    #[error("Logical process error: {0}")]
    LogicalError(String),
}

pub fn err_empty(name: &str) -> MinerError {
    MinerError::EmptyObject(name.to_string())
}
