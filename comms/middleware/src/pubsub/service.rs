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

use crate::{encryption::DecryptedInboundMessage, error::MiddlewareError, pubsub::message::DomainMessage};
use derive_error::Error;
use futures::{channel::mpsc, task::Context, Future, Poll, Sink, SinkExt};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, pin::Pin, sync::Arc};
use tari_comms::message::MessageError;
use tari_pubsub::TopicPayload;
use tokio::sync::mpsc::error::SendError;
use tower::Service;

const LOG_TARGET: &'static str = "comms::middleware::pubsub";

#[derive(Debug, Error)]
pub enum PubsubError {
    DeserializationFailed(MessageError),
    SendError(SendError),
}

/// This service receives DecryptedInboundMessages, deserializes the MessageHeader and
/// sends a `TopicPayload<DomainMessage>` on the given sender.
// TODO: Can be generalized into a service which "constructs a DomainMessage<MType> and sends that on the given sink".
//       It need not know about TopicPayloads, as the receiving side of the channel could simply map to that
pub struct PubsubService<MType> {
    sender: mpsc::Sender<TopicPayload<MType, Arc<DomainMessage<MType>>>>,
}

impl<MType> Clone for PubsubService<MType> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<MType> PubsubService<MType> {
    pub fn new(sender: mpsc::Sender<TopicPayload<MType, Arc<DomainMessage<MType>>>>) -> Self {
        Self { sender }
    }
}

impl<MType> Service<DecryptedInboundMessage> for PubsubService<MType>
where MType: Serialize + DeserializeOwned + Eq + Clone
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sender).poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, msg: DecryptedInboundMessage) -> Self::Future {
        Self::publish_message(self.sender.clone(), msg)
    }
}

impl<MType> PubsubService<MType>
where MType: Serialize + DeserializeOwned + Eq + Clone
{
    async fn publish_message<TSink>(
        mut sink: TSink,
        inbound_message: DecryptedInboundMessage,
    ) -> Result<(), MiddlewareError>
    where
        TSink: Sink<TopicPayload<MType, Arc<DomainMessage<MType>>>> + Unpin,
        TSink::Error: Into<MiddlewareError> + Error + 'static,
    {
        match inbound_message.succeeded() {
            Some(message) => {
                match message.deserialize_header::<MType>() {
                    Ok(header) => {
                        let DecryptedInboundMessage {
                            source_peer,
                            envelope_header,
                            decryption_result,
                            ..
                        } = inbound_message;

                        let domain_message = DomainMessage {
                            message_header: header,
                            source_peer,
                            envelope_header,
                            message: decryption_result
                                .ok()
                                .expect("Already checked that decrypted message succeeded"),
                        };

                        // If this fails there is something wrong with the sink and the pubsub middleware should not
                        // continue
                        sink.send(TopicPayload::new(
                            domain_message.message_header.message_type.clone(),
                            Arc::new(domain_message),
                        ))
                        .await?;
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
                // Although a message which failed to decrypt/deserialize should never reach here
                // as 'forward' should have forwarded the message and stopped it from propagation up the middleware
                // we still have to handle this case (because we are accepting a DecryptedInboundMessage)
                warn!(
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
    use crate::test_utils::{make_inbound_message, make_node_identity};
    use futures::{executor::block_on, StreamExt};
    use tari_comms::message::{Message, MessageFlags, MessageHeader};
    use tari_utilities::message_format::MessageFormat;

    #[test]
    fn publish_message() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(123).unwrap();
        let msg = Message::from_message_format(header, "my message".to_string()).unwrap();
        let inbound_message =
            make_inbound_message(&make_node_identity(), msg.to_binary().unwrap(), MessageFlags::empty());
        let decrypted = DecryptedInboundMessage::succeed(msg, inbound_message);
        block_on(PubsubService::<i32>::publish_message(tx, decrypted)).unwrap();

        let payload = block_on(rx.next()).unwrap();
        assert_eq!(payload.topic(), &123);
        let domain_message = payload.message();
        assert_eq!(
            domain_message.message.deserialize_message::<String>().unwrap(),
            "my message"
        );
    }

    #[test]
    fn publish_message_fail_deserialize() {
        let (tx, mut rx) = mpsc::channel(1);
        let msg = Message::from_message_format((), "my message".to_string()).unwrap();
        let inbound_message =
            make_inbound_message(&make_node_identity(), msg.to_binary().unwrap(), MessageFlags::empty());
        let decrypted = DecryptedInboundMessage::succeed(msg, inbound_message);
        block_on(PubsubService::<i32>::publish_message(tx, decrypted)).unwrap();

        assert!(rx.try_next().unwrap().is_none());
    }

    #[test]
    fn publish_message_fail_send() {
        // Drop the receiver of the channel, this is the only reason this middleware should return an error
        // from it's call function
        let (tx, _) = mpsc::channel(1);
        let header = MessageHeader::new(123).unwrap();
        let msg = Message::from_message_format(header, "my message".to_string()).unwrap();
        let inbound_message =
            make_inbound_message(&make_node_identity(), msg.to_binary().unwrap(), MessageFlags::empty());
        let decrypted = DecryptedInboundMessage::succeed(msg, inbound_message);
        let result = block_on(PubsubService::<i32>::publish_message(tx, decrypted));
        assert!(result.is_err());
    }
}
