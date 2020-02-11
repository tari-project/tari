// Copyright 2020, The Tari Project
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

use std::borrow::Cow;

pub enum KeyType {
    /// The server should generate a key of algorithm KeyBlob
    New,
    /// The server should use the 1024-bit RSA key provided in as KeyBlob (v2).
    Rsa1024,
    /// The server should use the ED25519-V3 key provided in as KeyBlob (v3).
    Ed25519V3,
}

impl KeyType {
    pub fn as_tor_repr(&self) -> &'static str {
        match self {
            KeyType::New => "NEW",
            KeyType::Rsa1024 => "RSA1024",
            KeyType::Ed25519V3 => "ED25519-V3",
        }
    }
}

pub enum KeyBlob {
    /// The server should generate a key using the "best" supported algorithm (KeyType == "NEW").
    Best,
    /// The server should generate a 1024 bit RSA key (KeyType == "NEW") (v2).
    Rsa1024,
    /// The server should generate an ed25519 private key (KeyType == "NEW") (v3).
    Ed25519V3,
    /// A serialized private key (without whitespace)
    String(String),
}

impl KeyBlob {
    pub fn as_tor_repr(&self) -> &str {
        match self {
            KeyBlob::Best => "BEST",
            KeyBlob::Rsa1024 => "RSA1024",
            KeyBlob::Ed25519V3 => "ED25519-V3",
            KeyBlob::String(priv_key) => priv_key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateKey<'a> {
    /// The server should use the 1024 bit RSA key provided in as KeyBlob (v2).
    Rsa1024(Cow<'a, str>),
    /// The server should use the ed25519 v3 key provided in as KeyBlob (v3).
    Ed25519V3(Cow<'a, str>),
}
