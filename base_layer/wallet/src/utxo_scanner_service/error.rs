// Copyright 2021. The Tari Project
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

use serde_json::Error as SerdeJsonError;
use tari_comms::{connectivity::ConnectivityError, protocol::rpc::RpcError};
use tari_crypto::tari_utilities::hex::HexError;
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;

use crate::{error::WalletStorageError, output_manager_service::error::OutputManagerError};

#[derive(Debug, Error)]
pub enum UtxoScannerError {
    #[error("API returned something unexpected.")]
    UnexpectedApiResponse,
    #[error("Wallet storage error: `{0}`")]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Connectivity error: `{0}`")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("RpcError: `{0}`")]
    RpcError(#[from] RpcError),
    #[error("RpcStatus: `{0}`")]
    RpcStatus(String),
    #[error("Base Node Response Error: '{0}'")]
    BaseNodeResponseError(String),
    #[error("Utxo Scanning Error: '{0}")]
    UtxoScanningError(String),
    #[error("Hex conversion error: {0}")]
    HexError(#[from] HexError),
    #[error("Error converting a type")]
    ConversionError,
    #[error("Output manager error: `{0}`")]
    OutputManagerError(#[from] OutputManagerError),
    #[error("UTXO Import error: `{0}`")]
    UtxoImportError(String),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Serde json error: `{0}`")]
    SerdeJsonError(#[from] SerdeJsonError),
    #[error("Overflow Error")]
    OverflowError,
}
