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

use crate::connection::zmq::ZmqEndpoint;
use multiaddr::{Multiaddr, Protocol};

impl ZmqEndpoint for Multiaddr {
    type Error = multiaddr::Error;

    fn to_zmq_endpoint(&self) -> Result<String, Self::Error> {
        let mut iter = self.iter();
        let protocol = iter.next().ok_or_else(|| multiaddr::Error::InvalidMultiaddr)?;
        let tcp_port = |tcp: Option<Protocol>| match tcp.unwrap_or(Protocol::Tcp(0)) {
            Protocol::Tcp(port) => Ok(port),
            _ => Err(multiaddr::Error::InvalidMultiaddr),
        };
        match protocol {
            Protocol::Onion(addr, port) => Ok(format!(
                "tcp://{}.onion:{}",
                String::from_utf8_lossy(addr.as_ref()),
                port
            )),
            Protocol::Ip4(ip) => Ok(format!("tcp://{}:{}", ip, tcp_port(iter.next())?)),
            Protocol::Ip6(ip) => Ok(format!("tcp://{}:{}", ip, tcp_port(iter.next())?)),
            _ => Err(multiaddr::Error::InvalidMultiaddr),
        }
    }
}
