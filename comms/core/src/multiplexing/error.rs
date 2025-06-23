<<<<<<<< HEAD:comms/core/src/multiplexing/error.rs
// Copyright 2019, The Tari Project
========
// Copyright 2025. The Tari Project
>>>>>>>> feature-dan2:base_layer/core/src/chain_storage/smt_hasher.rs
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

<<<<<<<< HEAD:comms/core/src/multiplexing/error.rs
use futures::channel::oneshot::Canceled;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use yamux::ConnectionError;

/// Error type for PeerConnection
#[derive(Debug, Error, Clone)]
pub enum YamuxControlError {
    #[error("Yamux connection error: {0}")]
    ConnectionError(String),
    #[error("Yamux connection closed")]
    ConnectionClosed,
    #[error("Yamux request send error: {0}")]
    RequestSendError(String),
    #[error("Yamux request send error: {0}")]
    RequestCanceled(#[from] Canceled),
}

impl From<ConnectionError> for YamuxControlError {
    fn from(err: ConnectionError) -> Self {
        match err {
            ConnectionError::Closed => Self::ConnectionClosed,
            _ => Self::ConnectionError(err.to_string()),
        }
    }
}

impl<T> From<SendError<T>> for YamuxControlError {
    fn from(err: SendError<T>) -> Self {
        Self::RequestSendError(err.to_string())
========
use blake2::Blake2b;
use digest::{consts::U32, FixedOutput};
use jmt::SimpleHasher;
use tari_crypto::{
    hash_domain,
    hashing::{AsFixedBytes, DomainSeparatedHasher},
};

use crate::ValidatorNodeMerkleHasherBlake256;

hash_domain!(OutputSmtHashDomain, "com.tari.base_layer.core.output_smt", 1);
pub type OutputSmtHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, OutputSmtHashDomain>;
pub struct SmtHasher {
    hasher: OutputSmtHasherBlake256,
}

impl SimpleHasher for SmtHasher {
    fn new() -> Self {
        Self {
            hasher: OutputSmtHasherBlake256::new(),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn finalize(self) -> [u8; 32] {
        self.hasher.finalize().as_fixed_bytes().expect("Hash is 32 bytes")
    }
}

pub struct ValidatorNodeJmtHasher {
    hasher: ValidatorNodeMerkleHasherBlake256,
}

impl SimpleHasher for ValidatorNodeJmtHasher {
    fn new() -> Self {
        Self {
            hasher: ValidatorNodeMerkleHasherBlake256::new(),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn finalize(self) -> [u8; 32] {
        self.hasher.finalize_fixed().into()
>>>>>>>> feature-dan2:base_layer/core/src/chain_storage/smt_hasher.rs
    }
}
