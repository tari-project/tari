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

use super::{error::ServiceError, registry::ServiceRegistry};
use crate::tari_message::TariMessageType;
use log::*;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tari_comms::{
    builder::CommsServices,
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::{NodeIdentity, PeerManager},
    DomainConnector,
};
use threadpool::ThreadPool;

use crossbeam_channel as channel;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use tari_comms::{
    builder::{CommsRoutes, CommsServicesError},
    connection::ZmqContext,
};

const LOG_TARGET: &str = "base_layer::p2p::services";

/// Control messages for services
pub enum ServiceControlMessage {
    /// Service should shut down
    Shutdown,
}

/// This is reponsible for creating and managing the thread pool for
/// services that should be executed.
pub struct ServiceExecutor {
    thread_pool: Mutex<ThreadPool>,
    senders: Vec<Sender<ServiceControlMessage>>,
}

impl ServiceExecutor {
    /// Execute the services contained in the given [ServiceRegistry].
    pub fn execute(comms_services: &CommsServices<TariMessageType>, registry: ServiceRegistry) -> Self {
        let thread_pool = threadpool::Builder::new()
            .thread_name("DomainServices".to_string())
            .num_threads(registry.num_services())
            .thread_stack_size(1_000_000)
            .build();

        let mut senders = Vec::new();

        for mut service in registry.services.into_iter() {
            let (sender, receiver) = channel::unbounded();
            senders.push(sender);

            let service_context = ServiceContext {
                oms: comms_services.outbound_message_service(),
                peer_manager: comms_services.peer_manager(),
                receiver,
                routes: comms_services.routes().clone(),
                zmq_context: comms_services.zmq_context().clone(),
            };

            thread_pool.execute(move || {
                info!(target: LOG_TARGET, "Starting service {}", service.get_name());

                match service.execute(service_context) {
                    Ok(_) => {
                        info!(
                            target: LOG_TARGET,
                            "Service '{}' has successfully shut down",
                            service.get_name(),
                        );
                    },
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Service '{}' has exited with an error: {:?}",
                            service.get_name(),
                            err
                        );
                    },
                }
            });
        }

        Self {
            thread_pool: Mutex::new(thread_pool),
            senders,
        }
    }

    /// Send a [ServiceControlMessage::Shutdown] message to all services.
    pub fn shutdown(&self) -> Result<(), ServiceError> {
        let mut failed = false;
        for sender in &self.senders {
            if sender.send(ServiceControlMessage::Shutdown).is_err() {
                failed = true;
            }
        }

        // TODO: Wait for services to exit and then shutdown the comms
        //        self.comms_services
        //            .shutdown()
        //            .map_err(ServiceError::CommsServicesError)?;

        if failed {
            Err(ServiceError::ShutdownSendFailed)
        } else {
            Ok(())
        }
    }

    /// Join on all threads in the thread pool until they all exit or a given timeout is reached.
    pub fn join_timeout(self, timeout: Duration) -> Result<(), ServiceError> {
        let (tx, rx) = channel::unbounded();
        let thread_pool = self.thread_pool;
        thread::spawn(move || {
            acquire_lock!(thread_pool).join();
            let _ = tx.send(());
        });

        rx.recv_timeout(timeout).map_err(|_| ServiceError::JoinTimedOut)?;

        Ok(())
    }
}

/// The context object given to each service. This allows the service to receive [ServiceControlMessage]s,
/// access the outbound message service and create [DomainConnector]s to receive comms messages of
/// a particular [TariMessageType].
pub struct ServiceContext {
    oms: Arc<OutboundMessageService>,
    peer_manager: Arc<PeerManager>,
    receiver: Receiver<ServiceControlMessage>,
    routes: CommsRoutes<TariMessageType>,
    zmq_context: ZmqContext,
}

impl ServiceContext {
    /// Attempt to retrieve a control message. Returns `Some(ServiceControlMessage)` if there
    /// is a message on the channel or `None` if the channel is empty and the timeout is reached.
    pub fn get_control_message(&self, timeout: Duration) -> Option<ServiceControlMessage> {
        match self.receiver.recv_timeout(timeout) {
            Ok(msg) => Some(msg),
            // Sender has disconnected (dropped) so return a shutdown signal
            // This should never happen in normal operation
            Err(RecvTimeoutError::Disconnected) => Some(ServiceControlMessage::Shutdown),
            Err(RecvTimeoutError::Timeout) => None,
        }
    }

    /// Retrieve and `Arc` of the outbound message service. Used for sending outbound messages.
    pub fn outbound_message_service(&self) -> Arc<OutboundMessageService> {
        Arc::clone(&self.oms)
    }

    /// Retrieve and `Arc` of the PeerManager. Used for managing peers.
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    /// Retrieve and `Arc` of the NodeIdentity. Used for managing the current Nodes Identity.
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms_services.node_identity.clone()
    }

    /// Create a [DomainConnector] which listens for a particular [TariMessageType].
    pub fn create_connector<'de>(&self, message_type: &TariMessageType) -> Result<DomainConnector<'de>, ServiceError> {
        let addr = self
            .routes
            .get_address(&message_type)
            .ok_or(ServiceError::CommsServicesError(
                CommsServicesError::MessageTypeNotRegistered,
            ))?;

        DomainConnector::listen(&self.zmq_context, &addr).map_err(ServiceError::ConnectorError)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{services::Service, tari_message::NetMessage};
    use rand::rngs::OsRng;
    use std::{path::PathBuf, sync::RwLock};
    use tari_comms::{peer_manager::NodeIdentity, CommsBuilder};
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};

    #[derive(Clone)]
    struct AddWordService(Arc<RwLock<String>>, &'static str);

    impl Service for AddWordService {
        fn get_name(&self) -> String {
            "tick service".to_string()
        }

        fn get_message_types(&self) -> Vec<TariMessageType> {
            vec![NetMessage::PingPong.into()]
        }

        fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
            let mut added_word = false;
            loop {
                if !added_word {
                    let mut lock = self.0.write().unwrap();
                    *lock = format!("{} {}", *lock, self.1);
                    added_word = true;
                }
                if let Some(msg) = context.get_control_message(Duration::from_millis(1000)) {
                    match msg {
                        ServiceControlMessage::Shutdown => break,
                    }
                }
            }

            Ok(())
        }
    }

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
    fn execute() {
        let node_identity =
            NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap();

        let state = Arc::new(RwLock::new("Hello".to_string()));
        let service = AddWordService(state.clone(), "Tari");
        let registry = ServiceRegistry::new().register(service);

        let database_name = "executor_execute"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();

        let comms_services = CommsBuilder::new()
            .with_routes(registry.build_comms_routes())
            .with_node_identity(node_identity)
            .with_peer_storage(peer_database)
            .build()
            .unwrap()
            .start()
            .map(Arc::new)
            .unwrap();

        let services = ServiceExecutor::execute(&comms_services, registry);

        services.shutdown().unwrap();
        services.join_timeout(Duration::from_millis(100)).unwrap();
        let comms = Arc::try_unwrap(comms_services)
            .map_err(|_| ServiceError::CommsServiceOwnershipError)
            .unwrap();

        comms.shutdown().unwrap();

        {
            let lock = acquire_read_lock!(state);
            assert_eq!(*lock, "Hello Tari");
        }

        clean_up_datastore(database_name);
    }
}
