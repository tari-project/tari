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

use super::messages::PingPong;
use crate::{
    services::{
        comms_outbound::CommsOutboundHandle,
        liveness::{error::LivenessError, state::LivenessState},
    },
    tari_message::{NetMessage, TariMessageType},
};
use futures::{
    future::{self, Either},
    Future,
};
use std::sync::Arc;
use tari_comms::{
    domain_subscriber::MessageInfo,
    message::MessageFlags,
    outbound_message_service::BroadcastStrategy,
    types::CommsPublicKey,
};

pub struct LivenessHandler {
    state: Arc<LivenessState>,
    outbound_handle: CommsOutboundHandle,
}

impl LivenessHandler {
    pub fn new(state: Arc<LivenessState>, outbound_handle: CommsOutboundHandle) -> Self {
        Self { state, outbound_handle }
    }

    pub fn handle_message(
        &mut self,
        info: MessageInfo,
        msg: PingPong,
    ) -> impl Future<Item = (), Error = LivenessError>
    {
        match msg {
            PingPong::Ping => {
                let state = self.state.clone();
                state.inc_pings_received();
                Either::A(self.send_pong(info.origin_source).and_then(move |_| {
                    state.inc_pongs_sent();
                    future::ok(())
                }))
            },
            PingPong::Pong => {
                self.state.inc_pongs_received();
                Either::B(future::ok(()))
            },
        }
    }

    fn send_pong(&mut self, dest: CommsPublicKey) -> impl Future<Item = (), Error = LivenessError> {
        self.outbound_handle
            .send_message(
                BroadcastStrategy::DirectPublicKey(dest),
                MessageFlags::empty(),
                TariMessageType::new(NetMessage::PingPong),
                PingPong::Pong,
            )
            .or_else(|_| future::err(LivenessError::SendPongFailed))
            .and_then(|res| match res {
                Ok(_) => future::ok(()),
                Err(err) => future::err(LivenessError::CommsOutboundError(err)),
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::services::comms_outbound::CommsOutboundRequest;
    use rand::rngs::OsRng;
    use std::sync::mpsc;
    use tari_comms::peer_manager::{NodeId, PeerNodeIdentity};
    use tari_crypto::keys::PublicKey;
    use tari_service_framework::transport;
    use tokio::runtime::Runtime;
    use tower_util::service_fn;

    fn create_dummy_message_info() -> MessageInfo {
        let mut rng = OsRng::new().unwrap();
        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let peer_source = PeerNodeIdentity::new(NodeId::from_key(&pk).unwrap(), pk.clone());
        MessageInfo {
            origin_source: peer_source.public_key.clone(),
            peer_source,
        }
    }

    #[test]
    fn handle_message_ping() {
        let mut rt = Runtime::new().unwrap();
        let state = Arc::new(LivenessState::new());
        let (tx, rx) = mpsc::channel();

        let (req, res) = transport::channel(service_fn(move |req| {
            // Send this out so that we can assert some things about it
            tx.send(req).unwrap();
            future::ok::<_, ()>(Ok(()))
        }));

        rt.spawn(res);

        let outbound_handle = CommsOutboundHandle::new(req);

        let mut handler = LivenessHandler::new(state, outbound_handle);

        let info = create_dummy_message_info();
        let fut = handler.handle_message(info.clone(), PingPong::Ping);

        let result = rt.block_on(fut);
        result.unwrap();

        assert_eq!(handler.state.pings_received(), 1);
        assert_eq!(handler.state.pongs_sent(), 1);

        match rx.try_recv().unwrap() {
            CommsOutboundRequest::SendMsg { broadcast_strategy, .. } => match broadcast_strategy {
                BroadcastStrategy::DirectPublicKey(pk) => assert_eq!(pk, info.origin_source),
                _ => panic!("unexpected broadcast strategy used"),
            },
            _ => panic!("liveness service sent unexpected message to outbound handle"),
        }
    }

    #[test]
    fn handle_message_pong() {
        let mut rt = Runtime::new().unwrap();
        let state = Arc::new(LivenessState::new());

        let (req, res) = transport::channel(service_fn(|_| future::ok::<_, ()>(Ok(()))));

        rt.spawn(res);

        let outbound_handle = CommsOutboundHandle::new(req);

        let mut handler = LivenessHandler::new(state, outbound_handle);

        let info = create_dummy_message_info();
        let fut = handler.handle_message(info, PingPong::Pong);

        let result = rt.block_on(fut);
        result.unwrap();

        assert_eq!(handler.state.pongs_received(), 1);
    }
}
