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

use super::{
    error::ControlServiceError,
    types::{ControlMessage, Result},
    worker::ControlServiceWorker,
};
use crate::{
    connection::ZmqContext,
    connection_manager::ConnectionManager,
    peer_manager::NodeIdentity,
    types::DEFAULT_CONTROL_PORT_ADDRESS,
};
use log::*;
use multiaddr::Multiaddr;
use std::{
    net::SocketAddr,
    sync::{mpsc::SyncSender, Arc},
    thread,
    time::Duration,
};
use tari_utilities::thread_join::ThreadJoinWithTimeout;

const LOG_TARGET: &str = "comms::control_service::service";

/// Configuration for [ControlService]
#[derive(Clone)]
pub struct ControlServiceConfig {
    /// Which address to listen on. This must be a TCP/IP address.
    pub listening_address: Multiaddr,
    /// The publicly accessible address which is sent to peers. If set to None, the TCP/IP4 address of the inbound
    /// listener is used. Using the latter is more useful in tests, in a production deployment an address should be
    /// specified.
    pub public_peer_address: Option<Multiaddr>,
    /// Optional SOCKS proxy
    pub socks_proxy_address: Option<SocketAddr>,
    /// The timeout for the peer to connect to the inbound connection.
    /// If this timeout expires the peer connection will be shut down and discarded.
    pub requested_connection_timeout: Duration,
}

impl Default for ControlServiceConfig {
    fn default() -> Self {
        let listening_address = DEFAULT_CONTROL_PORT_ADDRESS.parse::<Multiaddr>().unwrap();
        ControlServiceConfig {
            listening_address,
            public_peer_address: None,
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_secs(5),
        }
    }
}

/// The service responsible for establishing new [PeerConnection]s.
/// When `serve` is called, a worker thread starts up which listens for
/// connections on the configured `listener_address`.
pub struct ControlService {
    context: ZmqContext,
    config: ControlServiceConfig,
    node_identity: Arc<NodeIdentity>,
}

impl ControlService {
    pub fn with_default_config(context: ZmqContext, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            context,
            config: Default::default(),
            node_identity,
        }
    }
}

impl ControlService {
    pub fn new(context: ZmqContext, node_identity: Arc<NodeIdentity>, config: ControlServiceConfig) -> Self {
        Self {
            context,
            config,
            node_identity,
        }
    }

    pub fn serve(self, connection_manager: Arc<ConnectionManager>) -> Result<ControlServiceHandle> {
        let config = self.config;
        Ok(ControlServiceWorker::run(self.context, self.node_identity, config, connection_manager)?.into())
    }
}

/// This is retured from the `ControlService::serve` method. It s a thread-safe
/// handle which can send control messages to the [ControlService] worker.
#[derive(Debug)]
pub struct ControlServiceHandle {
    handle: thread::JoinHandle<Result<()>>,
    sender: SyncSender<ControlMessage>,
}

impl ControlServiceHandle {
    /// Send a [ControlMessage::Shutdown] message to the worker thread.
    pub fn shutdown(&self) -> Result<()> {
        info!(target: LOG_TARGET, "Sending control service shutdown message");
        self.sender
            .send(ControlMessage::Shutdown)
            .map_err(|_| ControlServiceError::ControlMessageSendFailed)
    }

    pub fn timeout_join(self, timeout: Duration) -> Result<()> {
        self.handle
            .timeout_join(timeout)
            .map_err(ControlServiceError::WorkerThreadJoinFailed)
    }
}

impl From<(thread::JoinHandle<Result<()>>, SyncSender<ControlMessage>)> for ControlServiceHandle {
    fn from((handle, sender): (thread::JoinHandle<Result<()>>, SyncSender<ControlMessage>)) -> Self {
        Self { handle, sender }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::peer_manager::PeerFeatures;

    #[test]
    fn control_service_has_default() {
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None, PeerFeatures::empty()));
        let control_service = ControlService::with_default_config(context, node_identity);
        assert_eq!(
            control_service.config.listening_address,
            DEFAULT_CONTROL_PORT_ADDRESS.parse::<Multiaddr>().unwrap()
        );
        assert!(control_service.config.socks_proxy_address.is_none());
    }
}
