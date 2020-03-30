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

use derive_error::Error;

#[derive(Debug, Error)]
pub enum SocksError {
    /// Failure caused by an IO error.
    Io(std::io::Error),
    /// Failure due to invalid target address.
    #[error(no_from, non_std)]
    InvalidTargetAddress(&'static str),
    /// Proxy server unreachable.
    ProxyServerUnreachable,
    /// Proxy server returns an invalid version number.
    InvalidResponseVersion,
    /// No acceptable auth methods
    NoAcceptableAuthMethods,
    /// Unknown auth method
    UnknownAuthMethod,
    /// General SOCKS server failure
    GeneralSocksServerFailure,
    /// Connection not allowed by ruleset
    ConnectionNotAllowedByRuleset,
    /// Network unreachable
    NetworkUnreachable,
    /// Host unreachable
    HostUnreachable,
    /// Connection refused
    ConnectionRefused,
    /// TTL expired
    TtlExpired,
    /// Command not supported
    CommandNotSupported,
    /// Address type not supported
    AddressTypeNotSupported,
    /// Unknown error
    UnknownError,
    /// Invalid reserved byte
    InvalidReservedByte,
    /// Unknown address type
    UnknownAddressType,
    // Invalid authentication values.
    #[error(msg_embedded, no_from, non_std)]
    InvalidAuthValues(String),
    /// Password auth failure
    #[error(no_from, non_std)]
    PasswordAuthFailure(u8),
}
