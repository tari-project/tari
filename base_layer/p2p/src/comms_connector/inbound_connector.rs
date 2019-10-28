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

use super::peer_message::PeerMessage;
use futures::{task::Context, Future, Poll, Sink, SinkExt};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, marker::PhantomData, pin::Pin, sync::Arc};
use tari_comms::message::MessageHeader;
use tari_comms_dht::inbound::DecryptedDhtMessage;
use tari_comms_middleware::MiddlewareError;
use tari_utilities::message_format::MessageFormat;
use tower::Service;

const LOG_TARGET: &'static str = "comms::middleware::inbound_domain_connector";

/// This service receives DecryptedInboundMessages, deserializes the MessageHeader and
/// sends a `DomainMessage<MType>` on the given sink.
#[derive(Clone)]
pub struct InboundDomainConnector<MType, TSink> {
    sink: TSink,
    _mt: PhantomData<MType>,
}

impl<MType, TSink> InboundDomainConnector<MType, TSink> {
    pub fn new(sink: TSink) -> Self {
        Self { sink, _mt: PhantomData }
    }
}

impl<MType, TSink> Service<DecryptedDhtMessage> for InboundDomainConnector<MType, TSink>
where
    MType: Serialize + DeserializeOwned + Eq,
    TSink: Sink<Arc<PeerMessage<MType>>> + Unpin + Clone,
    TSink::Error: Error + Send + Sync + 'static,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink).poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, msg: DecryptedDhtMessage) -> Self::Future {
        Self::handle_message(self.sink.clone(), msg)
    }
}

impl<MType, TSink> InboundDomainConnector<MType, TSink>
where
    MType: Serialize + DeserializeOwned + Eq,
    TSink: Sink<Arc<PeerMessage<MType>>> + Unpin,
    TSink::Error: Error + Send + Sync + 'static,
{
    async fn handle_message(mut sink: TSink, inbound_message: DecryptedDhtMessage) -> Result<(), MiddlewareError> {
        match inbound_message.success() {
            Some(envelope_body) => {
                // TODO: encoding Vec<u8> as protobuf message is a temporary hack until rest of messages are converted
                let header_bytes = envelope_body
                    .decode_part::<Vec<u8>>(0)?
                    .ok_or("failed to decode header")?;
                let msg_bytes = envelope_body
                    .decode_part::<Vec<u8>>(1)?
                    .ok_or("failed to decode body")?;
                match MessageHeader::<MType>::from_binary(&header_bytes) {
                    Ok(header) => {
                        let DecryptedDhtMessage {
                            source_peer,
                            dht_header,
                            ..
                        } = inbound_message;

                        let peer_message = PeerMessage {
                            message_header: header,
                            source_peer,
                            dht_header,
                            body: msg_bytes,
                        };

                        // If this fails there is something wrong with the sink and the pubsub middleware should not
                        // continue
                        sink.send(Arc::new(peer_message)).await.map_err(|err| Box::new(err))?;
                    },
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Pubsub middleware discarded inbound message: {}", err
                        );
                    },
                }

                Ok(())
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Pubsub middleware discarded inbound message: Message failed to decrypt."
                );
                Ok(())
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::{make_dht_inbound_message, make_node_identity};
    use futures::{channel::mpsc, executor::block_on, StreamExt};
    use tari_comms::{
        message::{MessageExt, MessageHeader},
        wrap_in_envelope_body,
    };
    use tari_comms_dht::envelope::DhtMessageFlags;
    use tari_utilities::message_format::MessageFormat;

    #[test]
    fn handle_message() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(123).unwrap();
        let msg = wrap_in_envelope_body!(
            header.to_binary().unwrap(),
            "my message".to_string().to_binary().unwrap()
        )
        .unwrap();

        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);
        block_on(InboundDomainConnector::<i32, _>::handle_message(tx, decrypted)).unwrap();

        let peer_message = block_on(rx.next()).unwrap();
        assert_eq!(peer_message.message_header.message_type, 123);
        assert_eq!(peer_message.deserialize_message::<String>().unwrap(), "my message");
    }

    #[test]
    fn handle_message_fail_deserialize() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(()).unwrap();
        let msg =
            wrap_in_envelope_body!(header.to_binary().unwrap(), "message".to_string().to_binary().unwrap()).unwrap();

        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);
        block_on(InboundDomainConnector::<i32, _>::handle_message(tx, decrypted)).unwrap();

        assert!(rx.try_next().unwrap().is_none());
    }

    #[test]
    fn handle_message_fail_send() {
        // Drop the receiver of the channel, this is the only reason this middleware should return an error
        // from it's call function
        let (tx, _) = mpsc::channel(1);
        let header = MessageHeader::new(123).unwrap();
        let msg = wrap_in_envelope_body!(
            header.to_binary().unwrap(),
            "my message".to_string().to_binary().unwrap()
        )
        .unwrap();
        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);
        let result = block_on(InboundDomainConnector::<i32, _>::handle_message(tx, decrypted));
        assert!(result.is_err());
    }
}
