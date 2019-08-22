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
    services::{comms_outbound::CommsOutboundHandle, liveness::messages::PingPong},
    tari_message::{NetMessage, TariMessageType},
};
use futures::{
    future::{self, Either},
    Future,
    Poll,
};
use std::sync::Arc;
use tari_comms::{message::MessageFlags, outbound_message_service::BroadcastStrategy, types::CommsPublicKey};
use tower_service::Service;

/// Service responsible for testing Liveness for Peers.
///
/// Very basic global ping and pong counter stats are implemented. In future,
/// peer latency and availability stats will be added.
pub struct LivenessService {
    state: Arc<LivenessState>,
    oms_handle: CommsOutboundHandle,
}

impl LivenessService {
    pub fn new(state: Arc<LivenessState>, oms_handle: CommsOutboundHandle) -> Self {
        Self { state, oms_handle }
    }

    fn send_ping(
        &mut self,
        pub_key: CommsPublicKey,
    ) -> impl Future<Item = Result<LivenessResponse, LivenessError>, Error = ()>
    {
        let state = self.state.clone();
        self.oms_handle
            .send_message(
                BroadcastStrategy::DirectPublicKey(pub_key),
                MessageFlags::empty(),
                TariMessageType::new(NetMessage::PingPong),
                PingPong::Ping,
            )
            .and_then(move |res| {
                state.inc_pings_sent();
                future::ok(
                    res.map(|_| LivenessResponse::PingSent)
                        .map_err(LivenessError::CommsOutboundError),
                )
            })
            .or_else(|_| future::ok(Err(LivenessError::SendPingFailed)))
    }

    fn get_ping_count(&self) -> usize {
        self.state.pings_received()
    }

    fn get_pong_count(&self) -> usize {
        self.state.pongs_received()
    }
}

impl Service<LivenessRequest> for LivenessService {
    type Error = ();
    type Future = impl Future<Item = Self::Response, Error = Self::Error>;
    type Response = Result<LivenessResponse, LivenessError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, req: LivenessRequest) -> Self::Future {
        match req {
            LivenessRequest::SendPing(pub_key) => Either::A(
                self.send_ping(pub_key)
                    .or_else(|_| future::ok(Err(LivenessError::SendPingFailed))),
            ),
            LivenessRequest::GetPingCount => Either::B(future::ok(Result::<_, LivenessError>::Ok(
                LivenessResponse::Count(self.get_ping_count()),
            ))),
            LivenessRequest::GetPongCount => Either::B(future::ok(Result::<_, LivenessError>::Ok(
                LivenessResponse::Count(self.get_pong_count()),
            ))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::executor::transport;
    use futures::Async;
    use rand::rngs::OsRng;
    use tari_crypto::keys::PublicKey;
    use tokio::runtime::Runtime;
    use tower_util::service_fn;

    #[test]
    fn get_ping_pong_count() {
        let state = Arc::new(LivenessState::new());
        state.inc_pings_received();
        state.inc_pongs_received();
        state.inc_pongs_received();

        let outbound_service = service_fn(|_| future::ok::<_, ()>(Ok(())));
        let (req, _res) = transport::channel(outbound_service);
        let oms_handle = CommsOutboundHandle::new(req);

        let mut service = LivenessService::new(state, oms_handle);

        let mut fut = service.call(LivenessRequest::GetPingCount);
        match fut.poll().unwrap() {
            Async::Ready(Ok(LivenessResponse::Count(n))) => assert_eq!(n, 1),
            _ => panic!(),
        }

        let mut fut = service.call(LivenessRequest::GetPongCount);
        match fut.poll().unwrap() {
            Async::Ready(Ok(LivenessResponse::Count(n))) => assert_eq!(n, 2),
            _ => panic!(),
        }
    }

    #[test]
    fn send_ping() {
        let mut rt = Runtime::new().unwrap();
        let state = Arc::new(LivenessState::new());

        // This service stubs out CommsOutboundService and always returns a successful result.
        // Therefore, LivenessService will behave as if it was able to send the ping
        // without actually sending it.
        let outbound_service = service_fn(|_| future::ok::<_, ()>(Ok(())));
        let (req, res) = transport::channel(outbound_service);
        rt.spawn(res);

        let oms_handle = CommsOutboundHandle::new(req);

        let mut service = LivenessService::new(Arc::clone(&state), oms_handle);

        let mut rng = OsRng::new().unwrap();
        let (_, pk) = CommsPublicKey::random_keypair(&mut rng);
        let fut = service.call(LivenessRequest::SendPing(pk));
        match rt.block_on(fut).unwrap() {
            Ok(LivenessResponse::PingSent) => {},
            Ok(_) => panic!("received unexpected response from liveness service"),
            Err(err) => panic!("received unexpected error from liveness service: {:?}", err),
        }

        assert_eq!(state.pings_sent(), 1);
    }
}
