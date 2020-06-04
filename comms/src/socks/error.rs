// Copyright 2019, The Tari Project
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

use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SocksError {
    #[error("Failure caused by an IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Failure due to invalid target address: {0}.")]
    InvalidTargetAddress(&'static str),
    #[error("Proxy server unreachable.")]
    ProxyServerUnreachable,
    #[error("Proxy server returns an invalid version number.")]
    InvalidResponseVersion,
    #[error("No acceptable auth methods")]
    NoAcceptableAuthMethods,
    #[error("Unknown auth method")]
    UnknownAuthMethod,
    #[error("General SOCKS server failure")]
    GeneralSocksServerFailure,
    #[error("Connection not allowed by ruleset")]
    ConnectionNotAllowedByRuleset,
    #[error("Network unreachable")]
    NetworkUnreachable,
    #[error("Host unreachable")]
    HostUnreachable,
    #[error("Connection refused")]
    ConnectionRefused,
    #[error("TTL expired")]
    TtlExpired,
    #[error("Command not supported")]
    CommandNotSupported,
    #[error("Address type not supported")]
    AddressTypeNotSupported,
    #[error("Unknown error")]
    UnknownError,
    #[error("Invalid reserved byte")]
    InvalidReservedByte,
    #[error("Unknown address type")]
    UnknownAddressType,
    #[error("Invalid authentication values: {0}.")]
    InvalidAuthValues(String),
    #[error("Password auth failure (code={0})")]
    PasswordAuthFailure(u8),
}
