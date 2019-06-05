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

use std::{sync::mpsc::SyncSender, thread};

use crate::{
    connection::{net_address::ip::SocketAddress, Context, NetAddress},
    connection_manager::ConnectionManager,
    dispatcher::{DispatchResolver, DispatchableKey},
    peer_manager::PeerManager,
    types::{CommsPublicKey, DEFAULT_LISTENER_ADDRESS},
};

use super::{
    error::ControlServiceError,
    types::{ControlMessage, ControlServiceDispatcher, ControlServiceMessageContext, Result},
    worker::ControlServiceWorker,
};
use std::sync::Arc;
use tari_storage::lmdb::LMDBStore;

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
/// ```edition2018
/// # use tari_comms::{connection::*, control_service::*, dispatcher::*, connection_manager::*, peer_manager::*, types::*};
/// # use tari_comms::control_service::handlers as comms_handlers;
/// # use std::{time::Duration, sync::Arc};
/// # use tari_storage::lmdb::LMDBStore;
/// # use std::collections::HashMap;
///
/// let context = Context::new();
/// let listener_address = "0.0.0.0:9000".parse::<NetAddress>().unwrap();
///
/// let conn_manager = Arc::new(ConnectionManager::new(&context, PeerConnectionConfig {
///      max_message_size: 1024,
///      max_connect_retries: 1,
///      socks_proxy_address: None,
///      consumer_address: InprocAddress::random(),
///      host: "0.0.0.0".parse().unwrap(),
///      establish_timeout: Duration::from_millis(1000),
/// }));
///
/// let peer_manager = Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap());
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
///     .serve(dispatcher, conn_manager, peer_manager)
///     .unwrap();
///
/// service.shutdown().unwrap();
/// ```
pub struct ControlService<'a> {
    context: &'a Context,
    config: ControlServiceConfig,
}

impl<'a> ControlService<'a> {
    pub fn new(context: &'a Context) -> Self {
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
        peer_manager: Arc<PeerManager<CommsPublicKey, LMDBStore>>,
    ) -> Result<ControlServiceHandle>
    {
        let config = self.config;
        Ok(ControlServiceWorker::start(
            self.context.clone(),
            config,
            dispatcher,
            connection_manager,
            peer_manager,
        )
        .into())
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

impl Drop for ControlServiceHandle {
    /// Ensure the control service shuts down when this handle is dropped
    fn drop(&mut self) {
        let _ = self.shutdown();
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
    };
    use std::{sync::mpsc::channel, time::Duration};

    struct TestResolver;

    impl DispatchResolver<u8, ControlServiceMessageContext> for TestResolver {
        fn resolve(&self, _context: &ControlServiceMessageContext) -> std::result::Result<u8, DispatchError> {
            Ok(0u8)
        }
    }

    fn make_connection_manager(context: &Context) -> Arc<ConnectionManager> {
        Arc::new(ConnectionManager::new(context, PeerConnectionConfig {
            establish_timeout: Duration::from_millis(1000),
            max_message_size: 1024 * 1024,
            socks_proxy_address: None,
            consumer_address: InprocAddress::random(),
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 1,
        }))
    }

    fn make_peer_manager() -> Arc<PeerManager<CommsPublicKey, LMDBStore>> {
        Arc::new(PeerManager::<CommsPublicKey, LMDBStore>::new(None).unwrap())
    }

    #[test]
    fn control_service_has_default() {
        let context = Context::new();
        let dispatcher = Dispatcher::new(TestResolver {});
        let connection_manager = make_connection_manager(&context);
        let peer_manager = make_peer_manager();
        let control_service = ControlService::new(&context);
        assert_eq!(
            control_service.config.listener_address,
            DEFAULT_LISTENER_ADDRESS.parse::<NetAddress>().unwrap()
        );
        assert!(control_service.config.socks_proxy_address.is_none());
        assert!(control_service
            .serve(dispatcher, connection_manager, peer_manager)
            .is_ok());
    }

    #[test]
    fn serve_and_shutdown() {
        let (tx, rx) = channel();
        let context = Context::new();
        let connection_manager = make_connection_manager(&context);
        let peer_manager = make_peer_manager();
        thread::spawn(move || {
            let dispatcher = Dispatcher::new(TestResolver {});

            let service = ControlService::new(&context)
                .configure(ControlServiceConfig {
                    listener_address: "127.0.0.1:9999".parse().unwrap(),
                    socks_proxy_address: None,
                })
                .serve(dispatcher, connection_manager, peer_manager)
                .unwrap();

            service.shutdown().unwrap();
            tx.send(()).unwrap();
        });

        // Test that the control service loop ends within 1000ms
        rx.recv_timeout(Duration::from_millis(1000)).unwrap();
    }
}
