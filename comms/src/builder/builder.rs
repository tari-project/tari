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
    backoff::{Backoff, BoxedBackoff, ExponentialBackoff},
    builder::null_sink::NullSink,
    connection::{ConnectionError, PeerConnectionError, ZmqContext},
    connection_manager::{
        actor::{ConnectionManagerActor, ConnectionManagerRequest},
        ConnectionManager,
        ConnectionManagerDialer,
        ConnectionManagerError,
        ConnectionManagerRequester,
        PeerConnectionConfig,
    },
    control_service::{ControlService, ControlServiceConfig, ControlServiceError, ControlServiceHandle},
    inbound_message_service::inbound_message_service::InboundMessageService,
    message::{FrameSet, InboundMessage},
    outbound_message_service::{
        OutboundEventPublisher,
        OutboundEventSubscription,
        OutboundMessage,
        OutboundMessageService,
        OutboundServiceConfig,
        OutboundServiceError,
    },
    peer_manager::{NodeIdentity, PeerManager, PeerManagerError},
    types::CommsDatabase,
};
use derive_error::Error;
use futures::{channel::mpsc, stream, Sink, Stream};
use log::*;
use std::{fmt::Debug, sync::Arc};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime, sync::broadcast};

const LOG_TARGET: &str = "comms::builder";

#[derive(Debug, Error)]
pub enum CommsBuilderError {
    PeerManagerError(PeerManagerError),
    InboundMessageServiceError(ConnectionError),
    OutboundServiceError(OutboundServiceError),
    /// Node identity not set. Call `with_node_identity(node_identity)` on [CommsBuilder]
    NodeIdentityNotSet,
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

type CommsConnectionManagerActor =
    ConnectionManagerActor<ConnectionManagerDialer, mpsc::Receiver<ConnectionManagerRequest>>;

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
///
/// The [build] method will return an error if any required builder methods are not called. These
/// are detailed further down on the method docs.
pub struct CommsBuilder<TInSink, TOutStream> {
    zmq_context: ZmqContext,
    peer_storage: Option<CommsDatabase>,
    control_service_config: Option<ControlServiceConfig>,
    outbound_service_config: Option<OutboundServiceConfig>,
    inbound_sink: Option<TInSink>,
    outbound_stream: Option<TOutStream>,
    node_identity: Option<Arc<NodeIdentity>>,
    peer_conn_config: Option<PeerConnectionConfig>,
    comms_builder_config: Option<CommsBuilderConfig>,
    executor: runtime::Handle,
    oms_backoff: Option<BoxedBackoff>,
    on_shutdown: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl CommsBuilder<NullSink<InboundMessage, mpsc::SendError>, stream::Empty<OutboundMessage>> {
    /// Create a new CommsBuilder
    pub fn new(executor: runtime::Handle) -> Self {
        let zmq_context = ZmqContext::new();

        Self {
            zmq_context,
            control_service_config: None,
            peer_conn_config: None,
            inbound_sink: Some(NullSink::new()),
            outbound_stream: Some(stream::empty()),
            outbound_service_config: None,
            peer_storage: None,
            node_identity: None,
            comms_builder_config: None,
            on_shutdown: None,
            oms_backoff: Some(Box::new(ExponentialBackoff::default())),
            executor,
        }
    }
}

impl<TInSink, TOutStream> CommsBuilder<TInSink, TOutStream>
where
    TInSink: Sink<InboundMessage, Error = mpsc::SendError> + Unpin + Send + 'static,
    TOutStream: Stream<Item = OutboundMessage> + Send + Sync + Unpin + 'static,
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

    /// Set the backoff for the [OutboundService]. This is optional. If omitted the default ExponentialBackoff is used.
    ///
    /// [OutboundService]: ../../outbound_service/index.html#outbound-service
    pub fn with_outbound_backoff<T>(mut self, backoff: T) -> Self
    where T: Backoff + Send + Sync + 'static {
        self.oms_backoff = Some(Box::new(backoff));
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

    /// Set the sink to use to consume all inbound messages
    pub fn with_inbound_sink<S>(self, sink: S) -> CommsBuilder<S, TOutStream>
    where S: Sink<InboundMessage> + Send + 'static {
        CommsBuilder {
            inbound_sink: Some(sink),
            // This unofficial RFC would avoid repeated fields.
            // https://github.com/jturner314/rust-rfcs/blob/type-changing-struct-update-syntax/text/0000-type-changing-struct-update-syntax.md
            zmq_context: self.zmq_context,
            control_service_config: self.control_service_config,
            peer_conn_config: self.peer_conn_config,
            outbound_service_config: self.outbound_service_config,
            outbound_stream: self.outbound_stream,
            peer_storage: self.peer_storage,
            node_identity: self.node_identity,
            comms_builder_config: self.comms_builder_config,
            executor: self.executor,
            oms_backoff: self.oms_backoff,
            on_shutdown: self.on_shutdown,
        }
    }

    /// Set the stream which emits messages to be sent by the OMS
    pub fn with_outbound_stream<S>(self, stream: S) -> CommsBuilder<TInSink, S>
    where S: Stream<Item = OutboundMessage> + Send + 'static {
        CommsBuilder {
            outbound_stream: Some(stream),
            // This unofficial RFC would avoid repeated fields.
            // https://github.com/jturner314/rust-rfcs/blob/type-changing-struct-update-syntax/text/0000-type-changing-struct-update-syntax.md
            zmq_context: self.zmq_context,
            inbound_sink: self.inbound_sink,
            control_service_config: self.control_service_config,
            peer_conn_config: self.peer_conn_config,
            outbound_service_config: self.outbound_service_config,
            peer_storage: self.peer_storage,
            node_identity: self.node_identity,
            comms_builder_config: self.comms_builder_config,
            executor: self.executor,
            oms_backoff: self.oms_backoff,
            on_shutdown: self.on_shutdown,
        }
    }

    pub fn on_shutdown<F>(mut self, on_shutdown: F) -> Self
    where F: FnOnce() + Send + Sync + 'static {
        self.on_shutdown = Some(Box::new(on_shutdown));
        self
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
        shutdown_signal: ShutdownSignal,
    ) -> (ConnectionManagerRequester, CommsConnectionManagerActor)
    {
        let (tx, rx) = mpsc::channel(10);
        let requester = ConnectionManagerRequester::new(tx);
        let actor = ConnectionManagerActor::new(ConnectionManagerDialer::new(connection_manager), rx, shutdown_signal);

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
        node_identity: Arc<NodeIdentity>,
        connection_manager_requester: ConnectionManagerRequester,
        shutdown_signal: ShutdownSignal,
    ) -> (OutboundMessageService<TOutStream, BoxedBackoff>, OutboundEventPublisher)
    {
        let outbound_stream = self.outbound_stream.take().expect("outbound_stream cannot be None");
        let oms_backoff = self.oms_backoff.take().expect("oms_backoff was None");
        let (event_tx, _) = broadcast::channel(10);
        (
            OutboundMessageService::with_backoff(
                self.outbound_service_config.take().unwrap_or_default(),
                outbound_stream,
                node_identity,
                connection_manager_requester,
                shutdown_signal,
                oms_backoff,
                event_tx.clone(),
            ),
            event_tx,
        )
    }

    /// Build the required comms services. Services will not be started.
    pub fn build(mut self) -> Result<CommsContainer<TInSink, TOutStream>, CommsBuilderError> {
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

        let mut shutdown = Shutdown::new();

        if let Some(on_shutdown) = self.on_shutdown.take() {
            shutdown.on_triggered(on_shutdown);
        }

        let (connection_manager_requester, connection_manager_actor) =
            self.make_connection_manager_actor(Arc::clone(&connection_manager), shutdown.to_signal());

        let (outbound_message_service, outbound_event_publisher) = self.make_outbound_message_service(
            Arc::clone(&node_identity),
            connection_manager_requester.clone(),
            shutdown.to_signal(),
        );

        //---------------------------------- Inbound message pipeline --------------------------------------------//
        let inbound_message_service = InboundMessageService::new(
            peer_connection_message_receiver,
            self.inbound_sink.take().expect("inbound_sink cannot be None"),
            Arc::clone(&peer_manager),
            shutdown.to_signal(),
        );

        Ok(CommsContainer {
            connection_manager,
            connection_manager_actor,
            control_service,
            executor: self.executor,
            shutdown,
            inbound_message_service,
            node_identity,
            outbound_message_service,
            outbound_event_publisher,
            peer_manager,
        })
    }
}

#[derive(Debug, Error)]
pub enum CommsError {
    ControlServiceError(ControlServiceError),
    PeerConnectionError(PeerConnectionError),
    ConnectionManagerError(ConnectionManagerError),
    /// Comms services shut down uncleanly
    UncleanShutdown,
    /// The message type was not registered
    MessageTypeNotRegistered,
    /// Failed to send shutdown signals
    FailedSendShutdownSignals,
}

/// Contains the built comms services
pub struct CommsContainer<TInSink, TOutStream> {
    connection_manager: Arc<ConnectionManager>,
    connection_manager_actor: CommsConnectionManagerActor,
    control_service: Option<ControlService>,

    executor: runtime::Handle,

    inbound_message_service: InboundMessageService<TInSink>,

    node_identity: Arc<NodeIdentity>,

    outbound_message_service: OutboundMessageService<TOutStream, BoxedBackoff>,
    outbound_event_publisher: OutboundEventPublisher,

    shutdown: Shutdown,

    peer_manager: Arc<PeerManager>,
}

impl<TInSink, TOutStream> CommsContainer<TInSink, TOutStream>
where
    TInSink: Sink<InboundMessage, Error = mpsc::SendError> + Unpin + Send + 'static,
    TOutStream: Stream<Item = OutboundMessage> + Unpin + Send + Sync + 'static,
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
        self.connection_manager.run_listener()?;

        self.executor.spawn(self.connection_manager_actor.run());
        self.executor.spawn(self.outbound_message_service.start());
        self.executor.spawn(self.inbound_message_service.run());

        Ok(CommsNode {
            connection_manager: self.connection_manager,
            outbound_event_publisher: self.outbound_event_publisher,
            executor: self.executor,
            shutdown: self.shutdown,
            control_service_handle,
            node_identity: self.node_identity,
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
    control_service_handle: Option<ControlServiceHandle>,
    shutdown: Shutdown,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    executor: runtime::Handle,
    outbound_event_publisher: OutboundEventPublisher,
}

impl CommsNode {
    /// Return a cloned atomic reference of the PeerManager
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    /// Return a subscription to OMS events. This will emit events sent _after_ this subscription was created.
    pub fn outbound_event_subscription(&self) -> OutboundEventSubscription {
        self.outbound_event_publisher.subscribe()
    }

    /// Return a reference to the executor used to run comms tasks
    pub fn executor(&self) -> &runtime::Handle {
        &self.executor
    }

    /// Returns a new `ShutdownSignal`
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown.to_signal()
    }

    /// Shuts comms down. This function returns an error if any of the services failed to shutdown
    pub fn shutdown(mut self) -> Result<(), CommsError> {
        info!(target: LOG_TARGET, "Comms is shutting down");

        let mut shutdown_results = Vec::new();

        // Send shutdown signals and wait for shutdown
        shutdown_results.push(
            self.shutdown
                .trigger()
                .map_err(|_| CommsError::FailedSendShutdownSignals),
        );

        // Shutdown control service
        if let Some(control_service_shutdown_result) = self.control_service_handle.map(|hnd| hnd.shutdown()) {
            shutdown_results.push(control_service_shutdown_result.map_err(CommsError::ControlServiceError));
        }

        // Lastly, Shutdown connection manager
        for result in self.connection_manager.shutdown() {
            shutdown_results.push(result.map_err(Into::into));
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
    use crate::peer_manager::PeerFeatures;
    use tari_storage::HashmapDatabase;
    use tokio::runtime::Runtime;

    #[test]
    fn new_no_control_service() {
        let rt = Runtime::new().unwrap();
        let container = CommsBuilder::new(rt.handle().clone())
            .with_node_identity(Arc::new(NodeIdentity::random_for_test(None, PeerFeatures::empty())))
            .with_peer_storage(HashmapDatabase::new())
            .build()
            .unwrap();

        assert!(container.control_service.is_none());
    }

    #[test]
    fn new_with_control_service() {
        let rt = Runtime::new().unwrap();
        let container = CommsBuilder::new(rt.handle().clone())
            .with_node_identity(Arc::new(NodeIdentity::random_for_test(None, PeerFeatures::empty())))
            .with_peer_storage(HashmapDatabase::new())
            .configure_control_service(ControlServiceConfig::default())
            .build()
            .unwrap();

        assert!(container.control_service.is_some());
    }
}
