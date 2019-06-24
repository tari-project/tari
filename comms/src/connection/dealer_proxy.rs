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

use log::*;

use std::thread;

use derive_error::Error;

use crate::connection::{
    types::{Direction, SocketEstablishment},
    Connection,
    ConnectionError,
    InprocAddress,
    ZmqContext,
};
use std::thread::JoinHandle;

const LOG_TARGET: &'static str = "comms::dealer_proxy";

#[derive(Debug, Error)]
pub enum DealerProxyError {
    #[error(msg_embedded, no_from, non_std)]
    SocketError(String),
    ConnectionError(ConnectionError),
    /// The dealer [thread::JoinHandle] is unavailable
    DealerUndefined,
    /// Could not join the dealer thread
    ThreadJoinError,
}

/// A DealerProxy Result
pub type Result<T> = std::result::Result<T, DealerProxyError>;

/// Proxies two addresses, receiving from the source_address and fair dealing to the
/// sink_address.
pub struct DealerProxy {
    context: ZmqContext,
    source_address: InprocAddress,
    sink_address: InprocAddress,
    control_address: InprocAddress,
    thread_handle: Option<JoinHandle<Result<()>>>,
}

/// Spawn a new steerable dealer proxy in its own thread.
pub fn spawn_proxy(
    context: ZmqContext,
    source_address: InprocAddress,
    sink_address: InprocAddress,
    control_address: InprocAddress,
) -> JoinHandle<Result<()>>
{
    thread::spawn(move || {
        let mut source = Connection::new(&context.clone(), Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&source_address.clone())
            .map_err(|err| DealerProxyError::ConnectionError(err))?;

        let mut sink = Connection::new(&context.clone(), Direction::Outbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&sink_address.clone())
            .map_err(|err| DealerProxyError::ConnectionError(err))?;

        let mut control = Connection::new(&context.clone(), Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&control_address.clone())
            .map_err(|err| DealerProxyError::ConnectionError(err))?;

        zmq::proxy_steerable(source.get_mut_socket(), sink.get_mut_socket(), control.get_mut_socket())
            .map_err(|err| DealerProxyError::SocketError(err.to_string()))
    })
}

impl DealerProxy {
    /// Creates a new DealerProxy.
    pub fn new(context: ZmqContext, source_address: InprocAddress, sink_address: InprocAddress) -> Self {
        Self {
            context,
            source_address,
            sink_address,
            control_address: InprocAddress::random(),
            thread_handle: None,
        }
    }

    /// Proxy the source and sink addresses. This method does not block and returns stores the [thread::JoinHandle] in
    /// the DealerProxy.
    pub fn spawn_proxy(&mut self) {
        self.thread_handle = Some(spawn_proxy(
            self.context.clone(),
            self.source_address.clone(),
            self.sink_address.clone(),
            self.control_address.clone(),
        ));
    }

    /// Send a shutdown request to the dealer proxy
    pub fn shutdown(self) -> Result<()> {
        match self.thread_handle {
            Some(thread_handle) => {
                let control = Connection::new(&self.context, Direction::Outbound)
                    .set_socket_establishment(SocketEstablishment::Connect)
                    .establish(&self.control_address)
                    .map_err(|err| DealerProxyError::ConnectionError(err))?;
                control
                    .send(&["TERMINATE".as_bytes()])
                    .map_err(|err| DealerProxyError::ConnectionError(err))?;
                match thread_handle.join() {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        error!(target: LOG_TARGET, "Failed to join dealer thread handle: {:?}", err);
                        Err(DealerProxyError::ThreadJoinError)
                    },
                }
            },
            None => Err(DealerProxyError::DealerUndefined),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn threaded_proxy() {
        let context = ZmqContext::new();
        let sender_addr = InprocAddress::random();
        let receiver_addr = InprocAddress::random();
        let sender = Connection::new(&context, Direction::Outbound)
            .establish(&sender_addr)
            .unwrap();

        let receiver = Connection::new(&context, Direction::Outbound)
            .establish(&receiver_addr)
            .unwrap();

        let mut proxy = DealerProxy::new(context, sender_addr, receiver_addr);
        proxy.spawn_proxy();

        sender.send_sync(&["HELLO".as_bytes()]).unwrap();

        let msg = receiver.receive(2000).unwrap();
        assert_eq!("HELLO".as_bytes().to_vec(), msg[1]);

        // You need to attach the identity frame to the head of the message
        // so that the internal ZMQ_ROUTER will send messages back to the
        // connection which sent the message
        receiver.send_sync(&[&msg[0], "WORLD".as_bytes()]).unwrap();

        let msg = sender.receive(2000).unwrap();
        assert_eq!("WORLD".as_bytes().to_vec(), msg[0]);

        // Test steerable dealer shutdown
        proxy.shutdown().unwrap();
        receiver.send_sync(&[&msg[0], "FAIL".as_bytes()]).unwrap();
        assert!(sender.receive(200).is_err());
    }
}
