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

use std::convert::{From, TryFrom};
use tari_comms::{peer_manager::Peer, types::CommsPublicKey};

/// Wrapper around a received message. Provides source peer and origin information
#[derive(Debug, Clone)]
pub struct DomainMessage<T> {
    /// The peer which sent this message
    pub source_peer: Peer,
    /// The origin of this message. This will be different from `source_peer.public_key` if
    /// this message was forwarded from another node on the network.
    pub origin_pubkey: CommsPublicKey,
    /// The domain-level message
    pub inner: T,
}

impl<T> DomainMessage<T> {
    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Converts the wrapped value of a DomainMessage to another compatible type.
    ///
    /// Note:
    /// The Rust compiler doesn't seem to be able to recognise that DomainMessage<T> != DomainMessage<U>, so a blanket
    /// `From` implementation isn't possible at this time
    pub fn convert<U>(self) -> DomainMessage<U>
    where U: From<T> {
        let inner = U::from(self.inner);
        DomainMessage {
            origin_pubkey: self.origin_pubkey,
            source_peer: self.source_peer,
            inner,
        }
    }

    /// Converts the wrapped value of a DomainMessage to another compatible type.
    ///
    /// Note:
    /// The Rust compiler doesn't seem to be able to recognise that DomainMessage<T> != DomainMessage<U>, so a blanket
    /// `From` implementation isn't possible at this time
    pub fn try_convert<U>(self) -> Result<DomainMessage<U>, U::Error>
    where U: TryFrom<T> {
        let inner = U::try_from(self.inner)?;
        Ok(DomainMessage {
            origin_pubkey: self.origin_pubkey,
            source_peer: self.source_peer,
            inner,
        })
    }
}
