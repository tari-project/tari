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

use std::thread;

use derive_error::Error;

use crate::connection::{
    types::{Direction, SocketEstablishment},
    Connection,
    ConnectionError,
    Context,
    InprocAddress,
};

#[derive(Debug, Error)]
pub enum DealerProxyError {
    #[error(msg_embedded, no_from, non_std)]
    SocketError(String),
    ConnectionError(ConnectionError),
}

/// A DealerProxy Result
pub type Result<T> = std::result::Result<T, DealerProxyError>;

/// Proxies two addresses, receiving from the source_address and fair dealing to the
/// sink_address.
pub struct DealerProxy {
    source_address: InprocAddress,
    sink_address: InprocAddress,
}

impl DealerProxy {
    /// Creates a new DealerProxy.
    pub fn new(source_address: InprocAddress, sink_address: InprocAddress) -> Self {
        Self {
            source_address,
            sink_address,
        }
    }

    /// Proxy the source and sink addresses. This method does not block and returns
    /// a [thread::JoinHandle] of the proxy thread.
    pub fn spawn_proxy(self, context: Context) -> thread::JoinHandle<Result<()>> {
        thread::spawn(move || self.proxy(&context))
    }

    /// Proxy the source and sink addresses. This method will block the current thread.
    pub fn proxy(&self, context: &Context) -> Result<()> {
        let source = Connection::new(context, Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&self.source_address)
            .map_err(|err| DealerProxyError::ConnectionError(err))?;

        let sink = Connection::new(context, Direction::Outbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&self.sink_address)
            .map_err(|err| DealerProxyError::ConnectionError(err))?;

        zmq::proxy(source.get_socket(), sink.get_socket()).map_err(|err| DealerProxyError::SocketError(err.to_string()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn threaded_proxy() {
        let context = Context::new();
        let sender_addr = InprocAddress::random();
        let receiver_addr = InprocAddress::random();
        let sender = Connection::new(&context, Direction::Outbound)
            .establish(&sender_addr)
            .unwrap();

        let receiver = Connection::new(&context, Direction::Outbound)
            .establish(&receiver_addr)
            .unwrap();

        let proxy = DealerProxy::new(sender_addr, receiver_addr);
        proxy.spawn_proxy(context);

        sender.send_sync(&["HELLO".as_bytes()]).unwrap();

        let msg = receiver.receive(2000).unwrap();
        assert_eq!("HELLO".as_bytes().to_vec(), msg[1]);

        // You need to attach the identity frame to the head of the message
        // so that the internal ZMQ_ROUTER will send messages back to the
        // connection which sent the message
        receiver.send_sync(&[&msg[0], "WORLD".as_bytes()]).unwrap();

        let msg = sender.receive(2000).unwrap();
        assert_eq!("WORLD".as_bytes().to_vec(), msg[0]);
    }
}
