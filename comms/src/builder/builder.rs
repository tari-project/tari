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
    builder::CommsRoutes,
    connection::{ConnectionError, DealerProxyError, InprocAddress, ZmqContext},
    connection_manager::{ConnectionManager, PeerConnectionConfig},
    control_service::{ControlService, ControlServiceConfig, ControlServiceError, ControlServiceHandle},
    dispatcher::DispatchableKey,
    domain_connector::ConnectorError,
    inbound_message_service::{
        comms_msg_handlers::construct_comms_msg_dispatcher,
        error::InboundError,
        inbound_message_broker::{BrokerError, InboundMessageBroker},
        inbound_message_service::{InboundMessageService, InboundMessageServiceConfig},
    },
    outbound_message_service::{
        outbound_message_pool::{OutboundMessagePoolConfig, OutboundMessagePoolError},
        outbound_message_service::OutboundMessageService,
        OutboundError,
        OutboundMessagePool,
    },
    peer_manager::{NodeIdentity, PeerManager, PeerManagerError},
    types::CommsDatabase,
    DomainConnector,
};
use derive_error::Error;
use log::*;
use serde::{de::DeserializeOwned, export::fmt::Debug, Serialize};
use std::sync::Arc;
use tari_storage::lmdb_store::LMDBDatabase;

const LOG_TARGET: &'static str = "comms::builder";

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
    /// Comms routes have not been defined. Call `with_routes` on [CommsBuilder]
    RoutesNotDefined,
    BrokerStartError(BrokerError),
    DatastoreUndefined,
}

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
///
/// The [build] method will return an error if any required builder methods are not called. These
/// are detailed further down on the method docs.
pub struct CommsBuilder<MType>
where MType: Clone
{
    zmq_context: ZmqContext,
    routes: Option<CommsRoutes<MType>>,
    peer_storage: Option<CommsDatabase>,
    control_service_config: Option<ControlServiceConfig<MType>>,
    omp_config: Option<OutboundMessagePoolConfig>,
    ims_config: Option<InboundMessageServiceConfig>,
    node_identity: Option<NodeIdentity>,
    peer_conn_config: Option<PeerConnectionConfig>,
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
            routes: None,
            node_identity: None,
        }
    }

    /// Set the [CommsRoute]s to use. This is required.
    ///
    /// [CommsRoute]: ../routes/CommsRoutes.html
    pub fn with_routes(mut self, routes: CommsRoutes<MType>) -> Self {
        self.routes = Some(routes);
        debug!(target: LOG_TARGET, "Comms routes: {:#?}", self.routes);
        self
    }

    /// Set the [NodeIdentity] for this comms instance. This is required.
    ///
    /// [OutboundMessagePool]: ../../outbound_message_service/index.html#outbound-message-pool
    pub fn with_node_identity(mut self, node_identity: NodeIdentity) -> Self {
        self.node_identity = Some(node_identity);
        self
    }

    /// Set the peer storage database to use. This is optional.
    pub fn with_peer_storage(mut self, peer_storage: LMDBDatabase) -> Self {
        self.peer_storage = Some(peer_storage);
        self
    }

    /// Configure the [ControlService]. This is optional.
    ///
    /// [ControlService]: ../../control_service/index.html
    pub fn configure_control_service(mut self, config: ControlServiceConfig<MType>) -> Self {
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

    fn make_control_service(&mut self, node_identity: Arc<NodeIdentity>) -> Option<ControlService<MType>> {
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
        message_sink_address: InprocAddress,
        peer_manager: Arc<PeerManager>,
    ) -> Result<Arc<OutboundMessageService>, CommsBuilderError>
    {
        OutboundMessageService::new(
            self.zmq_context.clone(),
            node_identity,
            message_sink_address,
            peer_manager,
        )
        .map(Arc::new)
        .map_err(CommsBuilderError::OutboundMessageServiceError)
    }

    fn make_outbound_message_pool(
        &mut self,
        message_sink_address: InprocAddress,
        peer_manager: Arc<PeerManager>,
        connection_manager: Arc<ConnectionManager>,
    ) -> OutboundMessagePool
    {
        let config = self.omp_config.take().unwrap_or_default();

        OutboundMessagePool::new(
            config,
            self.zmq_context.clone(),
            // OMP can requeue back onto itself
            message_sink_address.clone(),
            peer_manager,
            connection_manager,
        )
    }

    fn make_inbound_message_service(
        &mut self,
        node_identity: Arc<NodeIdentity>,
        message_sink_address: InprocAddress,
        inbound_message_broker: Arc<InboundMessageBroker<MType>>,
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
            inbound_message_broker,
            oms,
            peer_manager,
        )
    }

    fn make_inbound_message_broker(
        &mut self,
        routes: &CommsRoutes<MType>,
    ) -> Result<Arc<InboundMessageBroker<MType>>, CommsBuilderError>
    {
        let broker = routes.inner().iter().fold(
            InboundMessageBroker::new(self.zmq_context.clone()),
            |broker, (message_type, address)| broker.route(message_type.clone(), address.clone()),
        )
            // FIXME(sdbondi): We have to start the broker here because we cannot mutate it once inUse these fields when
            // able to shutdown
            .start().map_err(CommsBuilderError::BrokerStartError)?;

        Ok(Arc::new(broker))
    }

    fn make_routes(&mut self) -> Result<CommsRoutes<MType>, CommsBuilderError> {
        let mut routes = self.routes.take().ok_or(CommsBuilderError::RoutesNotDefined)?;

        // If the control service is enabled and an accept route is not already defined - define one
        // so that connections can be established
        if let Some(ref config) = self.control_service_config {
            if routes.get_address(&config.accept_message_type).is_none() {
                warn!(
                    target: LOG_TARGET,
                    "Adding dead end route for accept message as one was not specified which matches the control \
                     service `accept_message_type` setting"
                );
                routes = routes.register(config.accept_message_type.clone());
            }
        }

        Ok(routes)
    }

    /// Build the required comms services. Services will not be started.
    pub fn build(mut self) -> Result<CommsServiceContainer<MType>, CommsBuilderError> {
        let node_identity = self.make_node_identity()?;

        let peer_manager = self.make_peer_manager()?;

        let peer_conn_config = self.make_peer_connection_config();

        // This must happen before control service so that it can use it's config to setup a default route for accept
        let routes = self.make_routes()?;

        let control_service = self.make_control_service(node_identity.clone());

        let connection_manager =
            self.make_connection_manager(node_identity.clone(), peer_manager.clone(), peer_conn_config.clone());

        let outbound_message_sink_address = InprocAddress::random();
        let outbound_message_service = self.make_outbound_message_service(
            node_identity.clone(),
            outbound_message_sink_address.clone(),
            peer_manager.clone(),
        )?;

        let outbound_message_pool = self.make_outbound_message_pool(
            outbound_message_sink_address,
            peer_manager.clone(),
            connection_manager.clone(),
        );

        let inbound_message_broker = self.make_inbound_message_broker(&routes)?;

        let inbound_message_service = self.make_inbound_message_service(
            node_identity,
            peer_conn_config.message_sink_address,
            inbound_message_broker.clone(),
            outbound_message_service.clone(),
            peer_manager.clone(),
        );

        Ok(CommsServiceContainer {
            zmq_context: self.zmq_context,
            routes,
            control_service,
            inbound_message_service,
            inbound_message_broker,
            connection_manager,
            outbound_message_pool,
            outbound_message_service,
            peer_manager,
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
    ConnectorError(ConnectorError),
    InboundMessageBrokerError(BrokerError),
    OutboundMessagePoolError(OutboundMessagePoolError),
    OutboundError(OutboundError),
    InboundMessageServiceError(InboundError),
}

/// Contains the built comms services
pub struct CommsServiceContainer<MType>
where
    MType: Serialize + DeserializeOwned,
    MType: DispatchableKey,
    MType: Clone,
{
    zmq_context: ZmqContext,
    routes: CommsRoutes<MType>,
    connection_manager: Arc<ConnectionManager>,
    control_service: Option<ControlService<MType>>,
    inbound_message_broker: Arc<InboundMessageBroker<MType>>,
    inbound_message_service: InboundMessageService<MType>,
    outbound_message_pool: OutboundMessagePool,
    outbound_message_service: Arc<OutboundMessageService>,
    peer_manager: Arc<PeerManager>,
}

impl<MType> CommsServiceContainer<MType>
where
    MType: Serialize + DeserializeOwned,
    MType: DispatchableKey,
    MType: Clone,
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
            .map_err(|err| CommsServicesError::InboundMessageServiceError(err))?;
        self.outbound_message_pool
            .start()
            .map_err(CommsServicesError::OutboundMessagePoolError)?;

        Ok(CommsServices {
            // Transfer ownership to CommsServices
            zmq_context: self.zmq_context,
            outbound_message_service: self.outbound_message_service,
            routes: self.routes,
            connection_manager: self.connection_manager,
            inbound_message_broker: self.inbound_message_broker,
            peer_manager: self.peer_manager,
            outbound_message_pool: self.outbound_message_pool,
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
pub struct CommsServices<MType> {
    zmq_context: ZmqContext,
    outbound_message_service: Arc<OutboundMessageService>,
    routes: CommsRoutes<MType>,
    control_service_handle: Option<ControlServiceHandle>,
    inbound_message_broker: Arc<InboundMessageBroker<MType>>,
    outbound_message_pool: OutboundMessagePool,
    connection_manager: Arc<ConnectionManager>,
    pub peer_manager: Arc<PeerManager>,
}

impl<MType> CommsServices<MType>
where
    MType: DispatchableKey,
    MType: Clone,
{
    pub fn peer_manager(&self) -> &PeerManager {
        &self.peer_manager
    }

    pub fn outbound_message_service(&self) -> Arc<OutboundMessageService> {
        Arc::clone(&self.outbound_message_service)
    }

    pub fn create_connector<'de>(&self, message_type: &MType) -> Result<DomainConnector<'de>, CommsServicesError> {
        let addr = self
            .routes
            .get_address(&message_type)
            .ok_or(CommsServicesError::MessageTypeNotRegistered)?;

        DomainConnector::listen(&self.zmq_context, &addr).map_err(CommsServicesError::ConnectorError)
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

        match Arc::try_unwrap(self.inbound_message_broker) {
            Ok(broker) => drop(broker),
            Err(_) => {
                error!(
                    target: LOG_TARGET,
                    "Unable to cleanly drop inbound message broker because references are still held by other threads"
                );
            },
        }

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
    use std::path::PathBuf;
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};

    fn get_path(name: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name);
        path.to_str().unwrap().to_string()
    }

    fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
        let path = get_path(name);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(10)
            .set_max_number_of_databases(2)
            .add_database(name, lmdb_zero::db::CREATE)
            .build()
    }

    fn clean_up_datastore(name: &str) {
        std::fs::remove_dir_all(get_path(name)).unwrap();
    }

    #[test]
    fn new_no_control_service() {
        let database_name = "builder_new_no_control_service"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();

        let comms_services = CommsBuilder::new()
            .with_routes(CommsRoutes::new().register("hello".to_owned()))
            .with_node_identity(NodeIdentity::random_for_test(None))
            .with_peer_storage(peer_database)
            .build()
            .unwrap();

        assert!(comms_services.control_service.is_none());

        clean_up_datastore(database_name);
    }

    #[test]
    fn new_with_control_service() {
        let database_name = "builder_new_with_control_service"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();

        let comms_services = CommsBuilder::new()
            .with_routes(CommsRoutes::new().register("hello".to_owned()))
            .with_node_identity(NodeIdentity::random_for_test(None))
            .with_peer_storage(peer_database)
            .configure_control_service(ControlServiceConfig::default())
            .build()
            .unwrap();

        assert!(comms_services.control_service.is_some());

        clean_up_datastore(database_name);
    }
}
