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

use std::{pin::Pin, sync::Arc, task::Poll};

use anyhow::anyhow;
use futures::{task::Context, Future};
use log::*;
use tari_comms::pipeline::PipelineError;
use tari_comms_dht::{domain_message::MessageHeader, inbound::DecryptedDhtMessage};
use tokio::sync::mpsc;
use tower::Service;

use super::peer_message::PeerMessage;

const LOG_TARGET: &str = "comms::middleware::inbound_connector";
/// This service receives DecryptedDhtMessage, deserializes the MessageHeader and
/// sends a `PeerMessage` on the given sink.
#[derive(Clone)]
pub struct InboundDomainConnector {
    sink: mpsc::Sender<Arc<PeerMessage>>,
}

impl InboundDomainConnector {
    pub fn new(sink: mpsc::Sender<Arc<PeerMessage>>) -> Self {
        Self { sink }
    }
}

impl Service<DecryptedDhtMessage> for InboundDomainConnector {
    type Error = PipelineError;
    type Future = Pin<Box<dyn Future<Output = Result<(), PipelineError>> + Send>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DecryptedDhtMessage) -> Self::Future {
        let sink = self.sink.clone();
        let future = async move {
            let peer_message = Self::construct_peer_message(msg)?;
            // If this fails the channel has closed and the pubsub middleware should not
            // continue
            sink.send(Arc::new(peer_message)).await?;

            Ok(())
        };
        Box::pin(future)
    }
}

impl InboundDomainConnector {
    fn construct_peer_message(mut inbound_message: DecryptedDhtMessage) -> Result<PeerMessage, PipelineError> {
        let envelope_body = inbound_message
            .success_mut()
            .ok_or_else(|| anyhow!("Message failed to decrypt"))?;
        let header = envelope_body
            .decode_part::<MessageHeader>(0)?
            .ok_or_else(|| anyhow!("envelope body did not contain a header"))?;

        let msg_bytes = envelope_body
            .take_part(1)
            .ok_or_else(|| anyhow!("envelope body did not contain a message body"))?;

        let DecryptedDhtMessage {
            source_peer,
            dht_header,
            authenticated_origin,
            ..
        } = inbound_message;

        let peer_message = PeerMessage {
            message_header: header,
            source_peer: Clone::clone(&*source_peer),
            authenticated_origin,
            dht_header,
            body: msg_bytes,
        };
        trace!(
            target: LOG_TARGET,
            "Forwarding message {:?} to pubsub, Trace: {}",
            inbound_message.tag,
            &peer_message.dht_header.message_tag
        );
        Ok(peer_message)
    }
}

#[cfg(test)]
mod test {
    use futures::executor::block_on;
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tower::ServiceExt;

    use super::*;
    use crate::test_utils::{make_dht_inbound_message, make_node_identity};

    #[tokio::test]
    async fn handle_message() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(123);
        let msg = wrap_in_envelope_body!(header, b"my message".to_vec());

        let inbound_message = make_dht_inbound_message(&make_node_identity(), msg.encode_to_vec());
        let decrypted = DecryptedDhtMessage::succeeded(msg, None, inbound_message);
        InboundDomainConnector::new(tx).oneshot(decrypted).await.unwrap();

        let peer_message = block_on(rx.recv()).unwrap();
        assert_eq!(peer_message.message_header.message_type, 123);
        assert_eq!(peer_message.decode_message::<String>().unwrap(), "my message");
    }

    #[tokio::test]
    async fn send_on_sink() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = MessageHeader::new(123);
        let msg = wrap_in_envelope_body!(header, b"my message".to_vec());

        let inbound_message = make_dht_inbound_message(&make_node_identity(), msg.encode_to_vec());
        let decrypted = DecryptedDhtMessage::succeeded(msg, None, inbound_message);

        InboundDomainConnector::new(tx).call(decrypted).await.unwrap();

        let peer_message = block_on(rx.recv()).unwrap();
        assert_eq!(peer_message.message_header.message_type, 123);
        assert_eq!(peer_message.decode_message::<String>().unwrap(), "my message");
    }

    #[tokio::test]
    async fn handle_message_fail_deserialize() {
        let (tx, mut rx) = mpsc::channel(1);
        let header = b"dodgy header".to_vec();
        let msg = wrap_in_envelope_body!(header, b"message".to_vec());

        let inbound_message = make_dht_inbound_message(&make_node_identity(), msg.encode_to_vec());
        let decrypted = DecryptedDhtMessage::succeeded(msg, None, inbound_message);
        InboundDomainConnector::new(tx).oneshot(decrypted).await.unwrap_err();

        rx.close();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn handle_message_fail_send() {
        // Drop the receiver of the channel, this is the only reason this middleware should return an error
        // from it's call function
        let (tx, _) = mpsc::channel(1);
        let header = MessageHeader::new(123);
        let msg = wrap_in_envelope_body!(header, b"my message".to_vec());
        let inbound_message = make_dht_inbound_message(&make_node_identity(), msg.encode_to_vec());
        let decrypted = DecryptedDhtMessage::succeeded(msg, None, inbound_message);
        let result = InboundDomainConnector::new(tx).oneshot(decrypted).await;
        assert!(result.is_err());
    }
}
