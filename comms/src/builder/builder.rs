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
    connection::{ConnectionError, DealerProxyError, InprocAddress, ZmqContext},
    connection_manager::{ConnectionManager, PeerConnectionConfig},
    consts::COMMS_BUILDER_IMS_DEFAULT_PUB_SUB_BUFFER_LENGTH,
    control_service::{ControlService, ControlServiceConfig, ControlServiceError, ControlServiceHandle},
    dispatcher::DispatchableKey,
    inbound_message_service::{
        comms_msg_handlers::construct_comms_msg_dispatcher,
        error::InboundError,
        inbound_message_publisher::{InboundMessagePublisher, PublisherError},
        inbound_message_service::{InboundMessageService, InboundMessageServiceConfig},
        InboundTopicSubscriptionFactory,
    },
    message::InboundMessage,
    outbound_message_service::{
        outbound_message_pool::{OutboundMessagePoolConfig, OutboundMessagePoolError},
        outbound_message_service::OutboundMessageService,
        OutboundError,
        OutboundMessage,
        OutboundMessagePool,
    },
    peer_manager::{NodeIdentity, PeerManager, PeerManagerError},
    pub_sub_channel::{pubsub_channel, TopicPublisher},
    types::CommsDatabase,
};
use bitflags::_core::marker::PhantomData;
use crossbeam_channel::Sender;
use derive_error::Error;
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::Debug,
    sync::{Arc, RwLock},
};

const LOG_TARGET: &str = "comms::builder";

#[derive(Debug, Error)]
pub enum CommsBuilderError {
    PeerManagerError(PeerManagerError),
    InboundMessageServiceError(ConnectionError),
    #[error(no_from)]
    OutboundMessageServiceError(OutboundError),
    #[error(no_from)]
    OutboundMessagePoolError(OutboundError),
    /// Node identity not set. Call `with_node_identity(node_identity)` on [CommsBuilder]
    NodeIdentityNotSet,
    #[error(no_from)]
    DealerProxyError(DealerProxyError),
    DatastoreUndefined,
}

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
///
/// The [build] method will return an error if any required builder methods are not called. These
/// are detailed further down on the method docs.
#[derive(Default)]
pub struct CommsBuilder<MType> {
    zmq_context: ZmqContext,
    peer_storage: Option<CommsDatabase>,
    control_service_config: Option<ControlServiceConfig>,
    omp_config: Option<OutboundMessagePoolConfig>,
    ims_config: Option<InboundMessageServiceConfig>,
    node_identity: Option<NodeIdentity>,
    peer_conn_config: Option<PeerConnectionConfig>,
    inbound_message_buffer_size: Option<usize>,
    _m: PhantomData<MType>,
}

impl<MType> CommsBuilder<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
    MType: Clone + Debug,
{
    /// Create a new CommsBuilder
    pub fn new() -> Self {
        let zmq_context = ZmqContext::new();

        Self {
            zmq_context,
            control_service_config: None,
            peer_conn_config: None,
            omp_config: None,
            ims_config: None,
            peer_storage: None,
            node_identity: None,
            inbound_message_buffer_size: None,
            _m: PhantomData,
        }
    }

    /// Set the [NodeIdentity] for this comms instance. This is required.
    ///
    /// [OutboundMessagePool]: ../../outbound_message_service/index.html#outbound-message-pool
    pub fn with_node_identity(mut self, node_identity: NodeIdentity) -> Self {
        self.node_identity = Some(node_identity);
        self
    }

    /// Set the peer storage database to use. This is optional.
    pub fn with_peer_storage(mut self, peer_storage: CommsDatabase) -> Self {
        self.peer_storage = Some(peer_storage);
        self
    }

    /// Configure inbound message publisher/subscriber buffer size. This is optional
    pub fn configure_inbound_message_publisher_buffer_size(mut self, size: usize) -> Self {
        self.inbound_message_buffer_size = Some(size);
        self
    }

    /// Configure the [ControlService]. This is optional.
    ///
    /// [ControlService]: ../../control_service/index.html
    pub fn configure_control_service(mut self, config: ControlServiceConfig) -> Self {
        self.control_service_config = Some(config);
        self
    }

    /// Configure the [OutboundMessagePool]. This is optional. If omitted the default configuration is used.
    ///
    /// [OutboundMessagePool]: ../../outbound_message_service/index.html#outbound-message-pool
    pub fn configure_outbound_message_pool(mut self, config: OutboundMessagePoolConfig) -> Self {
        self.omp_config = Some(config);
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
    ) -> Arc<ConnectionManager>
    {
        Arc::new(ConnectionManager::new(
            self.zmq_context.clone(),
            node_identity,
            peer_manager,
            config,
        ))
    }

    fn make_peer_connection_config(&mut self) -> PeerConnectionConfig {
        let mut config = self.peer_conn_config.take().unwrap_or_default();
        // If the message_sink_address is not set (is default) set it to a random inproc address
        if config.message_sink_address.is_default() {
            config.message_sink_address = InprocAddress::random();
        }
        config
    }

    fn make_node_identity(&mut self) -> Result<Arc<NodeIdentity>, CommsBuilderError> {
        self.node_identity
            .take()
            .map(Arc::new)
            .ok_or(CommsBuilderError::NodeIdentityNotSet)
    }

    fn make_outbound_message_service(
        &self,
        node_identity: Arc<NodeIdentity>,
        message_sink: Sender<OutboundMessage>,
        peer_manager: Arc<PeerManager>,
    ) -> Result<Arc<OutboundMessageService>, CommsBuilderError>
    {
        OutboundMessageService::new(node_identity, message_sink, peer_manager)
            .map(Arc::new)
            .map_err(CommsBuilderError::OutboundMessageServiceError)
    }

    fn make_outbound_message_pool(
        &mut self,
        peer_manager: Arc<PeerManager>,
        connection_manager: Arc<ConnectionManager>,
    ) -> OutboundMessagePool
    {
        let config = self.omp_config.take().unwrap_or_default();

        OutboundMessagePool::new(config, peer_manager, connection_manager)
    }

    // TODO Remove this Arc + RwLock when the IMS worker is refactored to be future based.
    fn make_inbound_message_publisher(
        &mut self,
        publisher: TopicPublisher<MType, InboundMessage>,
    ) -> Arc<RwLock<InboundMessagePublisher<MType, InboundMessage>>>
    {
        Arc::new(RwLock::new(InboundMessagePublisher::new(publisher)))
    }

    fn make_inbound_message_service(
        &mut self,
        node_identity: Arc<NodeIdentity>,
        message_sink_address: InprocAddress,
        inbound_message_publisher: Arc<RwLock<InboundMessagePublisher<MType, InboundMessage>>>,
        oms: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager>,
    ) -> InboundMessageService<MType>
    {
        let config = self.ims_config.take().unwrap_or_default();

        InboundMessageService::new(
            config,
            self.zmq_context.clone(),
            node_identity,
            message_sink_address,
            Arc::new(construct_comms_msg_dispatcher()),
            inbound_message_publisher,
            oms,
            peer_manager,
        )
    }

    /// Build the required comms services. Services will not be started.
    pub fn build(mut self) -> Result<CommsServiceContainer<MType>, CommsBuilderError> {
        let node_identity = self.make_node_identity()?;

        let peer_manager = self.make_peer_manager()?;

        let peer_conn_config = self.make_peer_connection_config();

        let control_service = self.make_control_service(node_identity.clone());

        let connection_manager =
            self.make_connection_manager(node_identity.clone(), peer_manager.clone(), peer_conn_config.clone());

        let outbound_message_pool = self.make_outbound_message_pool(peer_manager.clone(), connection_manager.clone());

        let outbound_message_service = self.make_outbound_message_service(
            node_identity.clone(),
            outbound_message_pool.sender(),
            peer_manager.clone(),
        )?;

        // Create pub/sub channel for IMS
        let (publisher, inbound_message_subscription_factory) = pubsub_channel(
            self.inbound_message_buffer_size
                .or(Some(COMMS_BUILDER_IMS_DEFAULT_PUB_SUB_BUFFER_LENGTH))
                .unwrap(),
        );
        let inbound_message_publisher = self.make_inbound_message_publisher(publisher);

        let inbound_message_service = self.make_inbound_message_service(
            node_identity.clone(),
            peer_conn_config.message_sink_address,
            inbound_message_publisher,
            outbound_message_service.clone(),
            peer_manager.clone(),
        );

        Ok(CommsServiceContainer {
            zmq_context: self.zmq_context,
            control_service,
            inbound_message_service,
            connection_manager,
            outbound_message_pool,
            outbound_message_service,
            peer_manager,
            node_identity,
            inbound_message_subscription_factory: Arc::new(inbound_message_subscription_factory),
        })
    }
}

#[derive(Debug, Error)]
pub enum CommsServicesError {
    ControlServiceError(ControlServiceError),
    ConnectionManagerError(ConnectionError),
    /// Comms services shut down uncleanly
    UncleanShutdown,
    /// The message type was not registered
    MessageTypeNotRegistered,
    OutboundMessagePoolError(OutboundMessagePoolError),
    OutboundError(OutboundError),
    InboundMessageServiceError(InboundError),
    PublisherError(PublisherError),
}

/// Contains the built comms services
pub struct CommsServiceContainer<MType>
where
    MType: Serialize + DeserializeOwned,
    MType: DispatchableKey,
    MType: Clone + Debug,
{
    zmq_context: ZmqContext,
    connection_manager: Arc<ConnectionManager>,
    control_service: Option<ControlService>,
    inbound_message_service: InboundMessageService<MType>,
    outbound_message_pool: OutboundMessagePool,
    outbound_message_service: Arc<OutboundMessageService>,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    inbound_message_subscription_factory: Arc<InboundTopicSubscriptionFactory<MType>>,
}

impl<MType> CommsServiceContainer<MType>
where
    MType: Serialize + DeserializeOwned,
    MType: DispatchableKey,
    MType: Clone + Send + Debug,
{
    /// Start all the comms services and return a [CommsServices] object
    ///
    /// [CommsServices]: ./struct.CommsServices.html
    pub fn start(mut self) -> Result<CommsServices<MType>, CommsServicesError> {
        let mut control_service_handle = None;
        if let Some(control_service) = self.control_service {
            control_service_handle = Some(
                control_service
                    .serve(self.connection_manager.clone())
                    .map_err(CommsServicesError::ControlServiceError)?,
            );
        }

        self.inbound_message_service
            .start()
            .map_err(CommsServicesError::InboundMessageServiceError)?;
        self.outbound_message_pool
            .start()
            .map_err(CommsServicesError::OutboundMessagePoolError)?;

        Ok(CommsServices {
            // Transfer ownership to CommsServices
            zmq_context: self.zmq_context,
            outbound_message_service: self.outbound_message_service,
            connection_manager: self.connection_manager,
            peer_manager: self.peer_manager,
            inbound_message_subscription_factory: self.inbound_message_subscription_factory,
            outbound_message_pool: self.outbound_message_pool,
            node_identity: self.node_identity,
            // Add handles for started services
            control_service_handle,
        })
    }
}

/// # CommsServices
///
/// This struct provides a handle to and control over all the running comms services.
/// You can get a [DomainConnector] from which to receive messages by using the `create_connector`
/// method. Use the `shutdown` method to attempt to cleanly shut all comms services down.
pub struct CommsServices<MType>
where MType: Send + Sync + Debug
{
    zmq_context: ZmqContext,
    outbound_message_service: Arc<OutboundMessageService>,
    control_service_handle: Option<ControlServiceHandle>,
    outbound_message_pool: OutboundMessagePool,
    node_identity: Arc<NodeIdentity>,
    connection_manager: Arc<ConnectionManager>,
    peer_manager: Arc<PeerManager>,
    inbound_message_subscription_factory: Arc<InboundTopicSubscriptionFactory<MType>>,
}

impl<MType> CommsServices<MType>
where
    MType: DispatchableKey,
    MType: Clone + Send + Debug,
{
    pub fn zmq_context(&self) -> &ZmqContext {
        &self.zmq_context
    }

    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    pub fn connection_manager(&self) -> Arc<ConnectionManager> {
        Arc::clone(&self.connection_manager)
    }

    pub fn outbound_message_service(&self) -> Arc<OutboundMessageService> {
        Arc::clone(&self.outbound_message_service)
    }

    pub fn inbound_message_subscription_factory(&self) -> Arc<InboundTopicSubscriptionFactory<MType>> {
        Arc::clone(&self.inbound_message_subscription_factory)
    }

    pub fn shutdown(self) -> Result<(), CommsServicesError> {
        info!(target: LOG_TARGET, "Comms is shutting down");
        let mut shutdown_results = Vec::new();
        // Shutdown control service
        if let Some(control_service_shutdown_result) = self.control_service_handle.map(|hnd| hnd.shutdown()) {
            shutdown_results.push(control_service_shutdown_result.map_err(CommsServicesError::ControlServiceError));
        }

        // Shutdown outbound message pool
        shutdown_results.push(
            self.outbound_message_pool
                .shutdown()
                .map_err(CommsServicesError::OutboundError),
        );

        // Lastly, Shutdown connection manager
        match Arc::try_unwrap(self.connection_manager) {
            Ok(conn_manager) => {
                for result in conn_manager.shutdown() {
                    shutdown_results.push(result.map_err(CommsServicesError::ConnectionManagerError));
                }
            },
            Err(_) => error!(
                target: LOG_TARGET,
                "Unable to cleanly shutdown connection manager because references are still held by other threads"
            ),
        }

        Self::check_clean_shutdown(shutdown_results)
    }

    fn check_clean_shutdown(results: Vec<Result<(), CommsServicesError>>) -> Result<(), CommsServicesError> {
        let mut has_error = false;
        for result in results {
            if let Err(err) = result {
                error!(target: LOG_TARGET, "Error occurred when shutting down {:?}", err);
                has_error = true;
            }
        }

        if has_error {
            Err(CommsServicesError::UncleanShutdown)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_storage::key_val_store::HMapDatabase;

    #[test]
    fn new_no_control_service() {
        let comms_services: CommsServiceContainer<String> = CommsBuilder::new()
            .with_node_identity(NodeIdentity::random_for_test(None))
            .with_peer_storage(HMapDatabase::new())
            .build()
            .unwrap();

        assert!(comms_services.control_service.is_none());
    }

    #[test]
    fn new_with_control_service() {
        let comms_services: CommsServiceContainer<String> = CommsBuilder::new()
            .with_node_identity(NodeIdentity::random_for_test(None))
            .with_peer_storage(HMapDatabase::new())
            .configure_control_service(ControlServiceConfig::default())
            .build()
            .unwrap();

        assert!(comms_services.control_service.is_some());
    }
}
