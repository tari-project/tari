//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::task::Poll;

use futures::{future::BoxFuture, task::Context};
use log::*;
use prost::bytes::BufMut;
use tari_comms::{peer_manager::Peer, pipeline::PipelineError, BytesMut};
use tari_utilities::epoch_time::EpochTime;
use tower::{layer::Layer, Service, ServiceExt};

use crate::{
    envelope::NodeDestination,
    inbound::{error::DhtInboundError, DecryptedDhtMessage},
    outbound::{OutboundMessageRequester, SendMessageParams},
};

const LOG_TARGET: &str = "comms::dht::storeforward::forward";

/// This layer is responsible for forwarding messages which have failed to decrypt
pub struct ForwardLayer {
    outbound_service: OutboundMessageRequester,
    is_enabled: bool,
}

impl ForwardLayer {
    pub fn new(outbound_service: OutboundMessageRequester, is_enabled: bool) -> Self {
        Self {
            outbound_service,
            is_enabled,
        }
    }
}

impl<S> Layer<S> for ForwardLayer {
    type Service = ForwardMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        ForwardMiddleware::new(
            service,
            // Pass in just the config item needed by the middleware for almost free copies
            self.outbound_service.clone(),
            self.is_enabled,
        )
    }
}

/// # Forward middleware
///
/// Responsible for forwarding messages which fail to decrypt.
#[derive(Clone)]
pub struct ForwardMiddleware<S> {
    next_service: S,
    outbound_service: OutboundMessageRequester,
    is_enabled: bool,
}

impl<S> ForwardMiddleware<S> {
    pub fn new(service: S, outbound_service: OutboundMessageRequester, is_enabled: bool) -> Self {
        Self {
            next_service: service,
            outbound_service,
            is_enabled,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for ForwardMiddleware<S>
where
    S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: DecryptedDhtMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let outbound_service = self.outbound_service.clone();
        let is_enabled = self.is_enabled;
        Box::pin(async move {
            if !is_enabled {
                trace!(
                    target: LOG_TARGET,
                    "Passing message {} to next service (Not enabled) (Trace: {})",
                    message.tag,
                    message.dht_header.message_tag
                );
                return next_service.oneshot(message).await;
            }

            trace!(
                target: LOG_TARGET,
                "Passing message {} to next service (Trace: {})",
                message.tag,
                message.dht_header.message_tag
            );
            let forwarder = Forwarder::new(next_service, outbound_service);
            forwarder.handle(message).await
        })
    }
}

/// Responsible for processing a single DecryptedDhtMessage, forwarding if necessary or passing the message
/// to the next service.
struct Forwarder<S> {
    next_service: S,
    outbound_service: OutboundMessageRequester,
}

impl<S> Forwarder<S> {
    pub fn new(service: S, outbound_service: OutboundMessageRequester) -> Self {
        Self {
            next_service: service,
            outbound_service,
        }
    }
}

impl<S> Forwarder<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    async fn handle(mut self, message: DecryptedDhtMessage) -> Result<(), PipelineError> {
        if message.decryption_failed() {
            trace!(
                target: LOG_TARGET,
                "Decryption failed. Forwarding message {} (Trace: {})",
                message.tag,
                message.dht_header.message_tag
            );
            self.forward(&message).await?;
        }

        // The message has been forwarded, but downstream middleware may be interested
        trace!(
            target: LOG_TARGET,
            "Passing message {} to next service (Trace: {})",
            message.tag,
            message.dht_header.message_tag
        );
        self.next_service.oneshot(message).await?;
        Ok(())
    }

    async fn forward(&mut self, message: &DecryptedDhtMessage) -> Result<(), DhtInboundError> {
        let DecryptedDhtMessage {
            source_peer,
            decryption_result,
            dht_header,
            is_saf_stored,
            is_already_forwarded,
            ..
        } = message;

        if self.destination_matches_source(&dht_header.destination, source_peer) {
            //       #banheuristic - the origin of this message was the destination. Two things are wrong here:
            //       1. The origin/destination should not have forwarded this (the destination node didnt do this
            //          destination_matches_source check)
            //       1. The source sent a message that the destination could not decrypt
            //       The authenticated source should be banned (malicious), and origin should be temporarily banned
            //       (bug?)
            debug!(
                target: LOG_TARGET,
                "Received message {} from peer '{}' that is destined for that peer. Discarding message (Trace: {})",
                message.tag,
                source_peer.node_id.short_str(),
                message.dht_header.message_tag
            );
            return Ok(());
        }

        if let Some(expires) = &dht_header.expires {
            if *expires < EpochTime::now() {
                debug!(
                    target: LOG_TARGET,
                    "Received message {} from peer '{}' that is expired. Discarding message (Trace: {})",
                    message.tag,
                    source_peer.node_id.short_str(),
                    message.dht_header.message_tag
                );
                return Ok(());
            }
        }
        let err_body = decryption_result
            .as_ref()
            .expect_err("previous check that decryption failed");
        let mut body = BytesMut::with_capacity(err_body.len());
        body.put(err_body.as_slice());

        let excluded_peers = vec![source_peer.node_id.clone()];
        let dest_node_id = dht_header.destination.to_derived_node_id();

        let mut send_params = SendMessageParams::new();
        match (dest_node_id, is_saf_stored) {
            (Some(node_id), Some(true)) => {
                let debug_info = format!(
                    "Forwarding SAF message directly to node: {}, {}",
                    node_id, dht_header.message_tag
                );
                debug!(target: LOG_TARGET, "{}", &debug_info);
                send_params.with_debug_info(debug_info);
                send_params.direct_or_closest_connected(node_id, excluded_peers);
            },
            _ => {
                let debug_info = format!(
                    "Propagating SAF message for {}, propagating it. {}",
                    dht_header.destination, dht_header.message_tag
                );
                debug!(target: LOG_TARGET, "{}", debug_info);
                send_params.with_debug_info(debug_info);
                send_params.propagate(dht_header.destination.clone(), excluded_peers);
            },
        };

        if !is_already_forwarded {
            send_params.with_dht_header(dht_header.clone());
            self.outbound_service
                .send_raw_no_wait(send_params.finish(), body)
                .await?;
        }

        Ok(())
    }

    fn destination_matches_source(&self, destination: &NodeDestination, source: &Peer) -> bool {
        if let Some(pk) = destination.public_key() {
            return pk == &source.public_key;
        }

        false
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tokio::{sync::mpsc, task};

    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        outbound::mock::create_outbound_service_mock,
        test_utils::{make_dht_inbound_message, make_node_identity, service_spy},
    };

    #[tokio::test]
    async fn decryption_succeeded() {
        let spy = service_spy();
        let (oms_tx, _) = mpsc::channel(1);
        let oms = OutboundMessageRequester::new(oms_tx);
        let mut service = ForwardLayer::new(oms, true).layer(spy.to_service::<PipelineError>());

        let node_identity = make_node_identity();
        let inbound_msg =
            make_dht_inbound_message(&node_identity, &b"".to_vec(), DhtMessageFlags::empty(), false, false).unwrap();
        let msg = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(Vec::new()),
            Some(node_identity.public_key().clone()),
            inbound_msg,
        );
        service.call(msg).await.unwrap();
        assert!(spy.is_called());
    }

    #[tokio::test]
    async fn decryption_failed() {
        let spy = service_spy();
        let (oms_requester, oms_mock) = create_outbound_service_mock(1);
        let oms_mock_state = oms_mock.get_state();
        task::spawn(oms_mock.run());

        let mut service = ForwardLayer::new(oms_requester, true).layer(spy.to_service::<PipelineError>());

        let sample_body = b"Lorem ipsum";
        let inbound_msg = make_dht_inbound_message(
            &make_node_identity(),
            &sample_body.to_vec(),
            DhtMessageFlags::empty(),
            false,
            false,
        )
        .unwrap();
        let header = inbound_msg.dht_header.clone();
        let msg = DecryptedDhtMessage::failed(inbound_msg);
        service.call(msg).await.unwrap();
        assert!(spy.is_called());

        oms_mock_state
            .wait_call_count(1, Duration::from_secs(10))
            .await
            .unwrap();
        let (params, body) = oms_mock_state.pop_call().await.unwrap();

        // Header and body are preserved when forwarding
        assert_eq!(&body.to_vec(), &sample_body.to_vec().to_encoded_bytes());
        assert_eq!(params.dht_header.unwrap(), header);
    }
}
