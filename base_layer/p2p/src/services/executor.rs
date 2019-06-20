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
use crate::tari_message::{NetMessage, TariMessageType};
use log::*;
use std::{
    collections::HashMap,
    sync::{
        mpsc::{channel, Receiver, RecvTimeoutError, Sender},
        Arc,
        Barrier,
    },
    thread,
    time::Duration,
};
use tari_comms::{
    builder::CommsServices,
    connection::{Connection, InprocAddress, ZmqContext},
    control_service::ControlServiceConfig,
    domain_connector::ConnectorError,
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::PeerManager,
    CommsBuilder,
    DomainConnector,
};
use threadpool::ThreadPool;

const LOG_TARGET: &'static str = "base_layer::p2p::services";

/// Control messages for services
pub enum ServiceControlMessage {
    /// Service should shut down
    Shutdown,
}

/// This is reponsible for creating and managing the thread pool for
/// services that should be executed.
pub struct ServiceExecutor {
    thread_pool: ThreadPool,
    senders: Vec<Sender<ServiceControlMessage>>,
    comms_services: Arc<CommsServices<TariMessageType>>,
}

impl ServiceExecutor {
    /// Execute the services contained in the given [ServiceRegistry].
    pub fn execute(comms_services: Arc<CommsServices<TariMessageType>>, registry: ServiceRegistry) -> Self {
        let thread_pool = threadpool::Builder::new()
            .thread_name("DomainServices".to_string())
            .num_threads(registry.num_services())
            .thread_stack_size(1_000_000)
            .build();

        let mut senders = Vec::new();

        for mut service in registry.services.into_iter() {
            let (sender, receiver) = channel();
            senders.push(sender);

            let service_context = ServiceContext {
                comms_services: comms_services.clone(),
                receiver,
            };

            thread_pool.execute(move || {
                info!(target: LOG_TARGET, "Starting service {}", service.get_name());
                service.execute(service_context);
                info!(target: LOG_TARGET, "Service '{}' has shut down", service.get_name());
            });
        }

        Self {
            thread_pool,
            senders,
            comms_services,
        }
    }

    /// Send a [ServiceControlMessage::Shutdown] message to all services.
    pub fn shutdown(&self) -> Result<(), ServiceError> {
        if self
            .senders
            .iter()
            .all(sender.send(ServiceControlMessage::Shutdown).is_ok())
        {
            Ok(())
        } else {
            Err(ServiceError::ShutdownSendFailed)
        }
    }

    /// Join on all threads in the thread pool until they all exit or a given timeout is reached.
    pub fn join_timeout(self, timeout: Duration) -> Result<(), ServiceError> {
        let (tx, rx) = channel();
        thread::spawn(move || {
            self.thread_pool.join();
            let _ = tx.send(());
        });

        rx.recv_timeout(timeout).map_err(|_| ServiceError::JoinTimedOut)
    }
}

/// The context object given to each service. This allows the service to receive [ServiceControlMessage]s,
/// access the outbound message service and create [DomainConnector]s to receive comms messages of
/// a particular [TariMessageType].
pub struct ServiceContext {
    comms_services: Arc<CommsServices<TariMessageType>>,
    receiver: Receiver<ServiceControlMessage>,
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
    pub fn get_outbound_message_service(&self) -> &Arc<OutboundMessageService> {
        self.comms_services.get_outbound_message_service()
    }

    /// Create a [DomainConnector] which listens for a particular [TariMessageType].
    pub fn create_connector(&self, message_type: &TariMessageType) -> Result<DomainConnector<'static>, ServiceError> {
        self.comms_services
            .create_connector(message_type)
            .map_err(ServiceError::CommsServicesError)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{services::Service, tari_message::NetMessage};
    use rand::rngs::OsRng;
    use std::{
        sync::{
            mpsc::{channel, Receiver},
            RwLock,
        },
        thread,
    };
    use tari_comms::peer_manager::NodeIdentity;

    #[derive(Clone)]
    struct AddWordService(Arc<RwLock<String>>, &'static str);

    impl Service for AddWordService {
        fn get_name(&self) -> String {
            "tick service".to_string()
        }

        fn get_message_types(&self) -> Vec<TariMessageType> {
            vec![NetMessage::PingPong.into()]
        }

        fn execute(&mut self, context: ServiceContext) {
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
        }
    }

    #[test]
    fn execute() {
        let node_identity =
            NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap();

        let state = Arc::new(RwLock::new("Hello".to_string()));

        let registry = ServiceRegistry::new().register(AddWordService(state.clone(), "Tari"));

        let comms_services = CommsBuilder::new()
            .with_routes(registry.build_comms_routes())
            .with_node_identity(node_identity)
            .build()
            .unwrap()
            .start()
            .map(Arc::new)
            .unwrap();

        let services = ServiceExecutor::execute(comms_services, registry);

        services.shutdown().unwrap();
        services.join_timeout(Duration::from_millis(100));

        {
            let lock = state.read().unwrap();
            assert_eq!(*lock, "Hello Tari");
        }
    }
}
