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
    services::comms_outbound::{
        error::CommsOutboundServiceError,
        messages::{CommsOutboundRequest, CommsOutboundResponse},
    },
    tari_message::TariMessageType,
};
use tari_comms::{
    message::{Frame, Message, MessageEnvelope, MessageFlags, MessageHeader},
    outbound_message_service::BroadcastStrategy,
};
use tari_service_framework::{reply_channel::TransportChannelError, tower::ServiceExt};
use tari_utilities::message_format::MessageFormat;
use tower_service::Service;

/// Handle for the CommsOutboundService.
#[derive(Clone)]
pub struct CommsOutboundHandle<S> {
    service: S,
}

impl<S> CommsOutboundHandle<S>
where
    S: Service<
            CommsOutboundRequest,
            Response = Result<CommsOutboundResponse, CommsOutboundServiceError>,
            Error = TransportChannelError,
        > + Unpin,
    S::Future: Unpin,
{
    /// Create a new CommsOutboundHandle, which makes requests using the
    /// given Requester
    pub fn new(service: S) -> Self {
        Self { service }
    }

    /// Send a comms message
    pub async fn send_message<T>(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        flags: MessageFlags,
        message_type: TariMessageType,
        message: T,
    ) -> Result<CommsOutboundResponse, CommsOutboundServiceError>
    where
        T: MessageFormat,
    {
        let frame = serialize_message(message_type, message)?;
        self
            .service
            .call_ready(CommsOutboundRequest::SendMsg {
                broadcast_strategy,
                flags,
                body: Box::new(frame),
            })
            .await
             // Convert the transport channel error into the local error
            .unwrap_or_else(|err| Err(err.into()))
    }

    /// Forward a comms message
    pub async fn forward_message(
        mut self,
        broadcast_strategy: BroadcastStrategy,
        envelope: MessageEnvelope,
    ) -> Result<CommsOutboundResponse, CommsOutboundServiceError>
    {
        self.service
            .call_ready(CommsOutboundRequest::Forward {
                broadcast_strategy,
                message_envelope: Box::new(envelope),
            })
            .await
            // Convert the transport channel error into the local error
            .unwrap_or_else(|err| Err(err.into()))
    }
}

fn serialize_message<T>(message_type: TariMessageType, message: T) -> Result<Frame, CommsOutboundServiceError>
where T: MessageFormat {
    let header = MessageHeader::new(message_type)?;
    let msg = Message::from_message_format(header, message)?;

    msg.to_binary().map_err(Into::into)
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{executor::block_on, future};
    use rand::rngs::OsRng;
    use tari_comms::{
        message::{MessageEnvelopeHeader, NodeDestination},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::tower::service_fn;

    #[test]
    fn serialize_message() {
        let message_type = TariMessageType::new(0);
        let message = "FOO".to_string();
        let body = super::serialize_message(message_type.clone(), message).unwrap();

        let msg = Message::from_binary(&body).unwrap();
        let header: MessageHeader<TariMessageType> = msg.deserialize_header().unwrap();
        let body_msg: String = msg.deserialize_message().unwrap();
        assert_eq!(header.message_type, message_type);
        assert_eq!(body_msg, "FOO");
    }

    #[test]
    fn send_message() {
        let service = service_fn(|req| {
            match req {
                CommsOutboundRequest::SendMsg { .. } => {},
                _ => panic!("Unexpected request"),
            }
            future::ok(Ok(()))
        });

        let mut handle = CommsOutboundHandle::new(service);

        block_on(async move {
            handle
                .send_message(
                    BroadcastStrategy::Flood,
                    MessageFlags::empty(),
                    TariMessageType::new(0),
                    "FOO".to_string(),
                )
                .await
                .unwrap();
        });
    }

    #[test]
    fn forward() {
        let service = service_fn(|req| {
            match req {
                CommsOutboundRequest::Forward { .. } => {},
                _ => panic!("Unexpected request"),
            }
            future::ok(Ok(()))
        });

        let handle = CommsOutboundHandle::new(service);
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

        block_on(async move {
            handle
                .forward_message(
                    BroadcastStrategy::Flood,
                    MessageEnvelope::new(vec![0], header.to_binary().unwrap(), vec![]),
                )
                .await
                .unwrap();
        });
    }
}
