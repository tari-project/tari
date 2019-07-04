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
    types::{Direction, SocketEstablishment, SocketType},
    zmq::ZmqEndpoint,
    Connection,
    ConnectionError,
    InprocAddress,
    ZmqContext,
};
use std::{sync::mpsc::channel, thread::JoinHandle, time::Duration};
use tari_utilities::thread_join::{ThreadError, ThreadJoinWithTimeout};

const LOG_TARGET: &'static str = "comms::dealer_proxy";

/// Set the allocated stack size for the DealerProxy thread
const THREAD_STACK_SIZE: usize = 64 * 1024; // 64kb

/// Set the maximum waiting time for DealerProxy thread to join
const THREAD_JOIN_TIMEOUT_IN_MS: Duration = Duration::from_millis(100);

#[derive(Debug, Error)]
pub enum DealerProxyError {
    #[error(msg_embedded, no_from, non_std)]
    SocketError(String),
    ConnectionError(ConnectionError),
    /// The dealer [thread::JoinHandle] is unavailable
    DealerUndefined,
    /// Could not join the dealer thread
    ThreadJoinError(ThreadError),
    /// Proxy thread failed to start within 10 seconds
    ThreadStartFailed,
    #[error(msg_embedded, no_from, non_std)]
    ZmqError(String),
    /// Dealer proxy thread failed to start
    ThreadInitializationError,
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
    pub fn spawn_proxy(&mut self) -> Result<()> {
        let (ready_tx, ready_rx) = channel();

        let context = self.context.clone();
        let source_address = self.source_address.clone();
        let sink_address = self.sink_address.clone();
        let control_address = self.control_address.clone();

        self.thread_handle = Some(
            thread::Builder::new()
                .name("dealer-proxy-thread".to_string())
                .stack_size(THREAD_STACK_SIZE)
                .spawn(move || {
                    let mut source = Connection::new(&context.clone(), Direction::Inbound)
                        .set_name("dealer-proxy-source")
                        .set_socket_establishment(SocketEstablishment::Bind)
                        .establish(&source_address.clone())
                        .map_err(|err| DealerProxyError::ConnectionError(err))?;

                    let mut sink = Connection::new(&context.clone(), Direction::Outbound)
                        .set_name("dealer-proxy-sink")
                        .set_socket_establishment(SocketEstablishment::Bind)
                        .establish(&sink_address.clone())
                        .map_err(|err| DealerProxyError::ConnectionError(err))?;

                    let mut control = context
                        .socket(SocketType::Sub)
                        .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;
                    control
                        .connect(&control_address.to_zmq_endpoint())
                        .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;
                    control
                        .set_subscribe(&[])
                        .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;

                    let _ = ready_tx.send(()).unwrap();

                    zmq::proxy_steerable(source.get_socket_mut(), sink.get_socket_mut(), &mut control)
                        .map_err(|err| DealerProxyError::SocketError(err.to_string()))
                })
                .map_err(|_| DealerProxyError::ThreadInitializationError)?,
        );

        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| DealerProxyError::ThreadStartFailed)
    }

    pub fn is_running(&self) -> bool {
        self.thread_handle.is_some()
    }

    /// Send a shutdown request to the dealer proxy. If the dealer proxy has not been started
    /// this method has no effect.
    pub fn shutdown(self) -> Result<()> {
        if let Some(thread_handle) = self.thread_handle {
            let control = self
                .context
                .socket(SocketType::Pub)
                .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;

            control
                .set_linger(3000)
                .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;

            control
                .bind(&self.control_address.to_zmq_endpoint())
                .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;

            control
                .send("TERMINATE", zmq::DONTWAIT)
                .map_err(|err| DealerProxyError::ZmqError(err.to_string()))?;

            thread_handle
                .timeout_join(THREAD_JOIN_TIMEOUT_IN_MS)
                .map_err(|err| DealerProxyError::ThreadJoinError(err))
                .or_else(|err| {
                    error!(
                        target: LOG_TARGET,
                        "Dealer proxy thread exited with an error: {:?}", err
                    );
                    Err(err)
                })?;
        }

        Ok(())
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
        proxy.spawn_proxy().unwrap();

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
