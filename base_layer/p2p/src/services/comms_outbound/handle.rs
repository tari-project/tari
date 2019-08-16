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

use crate::{
    executor::transport::{AwaitResponseError, Requester},
    services::comms_outbound::{
        error::CommsOutboundServiceError,
        messages::{CommsOutboundRequest, CommsOutboundResponse},
    },
    tari_message::TariMessageType,
};
use futures::{
    future::{self, Either, Future},
    Poll,
};
use tari_comms::{
    message::{Frame, Message, MessageEnvelope, MessageFlags, MessageHeader},
    outbound_message_service::BroadcastStrategy,
};
use tari_utilities::message_format::MessageFormat;
use tokio_threadpool::{blocking, BlockingError};
use tower_service::Service;

type CommsOutboundRequester = Requester<CommsOutboundRequest, Result<CommsOutboundResponse, CommsOutboundServiceError>>;

/// Handle for the CommsOutboundService.
#[derive(Clone)]
pub struct CommsOutboundHandle {
    requester: CommsOutboundRequester,
}

impl CommsOutboundHandle {
    /// Create a new CommsOutboundHandle, which makes requests using the
    /// given Requester
    pub fn new(requester: CommsOutboundRequester) -> Self {
        Self { requester }
    }

    /// Send a comms message
    pub fn send_message<T>(
        mut self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message_type: TariMessageType,
        message: T,
    ) -> impl Future<Item = Result<CommsOutboundResponse, CommsOutboundServiceError>, Error = AwaitResponseError> + 'static
    where
        T: MessageFormat + 'static,
    {
        Self::message_body_serializer(message_type, message)
            .or_else(|err| future::ok(Err(CommsOutboundServiceError::BlockingError(err))))
            .and_then(move |res| match res {
                Ok(body) => Either::A(self.requester.call(CommsOutboundRequest::SendMsg {
                    broadcast_strategy,
                    flags,
                    body: Box::new(body),
                })),
                Err(err) => Either::B(future::ok(Err(err))),
            })
    }

    /// Forward a comms message
    pub fn forward_message(
        mut self,
        broadcast_strategy: BroadcastStrategy,
        envelope: MessageEnvelope,
    ) -> impl Future<Item = Result<CommsOutboundResponse, CommsOutboundServiceError>, Error = AwaitResponseError>
    {
        self.requester.call(CommsOutboundRequest::Forward {
            broadcast_strategy,
            message_envelope: Box::new(envelope),
        })
    }

    /// Return a message body serializer future
    fn message_body_serializer<T>(message_type: TariMessageType, message: T) -> MessageBodySerializer<T>
    where T: MessageFormat {
        MessageBodySerializer::new(message_type, message)
    }
}

#[must_use = "futures do nothing unless polled"]
struct MessageBodySerializer<T> {
    message: Option<T>,
    message_type: Option<TariMessageType>,
}

impl<T> MessageBodySerializer<T> {
    fn new(message_type: TariMessageType, message: T) -> Self {
        Self {
            message: Some(message),
            message_type: Some(message_type),
        }
    }
}

impl<T> Future for MessageBodySerializer<T>
where T: MessageFormat
{
    type Error = BlockingError;
    type Item = Result<Frame, CommsOutboundServiceError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let message_type = self.message_type.take().expect("called poll twice");
        let message = self.message.take().expect("called poll twice");
        blocking(move || {
            let header =
                MessageHeader::new(message_type).map_err(CommsOutboundServiceError::MessageSerializationError)?;
            let msg = Message::from_message_format(header, message)
                .map_err(CommsOutboundServiceError::MessageSerializationError)?;

            msg.to_binary().map_err(CommsOutboundServiceError::MessageFormatError)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::executor::transport;
    use rand::rngs::OsRng;
    use tari_comms::{
        message::{MessageEnvelopeHeader, NodeDestination},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tokio::runtime::Runtime;
    use tower_util::service_fn;

    #[test]
    fn message_body_serializer() {
        // Require tokio threadpool for blocking call
        let mut rt = Runtime::new().unwrap();

        let message_type = TariMessageType::new(0);
        let message = "FOO".to_string();
        let fut = CommsOutboundHandle::message_body_serializer(message_type.clone(), message);

        let body = rt.block_on(fut).unwrap().unwrap();

        let msg = Message::from_binary(&body).unwrap();
        let header: MessageHeader<TariMessageType> = msg.deserialize_header().unwrap();
        let body_msg: String = msg.deserialize_message().unwrap();
        assert_eq!(header.message_type, message_type);
        assert_eq!(body_msg, "FOO");
    }

    #[test]
    fn send_message() {
        let mut rt = Runtime::new().unwrap();

        let (req, res) = transport::channel(service_fn(|req| {
            match req {
                CommsOutboundRequest::SendMsg { .. } => {},
                _ => panic!("Unexpected request"),
            }
            future::ok::<_, ()>(Ok(()))
        }));

        rt.spawn(res);

        let handle = CommsOutboundHandle::new(req);
        let fut = handle.send_message(
            BroadcastStrategy::Flood,
            MessageFlags::empty(),
            TariMessageType::new(0),
            "FOO".to_string(),
        );

        rt.block_on(fut).unwrap().unwrap();
    }

    #[test]
    fn forward() {
        let mut rt = Runtime::new().unwrap();

        let (req, res) = transport::channel(service_fn(|req| {
            match req {
                CommsOutboundRequest::Forward { .. } => {},
                _ => panic!("Unexpected request"),
            }
            future::ok::<_, ()>(Ok(()))
        }));

        rt.spawn(res);

        let handle = CommsOutboundHandle::new(req);
        let mut rng = OsRng::new().unwrap();
        let header = MessageEnvelopeHeader {
            version: 0,
            origin_source: CommsPublicKey::random_keypair(&mut rng).1,
            peer_source: CommsPublicKey::random_keypair(&mut rng).1,
            dest: NodeDestination::Unknown,
            origin_signature: vec![],
            peer_signature: vec![],
            flags: MessageFlags::empty(),
        };

        let fut = handle.forward_message(
            BroadcastStrategy::Flood,
            MessageEnvelope::new(vec![0], header.to_binary().unwrap(), vec![]),
        );

        rt.block_on(fut).unwrap().unwrap();
    }
}
