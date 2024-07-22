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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Request error: {0}")]
    Request(String),
    // ResponseError(String),
    #[error("Failed to parse JSON: {0}")]
    Json(#[from] serde_json::error::Error),
    #[error("Blob is not a valid hex value: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("System time error: {0}")]
    Time(#[from] std::time::SystemTimeError),
    #[error("Client Tx is not set")]
    ClientTxNotSet,
    #[error("Io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Can't create TLS connector: {0}")]
    Tls(#[from] native_tls::Error),
    #[error("Can't establish TLS connection: {0}")]
    Tcp(#[from] Box<native_tls::HandshakeError<std::net::TcpStream>>),
    #[error("No connected stream")]
    NotConnected,
    #[error("Can't parse int: {0}")]
    Parse(#[from] std::num::ParseIntError),

    #[error("General error: {0}")]
    General(String),
    #[error("Missing Data error: {0}")]
    MissingData(String),
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(error: std::sync::PoisonError<T>) -> Self {
        Error::General(format!("Failed to get lock: {:?}", error))
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for Error {
    fn from(error: std::sync::mpsc::SendError<T>) -> Self {
        Error::General(format!("Failed to send to a channel: {:?}", error))
    }
}

impl From<native_tls::HandshakeError<std::net::TcpStream>> for Error {
    fn from(error: native_tls::HandshakeError<std::net::TcpStream>) -> Self {
        Error::General(format!("TLS handshake error: {:?}", error))
    }
}
