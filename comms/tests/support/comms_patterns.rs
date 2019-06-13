//  Copyright 2019 The Tari Project
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

use std::{
    sync::mpsc::{channel, Receiver},
    thread,
};
use tari_comms::{
    connection::{
        types::{Direction, SocketType},
        zmq::{CurvePublicKey, CurveSecretKey, ZmqEndpoint},
        CurveEncryption,
        ZmqContext,
    },
    message::FrameSet,
};

/// Creates an [AsyncRequestReplyPattern].
///
/// [AsyncRequestReplyPattern]: struct.AsyncRequestReplyPattern.html
pub fn async_request_reply<T>(direction: Direction) -> AsyncRequestReplyPattern<T>
where T: ZmqEndpoint + Clone + Send + Sync + 'static {
    AsyncRequestReplyPattern::new(direction)
}

/// This pattern either sends a message and waits for a response or waits for
/// a response and sends a reply.
/// Once a response is received, the thread exits. This can be used to write functional
/// tests for request/reply flows.
pub struct AsyncRequestReplyPattern<T: ZmqEndpoint + Clone + Send + Sync + 'static> {
    direction: Direction,
    endpoint: Option<T>,
    identity: Option<String>,
    secret_key: Option<CurveSecretKey>,
    server_public_key: Option<CurvePublicKey>,
    frames: Option<FrameSet>,
}

impl<T> AsyncRequestReplyPattern<T>
where T: ZmqEndpoint + Clone + Send + Sync + 'static
{
    /// Create a new AsyncRequestReplyPattern
    pub fn new(direction: Direction) -> Self {
        AsyncRequestReplyPattern {
            direction,
            endpoint: None,
            identity: None,
            secret_key: None,
            server_public_key: None,
            frames: None,
        }
    }

    /// Set the endpoint to/from which data is sent and received
    pub fn set_endpoint(mut self, v: T) -> Self {
        self.endpoint = Some(v);
        self
    }

    /// Set the identity to use when sending data
    pub fn set_identity(mut self, v: &str) -> Self {
        self.identity = Some(v.to_string());
        self
    }

    /// Set the secret key to use for encrypted connections.
    pub fn set_secret_key(mut self, sk: CurveSecretKey) -> Self {
        self.secret_key = Some(sk);
        self
    }

    /// Set the server public key to connect to a corresponding curve server.
    pub fn set_server_public_key(mut self, spk: CurvePublicKey) -> Self {
        self.server_public_key = Some(spk);
        self
    }

    /// Sets the data to send.
    pub fn set_send_data(mut self, frames: FrameSet) -> Self {
        self.frames = Some(frames);
        self
    }

    /// Start the thread and run the pattern!
    pub fn run(self, ctx: ZmqContext) -> Receiver<()> {
        let (tx, rx) = channel();
        let identity = self.identity.clone();
        let secret_key = self.secret_key.clone();
        let server_public_key = self.server_public_key.clone();
        let endpoint = self.endpoint.clone().unwrap();
        let msgs = self.frames.clone().unwrap();
        let direction = self.direction;
        thread::spawn(move || {
            let socket = ctx.socket(SocketType::Dealer).unwrap();
            if let Some(i) = identity {
                socket.set_identity(i.as_bytes()).unwrap();
            }

            socket.set_linger(100).unwrap();

            match direction {
                Direction::Inbound => {
                    if let Some(sk) = secret_key {
                        socket.set_curve_server(true).unwrap();
                        socket.set_curve_secretkey(&sk.into_inner()).unwrap();
                    }
                    socket.bind(endpoint.to_zmq_endpoint().as_str()).unwrap();

                    socket.recv_multipart(0).unwrap();

                    socket
                        .send_multipart(msgs.iter().map(|s| s.as_slice()).collect::<Vec<&[u8]>>().as_slice(), 0)
                        .unwrap();

                    // Send thread done signal
                    tx.send(()).unwrap();
                },

                Direction::Outbound => {
                    if let Some(spk) = server_public_key {
                        socket.set_curve_serverkey(&spk.into_inner()).unwrap();
                        let keypair = CurveEncryption::generate_keypair().unwrap();
                        socket.set_curve_publickey(&keypair.1.into_inner()).unwrap();
                        socket.set_curve_secretkey(&keypair.0.into_inner()).unwrap();
                    }

                    socket.connect(endpoint.to_zmq_endpoint().as_str()).unwrap();
                    socket
                        .send_multipart(msgs.iter().map(|s| s.as_slice()).collect::<Vec<&[u8]>>().as_slice(), 0)
                        .unwrap();

                    socket.recv_multipart(0).unwrap();

                    // Send thread done signal
                    tx.send(()).unwrap();
                },
            }
        });
        rx
    }
}
