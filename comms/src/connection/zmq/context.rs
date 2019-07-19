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

use zmq;

use crate::connection::{types::SocketType, zmq::ZmqError};

/// Thin wrapper of a [0MQ context].
///
/// [0MQ context]: http://api.zeromq.org/2-1:zmq#toc3
#[derive(Clone, Default)]
pub struct ZmqContext(zmq::Context);

impl ZmqContext {
    pub fn new() -> Self {
        Self(zmq::Context::new())
    }

    pub fn socket(&self, socket_type: SocketType) -> Result<zmq::Socket, ZmqError> {
        use SocketType::*;

        let zmq_socket_type = match socket_type {
            Request => zmq::REQ,
            Reply => zmq::REP,
            Router => zmq::ROUTER,
            Dealer => zmq::DEALER,
            Pub => zmq::PUB,
            Sub => zmq::SUB,
            Push => zmq::PUSH,
            Pull => zmq::PULL,
            Pair => zmq::PAIR,
        };

        self.0
            .socket(zmq_socket_type)
            .map_err(|e| ZmqError::SocketError(format!("{}", e)))
    }
}
