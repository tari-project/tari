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

use super::{error::LivenessError, state::LivenessState, LivenessRequest, LivenessResponse, LOG_TARGET};
use crate::{
    domain_message::DomainMessage,
    proto::{
        liveness::{PingPong, PingPongMessage},
        TariMessageType,
    },
    services::liveness::handle::LivenessEvent,
};
use futures::{pin_mut, stream::StreamExt, SinkExt, Stream};
use log::*;
use std::sync::Arc;
use tari_broadcast_channel::Publisher;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester},
};
use tari_service_framework::RequestContext;

/// Service responsible for testing Liveness for Peers.
///
/// Very basic global ping and pong counter stats are implemented. In future,
/// peer latency and availability stats will be added.
pub struct LivenessService<THandleStream, TPingStream> {
    request_rx: Option<THandleStream>,
    ping_stream: Option<TPingStream>,
    state: Arc<LivenessState>,
    oms_handle: OutboundMessageRequester,
    event_publisher: Publisher<LivenessEvent>,
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream> {
    pub fn new(
        request_rx: THandleStream,
        ping_stream: TPingStream,
        state: Arc<LivenessState>,
        oms_handle: OutboundMessageRequester,
        event_publisher: Publisher<LivenessEvent>,
    ) -> Self
    {
        Self {
            request_rx: Some(request_rx),
            ping_stream: Some(ping_stream),
            state,
            oms_handle,
            event_publisher,
        }
    }
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream>
where
    TPingStream: Stream<Item = DomainMessage<PingPongMessage>>,
    THandleStream: Stream<Item = RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>>,
{
    pub async fn run(mut self) {
        let ping_stream = self.ping_stream.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(ping_stream);

        let request_stream = self.request_rx.take().expect("ping_stream cannot be None").fuse();
        pin_mut!(request_stream);
        loop {
            futures::select! {
                // Requests from the handle
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming messages from the Comms layer
                msg = ping_stream.select_next_some() => {
                    let _ = self.handle_incoming_message(msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming PingPong message: {:?}", err);
                        Err(err)
                    });
                },
                complete => {
                    info!(target: LOG_TARGET, "Liveness service shutting down because all streams finished");
                    break;
                }
            }
        }
    }

    async fn handle_incoming_message(&mut self, msg: DomainMessage<PingPongMessage>) -> Result<(), LivenessError> {
        match msg.inner().kind().ok_or(LivenessError::InvalidPingPongType)? {
            PingPong::Ping => {
                self.state.inc_pings_received();
                self.send_pong(msg.origin_pubkey).await.unwrap();
                self.state.inc_pongs_sent();
                self.event_publisher
                    .send(LivenessEvent::ReceivedPing)
                    .await
                    .map_err(|_| LivenessError::EventStreamError)?;
            },
            PingPong::Pong => {
                self.state.inc_pongs_received();
                self.event_publisher
                    .send(LivenessEvent::ReceivedPong)
                    .await
                    .map_err(|_| LivenessError::EventStreamError)?;
            },
        }
        Ok(())
    }

    async fn send_pong(&mut self, dest: CommsPublicKey) -> Result<(), LivenessError> {
        self.oms_handle
            .send_direct(
                dest,
                OutboundEncryption::EncryptForDestination,
                OutboundDomainMessage::new(TariMessageType::PingPong, PingPongMessage::pong()),
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn handle_request(&mut self, request: LivenessRequest) -> Result<LivenessResponse, LivenessError> {
        match request {
            LivenessRequest::SendPing(pub_key) => {
                self.send_ping(pub_key).await?;
                self.state.inc_pings_sent();
                Ok(LivenessResponse::PingSent)
            },
            LivenessRequest::GetPingCount => {
                let ping_count = self.get_ping_count();
                Ok(LivenessResponse::Count(ping_count))
            },
            LivenessRequest::GetPongCount => {
                let pong_count = self.get_pong_count();
                Ok(LivenessResponse::Count(pong_count))
            },
        }
    }

    async fn send_ping(&mut self, pub_key: CommsPublicKey) -> Result<(), LivenessError> {
        self.oms_handle
            .send_direct(
                pub_key.clone(),
                OutboundEncryption::EncryptForDestination,
                OutboundDomainMessage::new(TariMessageType::PingPong, PingPongMessage::ping()),
            )
            .await
            .map_err(Into::<DhtOutboundError>::into)?;

        Ok(())
    }

    fn get_ping_count(&self) -> usize {
        self.state.pings_received()
    }

    fn get_pong_count(&self) -> usize {
        self.state.pongs_received()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::services::liveness::handle::LivenessHandle;
    use futures::{channel::mpsc, executor::LocalPool, stream, task::SpawnExt};
    use rand::rngs::OsRng;
    use std::sync::atomic::AtomicBool;
    use tari_broadcast_channel::bounded;
    use tari_comms::{
        connection::NetAddress,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    };
    use tari_comms_dht::outbound::DhtOutboundRequest;
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::reply_channel;
    use tari_test_utils::runtime;

    #[test]
    fn get_ping_pong_count() {
        let state = Arc::new(LivenessState::new());
        state.inc_pings_received();
        state.inc_pongs_received();
        state.inc_pongs_received();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, _) = mpsc::channel(10);
        let oms_handle = OutboundMessageRequester::new(outbound_tx);

        // Setup liveness service
        let (sender_service, receiver) = reply_channel::unbounded();
        let (publisher, subscriber) = bounded(100);
        let mut liveness_handle = LivenessHandle::new(sender_service, subscriber);
        let service = LivenessService::new(receiver, stream::empty(), state, oms_handle, publisher);

        let mut pool = LocalPool::new();
        // Run the service
        pool.spawner().spawn(service.run()).unwrap();

        let res = pool.run_until(liveness_handle.get_ping_count()).unwrap();
        assert_eq!(res, 1);

        let res = pool.run_until(liveness_handle.get_pong_count()).unwrap();
        assert_eq!(res, 2);
    }

    #[test]
    fn send_ping() {
        runtime::test_async(|rt| {
            let state = Arc::new(LivenessState::new());

            // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
            let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
            let oms_handle = OutboundMessageRequester::new(outbound_tx);

            // Setup liveness service
            let (sender_service, receiver) = reply_channel::unbounded();
            let (publisher, subscriber) = bounded(100);
            let mut liveness_handle = LivenessHandle::new(sender_service, subscriber);
            let service = LivenessService::new(receiver, stream::empty(), Arc::clone(&state), oms_handle, publisher);

            // Run the LivenessService
            rt.spawn(service.run());

            let mut rng = OsRng::new().unwrap();
            let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
            // Receive outbound request
            rt.spawn(async move {
                let request = outbound_rx.select_next_some().await;
                match request {
                    DhtOutboundRequest::SendMsg(_, reply_tx) => {
                        reply_tx.send(0).unwrap();
                    },
                    _ => panic!("Unexpected OutboundRequest"),
                }
            });

            let _res = rt.block_on(liveness_handle.send_ping(pk)).unwrap();

            assert_eq!(state.pings_sent(), 1);
        })
    }

    fn create_dummy_message<T>(inner: T) -> DomainMessage<T> {
        let mut rng = OsRng::new().unwrap();
        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let peer_source = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            Vec::<NetAddress>::new().into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        DomainMessage {
            origin_pubkey: peer_source.public_key.clone(),
            source_peer: peer_source,
            inner,
        }
    }

    #[test]
    fn handle_message_ping() {
        runtime::test_async(|rt| {
            let state = Arc::new(LivenessState::new());

            // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
            let (outbound_tx, mut outbound_rx) = mpsc::channel(10);
            let oms_handle = OutboundMessageRequester::new(outbound_tx);

            let msg = create_dummy_message(PingPongMessage::ping());
            // A stream which emits one message and then closes
            let pingpong_stream = stream::iter(std::iter::once(msg));

            // Setup liveness service
            let (publisher, _subscriber) = bounded(100);
            let service = LivenessService::new(
                stream::empty(),
                pingpong_stream,
                Arc::clone(&state),
                oms_handle,
                publisher,
            );

            rt.spawn(service.run());

            rt.spawn(async move {
                unwrap_oms_send_msg!(outbound_rx.select_next_some().await);
            });
        });
    }

    #[test]
    fn handle_message_pong() {
        use std::sync::atomic::Ordering;
        let state = Arc::new(LivenessState::new());
        let mut pool = LocalPool::new();

        let (outbound_tx, _) = mpsc::channel(10);
        let oms_handle = OutboundMessageRequester::new(outbound_tx);

        let msg = create_dummy_message(PingPongMessage::pong());
        // A stream which emits one message and then closes
        let pingpong_stream = stream::iter(std::iter::once(msg));

        // Setup liveness service
        let (publisher, subscriber) = bounded(100);
        let service = LivenessService::new(
            stream::empty(),
            pingpong_stream,
            Arc::clone(&state),
            oms_handle,
            publisher,
        );

        // Create a flag that gets flipped when the subscribed event is received
        let received_event = Arc::new(AtomicBool::new(false));
        let rec_event_clone = received_event.clone();

        // Listen for the pong event
        pool.spawner()
            .spawn(async move {
                let event = subscriber.fuse().select_next_some().await;
                match *event {
                    LivenessEvent::ReceivedPong => rec_event_clone.store(true, Ordering::SeqCst),
                    _ => panic!("Unexpected event"),
                }
            })
            .expect("Couldn't spawn subscription future");

        pool.spawner().spawn(service.run()).unwrap();
        pool.run_until_stalled();

        assert_eq!(state.pongs_received(), 1);
        assert_eq!(received_event.load(Ordering::SeqCst), true);
    }
}
