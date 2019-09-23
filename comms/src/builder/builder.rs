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

use crate::{
    connection::{ConnectionError, DealerProxyError, ZmqContext},
    connection_manager::{
        actor::{ConnectionManagerActor, ConnectionManagerRequest},
        ConnectionManager,
        ConnectionManagerRequester,
        PeerConnectionConfig,
    },
    control_service::{ControlService, ControlServiceConfig, ControlServiceError, ControlServiceHandle},
    inbound_message_pipeline::inbound_message_pipeline::InboundMessagePipeline,
    message::{FrameSet, InboundMessage},
    middleware::{IdentityMiddleware, MiddlewareError},
    outbound_message_service::{
        OutboundMessageService,
        OutboundServiceConfig,
        OutboundServiceError,
        OutboundServiceRequester,
    },
    peer_manager::{NodeIdentity, PeerManager, PeerManagerError},
    types::CommsDatabase,
};
use derive_error::Error;
use futures::channel::mpsc;
use log::*;
use std::{fmt::Debug, sync::Arc};
use tokio::runtime::TaskExecutor;
use tower::Service;

const LOG_TARGET: &str = "comms::builder";

#[derive(Debug, Error)]
pub enum CommsBuilderError {
    PeerManagerError(PeerManagerError),
    InboundMessageServiceError(ConnectionError),
    OutboundServiceError(OutboundServiceError),
    /// Node identity not set. Call `with_node_identity(node_identity)` on [CommsBuilder]
    NodeIdentityNotSet,
    #[error(no_from)]
    DealerProxyError(DealerProxyError),
    DatastoreUndefined,
}

#[derive(Clone)]
pub struct CommsBuilderConfig {
    inbound_message_buffer_size: usize,
    inbound_message_sink_buffer_size: usize,
}

impl Default for CommsBuilderConfig {
    fn default() -> Self {
        Self {
            inbound_message_buffer_size: 1000,
            inbound_message_sink_buffer_size: 1000,
        }
    }
}

type CommsConnectionManagerActor = ConnectionManagerActor<ConnectionManager, mpsc::Receiver<ConnectionManagerRequest>>;

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
///
/// The [build] method will return an error if any required builder methods are not called. These
/// are detailed further down on the method docs.
pub struct CommsBuilder<TInMiddlewareBuilder> {
    zmq_context: ZmqContext,
    peer_storage: Option<CommsDatabase>,
    control_service_config: Option<ControlServiceConfig>,
    outbound_service_config: Option<OutboundServiceConfig>,
    inbound_middleware_builder: TInMiddlewareBuilder,
    node_identity: Option<Arc<NodeIdentity>>,
    peer_conn_config: Option<PeerConnectionConfig>,
    comms_builder_config: Option<CommsBuilderConfig>,
    executor: TaskExecutor,
}

impl CommsBuilder<Box<dyn FnOnce(CommsServices) -> IdentityMiddleware>> {
    /// Create a new CommsBuilder
    pub fn new(executor: TaskExecutor) -> Self {
        let zmq_context = ZmqContext::new();

        Self {
            zmq_context,
            control_service_config: None,
            peer_conn_config: None,
            inbound_middleware_builder: Box::new(|_| IdentityMiddleware::new()),
            outbound_service_config: None,
            peer_storage: None,
            node_identity: None,
            comms_builder_config: None,
            executor,
        }
    }
}

impl<TInMiddlewareBuilder, TInMiddleware> CommsBuilder<TInMiddlewareBuilder>
where
    TInMiddlewareBuilder: FnOnce(CommsServices) -> TInMiddleware,
    TInMiddleware: Service<InboundMessage, Response = (), Error = MiddlewareError> + Send + Unpin + 'static,
{
    /// Set the [NodeIdentity] for this comms instance. This is required.
    ///
    /// [OutboundMessagePool]: ../../outbound_message_service/index.html#outbound-message-pool
    pub fn with_node_identity(mut self, node_identity: Arc<NodeIdentity>) -> Self {
        self.node_identity = Some(node_identity);
        self
    }

    /// Set the peer storage database to use. This is optional.
    pub fn with_peer_storage(mut self, peer_storage: CommsDatabase) -> Self {
        self.peer_storage = Some(peer_storage);
        self
    }

    /// Configure inbound message publisher/subscriber buffer size. This is optional
    pub fn configure_comms_builder_config(mut self, config: CommsBuilderConfig) -> Self {
        self.comms_builder_config = Some(config);
        self
    }

    /// Configure the [ControlService]. This is optional.
    ///
    /// [ControlService]: ../../control_service/index.html
    pub fn configure_control_service(mut self, config: ControlServiceConfig) -> Self {
        self.control_service_config = Some(config);
        self
    }

    /// Configure the [OutboundService]. This is optional. If omitted the default configuration is used.
    ///
    /// [OutboundService]: ../../outbound_service/index.html#outbound-service
    pub fn configure_outbound_service(mut self, config: OutboundServiceConfig) -> Self {
        self.outbound_service_config = Some(config);
        self
    }

    /// Common configuration for all [PeerConnection]s. This is optional.
    /// If omitted the default configuration is used.
    ///
    /// [PeerConnection]: ../../connection/peer_connection/index.html
    pub fn configure_peer_connections(mut self, config: PeerConnectionConfig) -> Self {
        self.peer_conn_config = Some(config);
        self
    }

    pub fn with_inbound_middleware<B, M>(self, inbound_middleware_builder: B) -> CommsBuilder<B>
    where
        B: FnOnce(CommsServices) -> M,
        M: Service<InboundMessage, Response = (), Error = MiddlewareError> + Send + Unpin + 'static,
    {
        CommsBuilder {
            inbound_middleware_builder,
            // This unofficial RFC would avoid repeated fields.
            // https://github.com/jturner314/rust-rfcs/blob/type-changing-struct-update-syntax/text/0000-type-changing-struct-update-syntax.md
            zmq_context: self.zmq_context,
            control_service_config: self.control_service_config,
            peer_conn_config: self.peer_conn_config,
            outbound_service_config: self.outbound_service_config,
            peer_storage: self.peer_storage,
            node_identity: self.node_identity,
            comms_builder_config: self.comms_builder_config,
            executor: self.executor,
        }
    }

    fn make_peer_manager(&mut self) -> Result<Arc<PeerManager>, CommsBuilderError> {
        match self.peer_storage.take() {
            Some(storage) => {
                let peer_manager = PeerManager::new(storage).map_err(CommsBuilderError::PeerManagerError)?;
                Ok(Arc::new(peer_manager))
            },
            None => Err(CommsBuilderError::DatastoreUndefined),
        }
    }

    fn make_control_service(&mut self, node_identity: Arc<NodeIdentity>) -> Option<ControlService> {
        self.control_service_config
            .take()
            .map(|config| ControlService::new(self.zmq_context.clone(), node_identity, config))
    }

    fn make_connection_manager(
        &mut self,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        config: PeerConnectionConfig,
        message_sink_sender: mpsc::Sender<FrameSet>,
    ) -> Arc<ConnectionManager>
    {
        Arc::new(ConnectionManager::new(
            self.zmq_context.clone(),
            node_identity,
            peer_manager,
            config,
            message_sink_sender,
        ))
    }

    fn make_connection_manager_actor(
        &mut self,
        connection_manager: Arc<ConnectionManager>,
    ) -> (ConnectionManagerRequester, CommsConnectionManagerActor)
    {
        let (tx, rx) = mpsc::channel(10);
        let requester = ConnectionManagerRequester::new(tx);
        let actor = ConnectionManagerActor::new(connection_manager, rx);

        (requester, actor)
    }

    fn make_peer_connection_config(&mut self) -> PeerConnectionConfig {
        let config = self.peer_conn_config.take().unwrap_or_default();
        config
    }

    fn make_node_identity(&mut self) -> Result<Arc<NodeIdentity>, CommsBuilderError> {
        self.node_identity.take().ok_or(CommsBuilderError::NodeIdentityNotSet)
    }

    fn make_outbound_message_service(
        &mut self,
        executor: TaskExecutor,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        conn_manager: ConnectionManagerRequester,
    ) -> (OutboundServiceRequester, OutboundMessageService)
    {
        let (tx, rx) = mpsc::unbounded();

        let requester = OutboundServiceRequester::new(tx);
        let service = OutboundMessageService::new(
            self.outbound_service_config.take().unwrap_or_default(),
            executor,
            rx,
            peer_manager,
            conn_manager,
            node_identity,
        );

        (requester, service)
    }

    /// Build the required comms services. Services will not be started.
    pub fn build(mut self) -> Result<CommsContainer<TInMiddleware>, CommsBuilderError> {
        let config = self.comms_builder_config.clone().unwrap_or_default();

        let node_identity = self.make_node_identity()?;

        //---------------------------------- Peer Manager --------------------------------------------//
        let peer_manager = self.make_peer_manager()?;

        let peer_conn_config = self.make_peer_connection_config();

        //---------------------------------- Control Service --------------------------------------------//

        let control_service = self.make_control_service(node_identity.clone());

        //---------------------------------- ConnectionManager --------------------------------------------//
        // Channel used for sending FrameSets from PeerConnections to IMS
        let (peer_connection_message_sender, peer_connection_message_receiver) =
            mpsc::channel(config.inbound_message_sink_buffer_size);
        let connection_manager = self.make_connection_manager(
            node_identity.clone(),
            peer_manager.clone(),
            peer_conn_config.clone(),
            peer_connection_message_sender,
        );
        let (connection_manager_requester, connection_manager_actor) =
            self.make_connection_manager_actor(Arc::clone(&connection_manager));

        //---------------------------------- Outbound message service --------------------------------------------//
        let (outbound_service_requester, outbound_message_service) = self.make_outbound_message_service(
            self.executor.clone(),
            Arc::clone(&node_identity),
            Arc::clone(&peer_manager),
            connection_manager_requester.clone(),
        );

        //---------------------------------- Inbound message pipeline --------------------------------------------//
        let inbound_middleware = (self.inbound_middleware_builder)(CommsServices {
            peer_manager: Arc::clone(&peer_manager),
            connection_manager_requester: connection_manager_requester.clone(),
            node_identity: Arc::clone(&node_identity),
            outbound_service_requester: outbound_service_requester.clone(),
        });
        let inbound_message_pipeline = InboundMessagePipeline::new(
            peer_connection_message_receiver,
            inbound_middleware,
            Arc::clone(&peer_manager),
        );

        Ok(CommsContainer {
            connection_manager,
            connection_manager_actor,
            connection_manager_requester,
            control_service,
            executor: self.executor,
            inbound_message_pipeline,
            node_identity,
            outbound_service_requester,
            outbound_message_service,
            peer_manager,
        })
    }
}

pub struct CommsServices {
    pub peer_manager: Arc<PeerManager>,
    pub connection_manager_requester: ConnectionManagerRequester,
    pub outbound_service_requester: OutboundServiceRequester,
    pub node_identity: Arc<NodeIdentity>,
}

#[derive(Debug, Error)]
pub enum CommsError {
    ControlServiceError(ControlServiceError),
    ConnectionManagerError(ConnectionError),
    /// Comms services shut down uncleanly
    UncleanShutdown,
    /// The message type was not registered
    MessageTypeNotRegistered,
}

/// Contains the built comms services
pub struct CommsContainer<TInMiddleware> {
    connection_manager: Arc<ConnectionManager>,
    connection_manager_actor: CommsConnectionManagerActor,
    connection_manager_requester: ConnectionManagerRequester,

    control_service: Option<ControlService>,

    executor: TaskExecutor,

    inbound_message_pipeline: InboundMessagePipeline<TInMiddleware>,

    node_identity: Arc<NodeIdentity>,

    outbound_service_requester: OutboundServiceRequester,
    outbound_message_service: OutboundMessageService,

    peer_manager: Arc<PeerManager>,
}

impl<TInMiddleware> CommsContainer<TInMiddleware>
where
    TInMiddleware: Service<InboundMessage, Response = (), Error = MiddlewareError> + Send + Unpin + 'static,
    TInMiddleware::Future: Send,
{
    /// Start all the comms services and return a [CommsServices] object
    ///
    /// [CommsServices]: ./struct.CommsServices.html
    pub fn start(self) -> Result<CommsNode, CommsError> {
        let mut control_service_handle = None;
        if let Some(control_service) = self.control_service {
            control_service_handle = Some(
                control_service
                    .serve(Arc::clone(&self.connection_manager))
                    .map_err(CommsError::ControlServiceError)?,
            );
        }

        self.executor.spawn(self.connection_manager_actor.start());
        self.executor.spawn(self.outbound_message_service.start());
        self.executor.spawn(self.inbound_message_pipeline.run());

        Ok(CommsNode {
            connection_manager: self.connection_manager,
            connection_manager_requester: self.connection_manager_requester,
            control_service_handle,
            node_identity: self.node_identity,
            outbound_service_requester: self.outbound_service_requester,
            peer_manager: self.peer_manager,
        })
    }
}

/// # CommsNode
///
/// This struct provides a handle to and control over all the running comms services.
/// You can get a [DomainConnector] from which to receive messages by using the `create_connector`
/// method. Use the `shutdown` method to attempt to cleanly shut all comms services down.
pub struct CommsNode {
    connection_manager: Arc<ConnectionManager>,
    connection_manager_requester: ConnectionManagerRequester,
    control_service_handle: Option<ControlServiceHandle>,
    node_identity: Arc<NodeIdentity>,
    outbound_service_requester: OutboundServiceRequester,
    peer_manager: Arc<PeerManager>,
}

impl CommsNode {
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    pub fn outbound_message_service(&self) -> OutboundServiceRequester {
        self.outbound_service_requester.clone()
    }

    pub fn shutdown(self) -> Result<(), CommsError> {
        info!(target: LOG_TARGET, "Comms is shutting down");

        // This shuts down the ConnectionManagerActor (releasing Arc<ConnectionManager>)
        drop(self.connection_manager_requester);
        drop(self.outbound_service_requester);

        let mut shutdown_results = Vec::new();
        // Shutdown control service
        if let Some(control_service_shutdown_result) = self.control_service_handle.map(|hnd| hnd.shutdown()) {
            shutdown_results.push(control_service_shutdown_result.map_err(CommsError::ControlServiceError));
        }

        // Lastly, Shutdown connection manager
        match Arc::try_unwrap(self.connection_manager) {
            Ok(conn_manager) => {
                for result in conn_manager.shutdown() {
                    shutdown_results.push(result.map_err(CommsError::ConnectionManagerError));
                }
            },
            Err(_) => error!(
                target: LOG_TARGET,
                "Unable to cleanly shutdown connection manager because references are still held by other threads"
            ),
        }

        Self::check_clean_shutdown(shutdown_results)
    }

    fn check_clean_shutdown(results: Vec<Result<(), CommsError>>) -> Result<(), CommsError> {
        let mut has_error = false;
        for result in results {
            if let Err(err) = result {
                error!(target: LOG_TARGET, "Error occurred when shutting down {:?}", err);
                has_error = true;
            }
        }

        if has_error {
            Err(CommsError::UncleanShutdown)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_storage::HMapDatabase;
    use tokio::runtime::Runtime;

    #[test]
    fn new_no_control_service() {
        let rt = Runtime::new().unwrap();
        let container = CommsBuilder::new(rt.executor())
            .with_node_identity(Arc::new(NodeIdentity::random_for_test(None)))
            .with_peer_storage(HMapDatabase::new())
            .build()
            .unwrap();

        assert!(container.control_service.is_none());
    }

    #[test]
    fn new_with_control_service() {
        let rt = Runtime::new().unwrap();
        let container = CommsBuilder::new(rt.executor())
            .with_node_identity(Arc::new(NodeIdentity::random_for_test(None)))
            .with_peer_storage(HMapDatabase::new())
            .configure_control_service(ControlServiceConfig::default())
            .build()
            .unwrap();

        assert!(container.control_service.is_some());
    }
}
