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
    connection::{net_address::ip::SocketAddress, NetAddress, ZmqContext},
    connection_manager::ConnectionManager,
    control_service::types::ControlServiceDispatcher,
    peer_manager::NodeIdentity,
    types::DEFAULT_LISTENER_ADDRESS,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    sync::{mpsc::SyncSender, Arc},
    thread,
    time::Duration,
};

const LOG_TARGET: &'static str = "comms::control_service::service";

/// Configuration for [ControlService]
#[derive(Clone)]
pub struct ControlServiceConfig<T>
where T: Clone
{
    /// Which address to open a port
    pub listener_address: NetAddress,
    /// Optional SOCKS proxy
    pub socks_proxy_address: Option<SocketAddress>,
    /// The message type
    pub accept_message_type: T,
    /// The timeout when connecting to the requesting peer's inbound connection.
    /// If this timeout expires the peer connection will not be established.
    pub requested_outbound_connection_timeout: Duration,
}

impl<T> Default for ControlServiceConfig<T>
where
    T: Default,
    T: Clone,
{
    fn default() -> Self {
        let listener_address = DEFAULT_LISTENER_ADDRESS.parse::<NetAddress>().unwrap();
        ControlServiceConfig {
            listener_address,
            socks_proxy_address: None,
            accept_message_type: T::default(),
            requested_outbound_connection_timeout: Duration::from_secs(2),
        }
    }
}

/// The service responsible for establishing new [PeerConnection]s.
/// When `serve` is called, a worker thread starts up which listens for
/// connections on the configured `listener_address`.
///
/// ```rust
/// # use tari_comms::{connection::*, control_service::*, dispatcher::*, connection_manager::*, peer_manager::*, types::*};
/// # use tari_comms::control_service::handlers as comms_handlers;
/// # use std::{time::Duration, sync::Arc};
/// # use tari_storage::lmdb::LMDBStore;
/// # use std::collections::HashMap;
/// # use rand::OsRng;
///
/// let node_identity = Arc::new(NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap());
///
/// let context = ZmqContext::new();
/// let listener_address = "127.0.0.1:9000".parse::<NetAddress>().unwrap();
///
/// let peer_manager = Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap());
///
/// let conn_manager = Arc::new(ConnectionManager::new(context.clone(), node_identity.clone(), peer_manager.clone(), PeerConnectionConfig {
///      max_message_size: 1024,
///      max_connect_retries: 1,
///      socks_proxy_address: None,
///      message_sink_address: InprocAddress::random(),
///      host: "127.0.0.1".parse().unwrap(),
///      peer_connection_establish_timeout: Duration::from_secs(4),
/// }));
///
/// let service = ControlService::<u8>::with_default_config(
///       context,
///       node_identity,
///     )
///     .serve(conn_manager)
///     .unwrap();
///
/// service.shutdown().unwrap();
/// ```
pub struct ControlService<MType>
where MType: Clone
{
    context: ZmqContext,
    dispatcher: ControlServiceDispatcher<MType>,
    config: ControlServiceConfig<MType>,
    node_identity: Arc<NodeIdentity>,
}

impl<MType> ControlService<MType>
where
    MType: Default,
    MType: Clone,
    MType: Serialize + DeserializeOwned,
{
    pub fn with_default_config(context: ZmqContext, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            context,
            dispatcher: Default::default(),
            config: Default::default(),
            node_identity,
        }
    }
}

impl<MType> ControlService<MType>
where
    MType: Send + Sync + 'static,
    MType: Serialize + DeserializeOwned,
    MType: Clone,
{
    setter!(with_custom_dispatcher, dispatcher, ControlServiceDispatcher<MType>);

    pub fn new(context: ZmqContext, node_identity: Arc<NodeIdentity>, config: ControlServiceConfig<MType>) -> Self {
        Self {
            context,
            dispatcher: Default::default(),
            config,
            node_identity,
        }
    }

    pub fn serve(self, connection_manager: Arc<ConnectionManager>) -> Result<ControlServiceHandle> {
        let config = self.config;
        Ok(ControlServiceWorker::start(
            self.context.clone(),
            self.node_identity,
            self.dispatcher,
            config,
            connection_manager,
        )?
        .into())
    }
}

/// This is retured from the `ControlService::serve` method. It s a thread-safe
/// handle which can send control messages to the [ControlService] worker.
#[derive(Debug)]
pub struct ControlServiceHandle {
    pub handle: thread::JoinHandle<Result<()>>,
    sender: SyncSender<ControlMessage>,
}

impl ControlServiceHandle {
    /// Send a [ControlMessage::Shutdown] message to the worker thread.
    pub fn shutdown(&self) -> Result<()> {
        warn!(target: LOG_TARGET, "CONTROL SERVICE SHUTDOWN");
        self.sender
            .send(ControlMessage::Shutdown)
            .map_err(|_| ControlServiceError::ControlMessageSendFailed)
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

    #[test]
    fn control_service_has_default() {
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let control_service = ControlService::<u8>::with_default_config(context, node_identity);
        assert_eq!(
            control_service.config.listener_address,
            DEFAULT_LISTENER_ADDRESS.parse::<NetAddress>().unwrap()
        );
        assert!(control_service.config.socks_proxy_address.is_none());
        assert_eq!(control_service.config.accept_message_type, 0);
    }
}
