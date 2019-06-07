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
use std::{sync::mpsc::SyncSender, thread};

use crate::{
    connection::{net_address::ip::SocketAddress, Context, NetAddress},
    connection_manager::ConnectionManager,
    dispatcher::{DispatchResolver, DispatchableKey},
    types::DEFAULT_LISTENER_ADDRESS,
};

use super::{
    error::ControlServiceError,
    types::{ControlMessage, ControlServiceDispatcher, ControlServiceMessageContext, Result},
    worker::ControlServiceWorker,
};
use std::sync::Arc;

const LOG_TARGET: &'static str = "comms::control_service::service";

/// Configuration for [ControlService]
pub struct ControlServiceConfig {
    /// Which address to open a port
    pub listener_address: NetAddress,
    /// Optional SOCKS proxy
    pub socks_proxy_address: Option<SocketAddress>,
}

impl Default for ControlServiceConfig {
    fn default() -> Self {
        let listener_address = DEFAULT_LISTENER_ADDRESS.parse::<NetAddress>().unwrap();
        ControlServiceConfig {
            listener_address,
            socks_proxy_address: None,
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
/// # use tari_crypto::{ristretto::{RistrettoSecretKey, RistrettoPublicKey}, keys::{PublicKey, SecretKey}};
///
/// # let secret_key = RistrettoSecretKey::random(&mut rand::OsRng::new().unwrap());
/// # let public_key = RistrettoPublicKey::from_secret_key(&secret_key);
/// # let node_id = NodeId::from_key(&public_key).unwrap();
/// # let node_identity = CommsNodeIdentity {
/// #      identity: PeerNodeIdentity::new(node_id, public_key),
/// #      secret_key,
/// #      control_service_address: "127.0.0.1:9090".parse::<NetAddress>().unwrap(),
/// # };
/// # CommsNodeIdentity::set_global(node_identity);
///
/// let context = Context::new();
/// let listener_address = "127.0.0.1:9000".parse::<NetAddress>().unwrap();
///
/// let peer_manager = Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap());
///
/// let conn_manager = Arc::new(ConnectionManager::new(peer_manager.clone(), PeerConnectionConfig {
///      context: context.clone(),
///      max_message_size: 1024,
///      max_connect_retries: 1,
///      socks_proxy_address: None,
///      consumer_address: InprocAddress::random(),
///      host: "127.0.0.1".parse().unwrap(),
///      control_service_establish_timeout: Duration::from_millis(1000),
///      peer_connection_establish_timeout: Duration::from_secs(4),
/// }));
///
/// let dispatcher = Dispatcher::new(comms_handlers::ControlServiceResolver{})
///     .route(ControlServiceMessageType::EstablishConnection, comms_handlers::establish_connection)
///     .route(ControlServiceMessageType::Accept, comms_handlers::accept)
///     .catch_all(comms_handlers::discard);
///
/// let service = ControlService::new(&context)
///     .configure(ControlServiceConfig {
///         listener_address,
///         socks_proxy_address: None,
///     })
///     .serve(dispatcher, conn_manager)
///     .unwrap();
///
/// service.shutdown().unwrap();
/// ```
pub struct ControlService<'c> {
    context: &'c Context,
    config: ControlServiceConfig,
}

impl<'c> ControlService<'c> {
    pub fn new(context: &'c Context) -> Self {
        Self {
            context,
            config: ControlServiceConfig::default(),
        }
    }

    pub fn configure(mut self, config: ControlServiceConfig) -> Self {
        self.config = config;
        self
    }

    pub fn serve<MType: DispatchableKey, R: DispatchResolver<MType, ControlServiceMessageContext>>(
        self,
        dispatcher: ControlServiceDispatcher<MType, R>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<ControlServiceHandle>
    {
        let config = self.config;
        Ok(ControlServiceWorker::start(self.context.clone(), config, dispatcher, connection_manager)?.into())
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
    use crate::{
        connection::InprocAddress,
        connection_manager::{ConnectionManager, PeerConnectionConfig},
        control_service::types::ControlServiceMessageContext,
        dispatcher::{DispatchError, DispatchResolver, Dispatcher},
        peer_manager::PeerManager,
        test_support::factories::{self, Factory},
        types::{CommsDataStore, CommsPublicKey},
    };
    use std::{sync::mpsc::channel, time::Duration};

    struct TestResolver;

    impl DispatchResolver<u8, ControlServiceMessageContext> for TestResolver {
        fn resolve(&self, _context: &ControlServiceMessageContext) -> std::result::Result<u8, DispatchError> {
            Ok(0u8)
        }
    }

    fn make_connection_manager(context: &Context) -> Arc<ConnectionManager> {
        Arc::new(ConnectionManager::new(make_peer_manager(), PeerConnectionConfig {
            control_service_establish_timeout: Duration::from_millis(1000),
            peer_connection_establish_timeout: Duration::from_secs(4),
            max_message_size: 1024 * 1024,
            socks_proxy_address: None,
            consumer_address: InprocAddress::random(),
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 1,
            context: context.clone(),
        }))
    }

    fn make_peer_manager() -> Arc<PeerManager<CommsPublicKey, CommsDataStore>> {
        Arc::new(PeerManager::<CommsPublicKey, CommsDataStore>::new(None).unwrap())
    }

    #[test]
    fn control_service_has_default() {
        let context = Context::new();
        let control_service = ControlService::new(&context);
        assert_eq!(
            control_service.config.listener_address,
            DEFAULT_LISTENER_ADDRESS.parse::<NetAddress>().unwrap()
        );
        assert!(control_service.config.socks_proxy_address.is_none());
    }

    #[test]
    fn serve_and_shutdown() {
        let (tx, rx) = channel();
        let context = Context::new();
        let connection_manager = make_connection_manager(&context);

        let listener_address = factories::net_address::create().use_os_port().build().unwrap();
        thread::spawn(move || {
            let dispatcher = Dispatcher::new(TestResolver {});

            let service = ControlService::new(&context)
                .configure(ControlServiceConfig {
                    listener_address,
                    socks_proxy_address: None,
                })
                .serve(dispatcher, connection_manager)
                .unwrap();

            service.shutdown().unwrap();
            tx.send(()).unwrap();
        });

        // Test that the control service loop ends within 1000ms
        rx.recv_timeout(Duration::from_millis(1000)).unwrap();
    }
}
