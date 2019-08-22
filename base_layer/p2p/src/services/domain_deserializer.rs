// Copyright 2019 The Tari Project
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

use futures::{Future, Poll};
use serde::export::PhantomData;
use tari_comms::{
    domain_subscriber::MessageInfo,
    message::{DomainMessageContext, MessageError},
};
use tari_utilities::message_format::MessageFormat;
use tokio_threadpool::{blocking, BlockingError};

/// Future which asynchonously attempts to deserialize DomainMessageContext into
/// a `(MessageInfo, T)` tuple where T is [MessageFormat].
pub struct DomainMessageDeserializer<T> {
    message: Option<DomainMessageContext>,
    _t: PhantomData<T>,
}

impl<T> DomainMessageDeserializer<T> {
    /// Create a new DomainMessageDeserializer from the given DomainMessageContext
    pub fn new(message: DomainMessageContext) -> Self {
        Self {
            message: Some(message),
            _t: PhantomData,
        }
    }
}

impl<T: MessageFormat> Future for DomainMessageDeserializer<T> {
    type Error = BlockingError;
    type Item = Result<(MessageInfo, T), MessageError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let msg = self.message.take().expect("poll called twice on Deserializer");
        blocking(|| {
            let deserialized: T = msg.message.deserialize_message()?;
            let info = MessageInfo {
                peer_source: msg.peer_source,
                origin_source: msg.origin_source,
            };
            Ok((info, deserialized))
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::rngs::OsRng;
    use tari_comms::{
        message::{Message, MessageHeader},
        peer_manager::{NodeId, PeerNodeIdentity},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tokio::runtime::Runtime;

    fn create_domain_message<T: MessageFormat>(message_type: u8, inner_msg: T) -> DomainMessageContext {
        let mut rng = OsRng::new().unwrap();
        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let peer_source = PeerNodeIdentity::new(NodeId::from_key(&pk).unwrap(), pk.clone());
        let header = MessageHeader::new(message_type).unwrap();
        let msg = Message::from_message_format(header, inner_msg).unwrap();
        DomainMessageContext::new(peer_source, pk, msg)
    }

    #[test]
    fn deserialize_success() {
        let mut rt = Runtime::new().unwrap();
        let domain_msg = create_domain_message(1, "wubalubadubdub".to_string());
        let fut = DomainMessageDeserializer::<String>::new(domain_msg.clone());

        let (info, msg) = rt.block_on(fut).unwrap().unwrap();
        assert_eq!(msg, "wubalubadubdub");
        assert_eq!(info.peer_source, domain_msg.peer_source);
        assert_eq!(info.origin_source, domain_msg.origin_source);
    }

    #[test]
    fn deserialize_fail() {
        let mut rt = Runtime::new().unwrap();
        let domain_msg = create_domain_message(1, "wubalubadubdub".to_string());
        let fut = DomainMessageDeserializer::<bool>::new(domain_msg.clone());

        match rt.block_on(fut).unwrap() {
            Ok(_) => panic!("unexpected success when deserializing to mismatched type"),
            Err(MessageError::MessageFormatError(_)) => {},
            Err(err) => panic!("unexpected error when deserializing mismatched types: {:?}", err),
        }
    }
}
