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
use futures::{task::Context, Future, Sink, SinkExt};
use std::{error::Error, pin::Pin, sync::Arc, task::Poll};
use tari_comms::pipeline::PipelineError;
use tari_comms_dht::{domain_message::MessageHeader, inbound::DecryptedDhtMessage};
use tower::Service;

/// This service receives DecryptedDhtMessage, deserializes the MessageHeader and
/// sends a `PeerMessage` on the given sink.
#[derive(Clone)]
pub struct InboundDomainConnector<TSink> {
    sink: TSink,
}

impl<TSink> InboundDomainConnector<TSink> {
    pub fn new(sink: TSink) -> Self {
        Self { sink }
    }
}

impl<TSink> Service<DecryptedDhtMessage> for InboundDomainConnector<TSink>
where
    TSink: Sink<Arc<PeerMessage>> + Unpin + Clone,
    TSink::Error: Error + Send + Sync + 'static,
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink)
            .poll_ready(cx)
            .map_err(PipelineError::from_debug)
    }

    fn call(&mut self, msg: DecryptedDhtMessage) -> Self::Future {
        let mut sink = self.sink.clone();
        async move {
            let peer_message = Self::to_peer_message(msg)?;
            // If this fails there is something wrong with the sink and the pubsub middleware should not
            // continue
            sink.send(Arc::new(peer_message))
                .await
                .map_err(PipelineError::from_debug)?;

            Ok(())
        }
    }
}

impl<TSink> InboundDomainConnector<TSink> {
    fn to_peer_message(mut inbound_message: DecryptedDhtMessage) -> Result<PeerMessage, PipelineError> {
        let envelope_body = inbound_message
            .success_mut()
            .ok_or_else(|| "Message failed to decrypt")?;
        let header = envelope_body
            .decode_part::<MessageHeader>(0)
            .map_err(PipelineError::from_debug)?
            .ok_or_else(|| "envelope body did not contain a header")?;

        let msg_bytes = envelope_body
            .take_part(1)
            .ok_or_else(|| "envelope body did not contain a message body")?;

        let DecryptedDhtMessage {
            source_peer,
            dht_header,
            ..
        } = inbound_message;

        let peer_message = PeerMessage {
            message_header: header,
            source_peer: Clone::clone(&*source_peer),
            dht_header,
            body: msg_bytes,
        };

        Ok(peer_message)
    }
}

impl<TSink> Sink<DecryptedDhtMessage> for InboundDomainConnector<TSink>
where
    TSink: Sink<Arc<PeerMessage>> + Unpin,
    TSink::Error: Error + Send + Sync + 'static,
{
    type Error = PipelineError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink)
            .poll_ready(cx)
            .map_err(PipelineError::from_debug)
    }

    fn start_send(mut self: Pin<&mut Self>, item: DecryptedDhtMessage) -> Result<(), Self::Error> {
        let item = Self::to_peer_message(item)?;
        Pin::new(&mut self.sink)
            .start_send(Arc::new(item))
            .map_err(PipelineError::from_debug)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink)
            .poll_flush(cx)
            .map_err(PipelineError::from_debug)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.sink)
            .poll_close(cx)
            .map_err(PipelineError::from_debug)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::{make_dht_inbound_message, make_node_identity};
    use futures::{channel::mpsc, executor::block_on, StreamExt};
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tari_comms_dht::{domain_message::MessageHeader, envelope::DhtMessageFlags};
    use tower::ServiceExt;

    #[tokio_macros::test_basic]
    async fn handle_message() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(123);
        let msg = wrap_in_envelope_body!(header, b"my message".to_vec()).unwrap();

        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);
        InboundDomainConnector::new(tx).oneshot(decrypted).await.unwrap();

        let peer_message = block_on(rx.next()).unwrap();
        assert_eq!(peer_message.message_header.message_type, 123);
        assert_eq!(peer_message.decode_message::<String>().unwrap(), "my message");
    }

    #[tokio_macros::test_basic]
    async fn send_on_sink() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(123);
        let msg = wrap_in_envelope_body!(header, b"my message".to_vec()).unwrap();

        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);

        InboundDomainConnector::new(tx).send(decrypted).await.unwrap();

        let peer_message = block_on(rx.next()).unwrap();
        assert_eq!(peer_message.message_header.message_type, 123);
        assert_eq!(peer_message.decode_message::<String>().unwrap(), "my message");
    }

    #[tokio_macros::test_basic]
    async fn handle_message_fail_deserialize() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = b"dodgy header".to_vec();
        let msg = wrap_in_envelope_body!(header, b"message".to_vec()).unwrap();

        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);
        InboundDomainConnector::new(tx).oneshot(decrypted).await.unwrap_err();

        assert!(rx.try_next().unwrap().is_none());
    }

    #[tokio_macros::test_basic]
    async fn handle_message_fail_send() {
        // Drop the receiver of the channel, this is the only reason this middleware should return an error
        // from it's call function
        let (tx, _) = mpsc::channel(1);
        let header = MessageHeader::new(123);
        let msg = wrap_in_envelope_body!(header, b"my message".to_vec()).unwrap();
        let inbound_message = make_dht_inbound_message(
            &make_node_identity(),
            msg.to_encoded_bytes().unwrap(),
            DhtMessageFlags::empty(),
        );
        let decrypted = DecryptedDhtMessage::succeeded(msg, inbound_message);
        let result = InboundDomainConnector::new(tx).oneshot(decrypted).await;
        assert!(result.is_err());
    }
}
