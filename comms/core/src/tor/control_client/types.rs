// Copyright 2020, The Taiji Project
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

use std::{fmt, net::SocketAddr};

use serde_derive::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug)]
pub enum KeyType {
    /// The server should generate a key of algorithm KeyBlob
    New,
    /// The server should use the 1024-bit RSA key provided in as KeyBlob (v2).
    Rsa1024,
    /// The server should use the ED25519-V3 key provided in as KeyBlob (v3).
    Ed25519V3,
}

impl KeyType {
    pub fn as_tor_repr(self) -> &'static str {
        match self {
            KeyType::New => "NEW",
            KeyType::Rsa1024 => "RSA1024",
            KeyType::Ed25519V3 => "ED25519-V3",
        }
    }
}

pub enum KeyBlob<'a> {
    /// The server should generate a key using the "best" supported algorithm (KeyType == "NEW").
    Best,
    /// The server should generate a 1024 bit RSA key (KeyType == "NEW") (v2).
    Rsa1024,
    /// The server should generate an ed25519 private key (KeyType == "NEW") (v3).
    Ed25519V3,
    /// A serialized private key (without whitespace)
    String(&'a str),
}

impl KeyBlob<'_> {
    pub fn as_tor_repr(&self) -> &str {
        match self {
            KeyBlob::Best => "BEST",
            KeyBlob::Rsa1024 => "RSA1024",
            KeyBlob::Ed25519V3 => "ED25519-V3",
            KeyBlob::String(priv_key) => priv_key,
        }
    }
}

impl fmt::Display for KeyBlob<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use KeyBlob::{Best, Ed25519V3, Rsa1024, String};
        match self {
            Best => write!(f, "Best"),
            Rsa1024 => write!(f, "Rsa1024"),
            Ed25519V3 => write!(f, "Ed25519V3"),
            String(_) => write!(f, "[redacted]"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub enum PrivateKey {
    /// The server should use the 1024 bit RSA key provided in as KeyBlob (v2).
    Rsa1024(String),
    /// The server should use the ed25519 v3 key provided in as KeyBlob (v3).
    Ed25519V3(String),
}

/// Represents a mapping between an onion port and a proxied address (usually 127.0.0.1:xxxx).
/// If the proxied_address is not specified, the default `127.0.0.1:[onion_port]` will be used.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PortMapping {
    onion_port: u16,
    proxied_address: SocketAddr,
}

impl PortMapping {
    /// Returns a new `PortMapping` with the proxied address set to 127.0.0.1:{onion_port}
    pub fn from_port(onion_port: u16) -> Self {
        Self {
            onion_port,
            proxied_address: ([127, 0, 0, 1], onion_port).into(),
        }
    }

    /// Returns a new `PortMapping` with the specified proxied address
    pub fn new(onion_port: u16, proxied_address: SocketAddr) -> Self {
        Self {
            onion_port,
            proxied_address,
        }
    }

    pub fn onion_port(&self) -> u16 {
        self.onion_port
    }

    pub fn proxied_address(&self) -> &SocketAddr {
        &self.proxied_address
    }

    pub fn set_proxied_addr(&mut self, proxied_addr: SocketAddr) {
        self.proxied_address = proxied_addr
    }
}

impl From<u16> for PortMapping {
    fn from(onion_port: u16) -> Self {
        Self::from_port(onion_port)
    }
}

impl<T: Into<u16>, U: Into<SocketAddr>> From<(T, U)> for PortMapping {
    fn from((port, addr): (T, U)) -> Self {
        Self {
            onion_port: port.into(),
            proxied_address: addr.into(),
        }
    }
}

impl fmt::Display for PortMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "PortMapping [{} -> {}]", self.onion_port, self.proxied_address)
    }
}

impl fmt::Debug for PortMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
