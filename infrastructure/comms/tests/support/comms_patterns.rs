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
use tari_comms::connection::{
    message::FrameSet,
    types::SocketType,
    zmq::{Context, CurvePublicKey, ZmqEndpoint},
};

/// Creates an [AsyncRequestReplyPattern].
///
/// [AsyncRequestReplyPattern]: struct.AsyncRequestReplyPattern.html
pub fn async_request_reply<T>() -> AsyncRequestReplyPattern<T>
where T: ZmqEndpoint + Clone + Send + Sync + 'static {
    AsyncRequestReplyPattern::new()
}

/// This pattern starts a new thread, sends a message and waits for a response.
/// Once a response is received, the thread exits. This can be used to write functional
/// tests for request/reply flows.
pub struct AsyncRequestReplyPattern<T: ZmqEndpoint + Clone + Send + Sync + 'static> {
    endpoint: Option<T>,
    identity: Option<String>,
    public_key: Option<CurvePublicKey>,
    frames: Option<FrameSet>,
}

impl<T> AsyncRequestReplyPattern<T>
where T: ZmqEndpoint + Clone + Send + Sync + 'static
{
    /// Create a new AsyncRequestReplyPattern
    pub fn new() -> Self {
        AsyncRequestReplyPattern {
            endpoint: None,
            identity: None,
            public_key: None,
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

    /// Set the server public key to use for encrypted connections.
    pub fn set_public_key(mut self, pk: CurvePublicKey) -> Self {
        self.public_key = Some(pk);
        self
    }

    /// Sets the data to send.
    pub fn set_send_data(mut self, frames: FrameSet) -> Self {
        self.frames = Some(frames);
        self
    }

    /// Start the thread and run the pattern!
    pub fn run(self, ctx: Context) -> Receiver<()> {
        let (tx, rx) = channel();
        let identity = self.identity.clone().unwrap();
        let public_key = self.public_key.clone();
        let endpoint = self.endpoint.clone().unwrap();
        let msgs = self.frames.clone().unwrap();
        thread::spawn(move || {
            let socket = ctx.socket(SocketType::Dealer).unwrap();
            socket.set_identity(identity.as_bytes()).unwrap();

            let keypair = zmq::CurveKeyPair::new().unwrap();
            if let Some(public_key) = public_key {
                socket.set_curve_serverkey(&public_key.into_inner()).unwrap();
                socket.set_curve_publickey(&keypair.public_key).unwrap();
                socket.set_curve_secretkey(&keypair.secret_key).unwrap();
            }
            socket.set_linger(100).unwrap();
            socket.connect(endpoint.to_zmq_endpoint().as_str()).unwrap();
            socket
                .send_multipart(msgs.iter().map(|s| s.as_slice()).collect::<Vec<&[u8]>>().as_slice(), 0)
                .unwrap();

            let data = socket.recv_multipart(0).unwrap();
            println!("Received {:?}", data);

            // Send thread done signal
            tx.send(()).unwrap();
        });
        rx
    }
}
