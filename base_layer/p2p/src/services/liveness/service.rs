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

use super::{error::LivenessError, state::LivenessState, LivenessRequest, LivenessResponse};
use crate::{
    domain_subscriber::DomainMessage,
    services::{comms_outbound::CommsOutboundHandle, liveness::messages::PingPong},
    tari_message::{NetMessage, TariMessageType},
};
use futures::{pin_mut, stream::StreamExt, Stream};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::MessageFlags,
    outbound_message_service::{BroadcastStrategy, OutboundServiceError},
    types::CommsPublicKey,
};
use tari_service_framework::RequestContext;

const LOG_TARGET: &'static str = "tari_p2p::services::liveness";

/// Convenience type alias for a request receiver which receives LivenessRequests and sends back
/// a Result.
// type LivenessRequestRx = Receiver<LivenessRequest, Result<LivenessResponse, LivenessError>>;

/// Service responsible for testing Liveness for Peers.
///
/// Very basic global ping and pong counter stats are implemented. In future,
/// peer latency and availability stats will be added.
pub struct LivenessService<THandleStream, TPingStream> {
    request_rx: Option<THandleStream>,
    ping_stream: Option<TPingStream>,
    state: Arc<LivenessState>,
    oms_handle: CommsOutboundHandle,
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream> {
    pub fn new(
        request_rx: THandleStream,
        ping_stream: TPingStream,
        state: Arc<LivenessState>,
        oms_handle: CommsOutboundHandle,
    ) -> Self
    {
        Self {
            request_rx: Some(request_rx),
            ping_stream: Some(ping_stream),
            state,
            oms_handle,
        }
    }
}

impl<THandleStream, TPingStream> LivenessService<THandleStream, TPingStream>
where
    TPingStream: Stream<Item = DomainMessage<PingPong>>,
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
                complete => break,
            }
        }
    }

    async fn handle_incoming_message(&mut self, msg: DomainMessage<PingPong>) -> Result<(), LivenessError> {
        match msg.inner() {
            PingPong::Ping => {
                self.state.inc_pings_received();
                self.send_pong(msg.origin_source).await.unwrap();
                self.state.inc_pongs_sent();
            },
            PingPong::Pong => {
                self.state.inc_pongs_received();
            },
        }
        Ok(())
    }

    async fn send_pong(&mut self, dest: CommsPublicKey) -> Result<(), LivenessError> {
        self.oms_handle
            .send_message(
                BroadcastStrategy::DirectPublicKey(dest),
                MessageFlags::empty(),
                TariMessageType::new(NetMessage::PingPong),
                PingPong::Pong,
            )
            .await
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
            .send_message(
                BroadcastStrategy::DirectPublicKey(pub_key),
                MessageFlags::empty(),
                TariMessageType::new(NetMessage::PingPong),
                PingPong::Ping,
            )
            .await
            .map_err(Into::<OutboundServiceError>::into)?;

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
    use futures::{channel::mpsc, executor::LocalPool, stream, task::SpawnExt};
    use rand::rngs::OsRng;
    use tari_comms::{
        outbound_message_service::OutboundRequest,
        peer_manager::{NodeId, PeerNodeIdentity},
    };
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::reply_channel;
    use tower_service::Service;

    #[test]
    fn get_ping_pong_count() {
        let state = Arc::new(LivenessState::new());
        state.inc_pings_received();
        state.inc_pongs_received();
        state.inc_pongs_received();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        let (outbound_tx, _) = mpsc::unbounded();
        let oms_handle = CommsOutboundHandle::new(outbound_tx);

        // Setup liveness service
        let (mut sender_service, receiver) = reply_channel::unbounded();
        let service = LivenessService::new(receiver, stream::empty(), state, oms_handle);

        let mut pool = LocalPool::new();
        // Run the service
        pool.spawner().spawn(service.run()).unwrap();

        let res = pool.run_until(sender_service.call(LivenessRequest::GetPingCount));
        match res.unwrap() {
            Ok(LivenessResponse::Count(n)) => assert_eq!(n, 1),
            _ => panic!("Unexpected service result"),
        }

        let res = pool.run_until(sender_service.call(LivenessRequest::GetPongCount));
        match res.unwrap() {
            Ok(LivenessResponse::Count(n)) => assert_eq!(n, 2),
            _ => panic!("Unexpected service result"),
        }
    }

    #[test]
    fn send_ping() {
        let state = Arc::new(LivenessState::new());
        let mut pool = LocalPool::new();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        // TODO(sdbondi): Setting up a "dummy" CommsOutbound service should be moved into testing utilities
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded();
        let oms_handle = CommsOutboundHandle::new(outbound_tx);

        // Setup liveness service
        let (mut sender_service, receiver) = reply_channel::unbounded();
        let service = LivenessService::new(receiver, stream::empty(), Arc::clone(&state), oms_handle);

        // Run the LivenessService
        pool.spawner().spawn(service.run()).unwrap();

        let mut rng = OsRng::new().unwrap();
        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let res = pool.run_until(sender_service.call(LivenessRequest::SendPing(pk)));
        match res.unwrap() {
            Ok(LivenessResponse::PingSent) => {},
            Ok(_) => panic!("received unexpected response from liveness service"),
            Err(err) => panic!("received unexpected error from liveness service: {:?}", err),
        }

        // Receive outbound request
        pool.run_until(async move {
            let request = outbound_rx.select_next_some().await;
            match request {
                OutboundRequest::SendMsg { .. } => {},
                _ => panic!("Unexpected OutboundRequest"),
            }
        });

        assert_eq!(state.pings_sent(), 1);
    }

    fn create_dummy_message<T>(inner: T) -> DomainMessage<T> {
        let mut rng = OsRng::new().unwrap();
        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let peer_source = PeerNodeIdentity::new(NodeId::from_key(&pk).unwrap(), pk.clone());
        DomainMessage {
            origin_source: peer_source.public_key.clone(),
            peer_source,
            inner,
        }
    }

    #[test]
    fn handle_message_ping() {
        let state = Arc::new(LivenessState::new());
        let mut pool = LocalPool::new();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        // TODO(sdbondi): Setting up a "dummy" CommsOutbound service should be moved into testing utilities
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded();
        let oms_handle = CommsOutboundHandle::new(outbound_tx);

        let msg = create_dummy_message(PingPong::Ping);
        // A stream which emits one message and then closes
        let pingpong_stream = stream::iter(std::iter::once(msg));

        // Setup liveness service
        let service = LivenessService::new(stream::empty(), pingpong_stream, Arc::clone(&state), oms_handle);

        pool.spawner().spawn(service.run()).unwrap();

        let oms_request = pool.run_until(outbound_rx.next()).unwrap();

        match oms_request {
            OutboundRequest::SendMsg { .. } => {},
            _ => panic!("Unpexpected OMS request"),
        }

        pool.run_until_stalled();

        assert_eq!(state.pings_received(), 1);
        assert_eq!(state.pongs_sent(), 1);
    }

    #[test]
    fn handle_message_pong() {
        let state = Arc::new(LivenessState::new());
        let mut pool = LocalPool::new();

        // Setup a CommsOutbound service handle which is not connected to the actual CommsOutbound service
        // TODO(sdbondi): Setting up a "dummy" CommsOutbound service should be moved into testing utilities
        let (outbound_tx, _) = mpsc::unbounded();
        let oms_handle = CommsOutboundHandle::new(outbound_tx);

        let msg = create_dummy_message(PingPong::Pong);
        // A stream which emits one message and then closes
        let pingpong_stream = stream::iter(std::iter::once(msg));

        // Setup liveness service
        let service = LivenessService::new(stream::empty(), pingpong_stream, Arc::clone(&state), oms_handle);

        pool.spawner().spawn(service.run()).unwrap();

        pool.run_until_stalled();

        assert_eq!(state.pongs_received(), 1);
    }
}
