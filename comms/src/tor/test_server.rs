// Copyright 2020, The Tari Project
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
    compat::IoCompat,
    memsocket::MemorySocket,
    multiaddr::Multiaddr,
    test_utils::transport::build_connected_sockets,
};
use futures::{lock::Mutex, stream, SinkExt, StreamExt};
use std::sync::Arc;
use tokio::runtime;
use tokio_util::codec::{Framed, LinesCodec};

pub async fn spawn() -> (Multiaddr, State, MemorySocket) {
    let (addr, socket_out, socket_in) = build_connected_sockets().await;

    let server = TorControlPortTestServer::new(socket_in);
    let state = server.get_shared_state();
    runtime::Handle::current().spawn(server.run());

    (addr, state, socket_out)
}

#[derive(Clone)]
pub struct State {
    request_lines: Arc<Mutex<Vec<String>>>,
    canned_response: Arc<Mutex<Vec<String>>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            request_lines: Arc::new(Mutex::new(Vec::new())),
            canned_response: Arc::new(Mutex::new(all_to_owned(canned_responses::OK))),
        }
    }

    pub async fn set_canned_response<'a, T: AsRef<[&'a str]>>(&self, lines: T) {
        *self.canned_response.lock().await = all_to_owned(lines);
    }

    pub async fn take_requests(&self) -> Vec<String> {
        self.request_lines.lock().await.drain(..).collect()
    }
}

pub struct TorControlPortTestServer {
    socket: MemorySocket,
    state: State,
}

impl TorControlPortTestServer {
    pub fn new(socket: MemorySocket) -> Self {
        Self {
            socket,
            state: State::new(),
        }
    }

    pub fn get_shared_state(&self) -> State {
        self.state.clone()
    }

    pub async fn run(self) {
        let mut framed = Framed::new(IoCompat::new(self.socket), LinesCodec::new());
        let state = self.state;
        while let Some(msg) = framed.next().await {
            state.request_lines.lock().await.push(msg.unwrap());
            let mut responses = stream::iter(state.canned_response.lock().await.clone()).map(Ok);
            framed.send_all(&mut responses).await.unwrap();
        }
    }
}

fn all_to_owned<'a, T: AsRef<[&'a str]>>(strings: T) -> Vec<String> {
    strings.as_ref().into_iter().map(|s| (*s).to_owned()).collect()
}

pub mod canned_responses {
    pub const OK: &[&str] = &["250 OK"];
    pub const GET_CONF_OK: &[&str] = &[
        "250-HiddenServicePort=8080",
        "250-HiddenServicePort=8081 127.0.0.1:9000",
        "250 HiddenServicePort=8082 127.0.0.1:9001",
    ];

    pub const ADD_ONION_OK: &[&str] = &[
        "250-ServiceID=qigbgbs4ue3ghbupsotgh73cmmkjrin2aprlyxsrnrvpmcmzy3g4wbid",
        "250-PrivateKey=ED25519-V3:\
         Pg3GEyssauPRW3jP6mHwKOxvl_fMsF0QsZC3DvQ8jZ9AxmfRvSP35m9l0vOYyOxkOqWM6ufjdYuM8Ae6cR2UdreG6",
        "250 OK",
    ];
    pub const ADD_ONION_DISCARDPK_OK: &[&str] = &[
        "250-ServiceID=qigbgbs4ue3ghbupsotgh73cmmkjrin2aprlyxsrnrvpmcmzy3g4wbid",
        "250 OK",
    ];

    pub const ERR_552: &[&str] = &["552 Unrecognised configuration key \"dummy\""];
}
