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
use super::{MessagePoolWorker, MessageRetryService};
use crate::{
    connection::{
        zmq::{InprocAddress, ZmqContext},
        DealerProxy,
    },
    connection_manager::ConnectionManager,
    outbound_message_service::{outbound_message_pool::error::OutboundMessagePoolError, OutboundError},
    peer_manager::PeerManager,
};
use log::*;
use std::{
    sync::{
        mpsc::{sync_channel, SyncSender},
        Arc,
    },
    thread::JoinHandle,
};

/// The default number of processing worker threads that will be created by the OutboundMessageService
pub const DEFAULT_OUTBOUND_MSG_PROCESSING_WORKERS: usize = 4;

const LOG_TARGET: &'static str = "comms::outbound_message_service::pool";

#[derive(Clone, Copy)]
pub struct OutboundMessagePoolConfig {
    /// How many workers to spawn
    pub num_workers: usize,
    /// How many times the pool will requeue a message to be sent
    pub max_retries: u32,
}

impl Default for OutboundMessagePoolConfig {
    fn default() -> Self {
        OutboundMessagePoolConfig {
            num_workers: DEFAULT_OUTBOUND_MSG_PROCESSING_WORKERS,
            max_retries: 10,
        }
    }
}

/// The OutboundMessagePool will field outbound messages received from multiple OutboundMessageService instance that
/// it will receive via the Inbound Inproc connection. It will handle the messages in the queue one at a time and
/// attempt to send them. If they cannot be sent then the Retry count will be incremented and the message pushed to
/// the back of the queue.
pub struct OutboundMessagePool {
    config: OutboundMessagePoolConfig,
    context: ZmqContext,
    worker_dealer_address: InprocAddress,
    message_source_address: InprocAddress,
    failed_message_address: InprocAddress,
    peer_manager: Arc<PeerManager>,
    connection_manager: Arc<ConnectionManager>,
    worker_thread_handles: Vec<JoinHandle<Result<(), OutboundMessagePoolError>>>,
    worker_shutdown_signals: Vec<SyncSender<()>>,
    retry_service_shutdown_signal: Option<SyncSender<()>>,
    retry_service_thread_handle: Option<JoinHandle<Result<(), OutboundMessagePoolError>>>,
    dealer_proxy: DealerProxy,
}
impl OutboundMessagePool {
    /// Construct a new Outbound Message Pool.
    /// # Arguments
    /// `context` - A ZeroMQ context
    /// `config` - The configuration struct to use for the Outbound Message Pool
    /// `message_source_address` - The InProc address used to send messages to this message pool. Usually by the
    /// outbound message service. `failed_message_queue_address` - The InProc address used for messages that have
    /// failed to send. Typically this will be set to the MessageRetryQueue `peer_manager` - an atomic reference to
    /// the peer manager. Used to locate destination peers. `connection_manager` - an atomic reference to the
    /// connection manager. Used to establish peer connections.
    pub fn new(
        config: OutboundMessagePoolConfig,
        context: ZmqContext,
        message_source_address: InprocAddress,
        peer_manager: Arc<PeerManager>,
        connection_manager: Arc<ConnectionManager>,
    ) -> OutboundMessagePool
    {
        let worker_dealer_address = InprocAddress::random();
        OutboundMessagePool {
            config,
            context: context.clone(),
            worker_dealer_address: worker_dealer_address.clone(),
            message_source_address: message_source_address.clone(),
            failed_message_address: InprocAddress::random(),
            peer_manager,
            connection_manager,
            worker_thread_handles: Vec::new(),
            worker_shutdown_signals: Vec::new(),
            retry_service_shutdown_signal: None,
            retry_service_thread_handle: None,
            dealer_proxy: DealerProxy::new(context, message_source_address, worker_dealer_address.clone()),
        }
    }

    /// Start the dealer proxy, which fair-deals messages to workers
    fn start_dealer_proxy(&mut self) -> Result<(), OutboundMessagePoolError> {
        self.dealer_proxy
            .spawn_proxy()
            .map_err(OutboundMessagePoolError::DealerProxyError)
    }

    /// Start the Outbound Message Pool. This will spawn a thread that services the message queue that is sent to the
    /// Inproc address.
    pub fn start(&mut self) -> Result<(), OutboundMessagePoolError> {
        info!(target: LOG_TARGET, "Starting outbound message pool");

        info!(target: LOG_TARGET, "Starting retry message service");
        self.start_retry_service();

        info!(target: LOG_TARGET, "Starting OMP proxy");
        self.start_dealer_proxy()?;

        info!(target: LOG_TARGET, "Starting {} OMP workers", self.config.num_workers);
        for _ in 0..self.config.num_workers {
            self.start_message_worker()?;
        }

        Ok(())
    }

    fn start_message_worker(&mut self) -> Result<(), OutboundMessagePoolError> {
        let (worker_thread_handle, worker_shutdown_signal) = MessagePoolWorker::start(
            self.config.clone(),
            self.context.clone(),
            self.worker_dealer_address.clone(),
            self.failed_message_address.clone(),
            self.peer_manager.clone(),
            self.connection_manager.clone(),
        )?;

        self.worker_thread_handles.push(worker_thread_handle);
        self.worker_shutdown_signals.push(worker_shutdown_signal);

        Ok(())
    }

    fn start_retry_service(&mut self) {
        let (mrq_sender, mrq_receiver) = sync_channel(1);
        let handle = MessageRetryService::start(
            self.context.clone(),
            self.config.clone(),
            self.failed_message_address.clone(),
            self.message_source_address.clone(),
            mrq_receiver,
        );
        self.retry_service_shutdown_signal = Some(mrq_sender);
        self.retry_service_thread_handle = Some(handle);
    }

    /// Tell the underlying dealer thread, nessage retry service and workers to shut down
    pub fn shutdown(self) -> Result<(), OutboundError> {
        // Send Shutdown control message
        for worker_shutdown_signal in self.worker_shutdown_signals {
            worker_shutdown_signal.send(()).map_err(|e| {
                OutboundError::ShutdownSignalSendError(format!(
                    "Failed to send shutdown signal to outbound workers: {:?}",
                    e
                ))
            })?;
        }

        // Send shutdown signal to message retry queue if it has been started
        if let Some(sig) = self.retry_service_shutdown_signal {
            sig.send(()).map_err(|e| {
                OutboundError::ShutdownSignalSendError(format!("Failed to send shutdown signal to MRQ: {:?}", e))
            })?;

            if let Some(handle) = self.retry_service_thread_handle {
                handle
                    .join()
                    .map_err(|_| OutboundError::ThreadJoinError)?
                    .map_err(OutboundError::OutboundMessagePoolError)?;
            }
        }

        // Join worker threads
        for worker_thread_handle in self.worker_thread_handles {
            worker_thread_handle
                .join()
                .map_err(|_| OutboundError::ThreadJoinError)??;
        }

        self.dealer_proxy.shutdown().map_err(OutboundError::DealerProxyError)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        connection::{InprocAddress, NetAddress, ZmqContext},
        connection_manager::{ConnectionManager, PeerConnectionConfig},
        outbound_message_service::{outbound_message_pool::OutboundMessagePoolConfig, OutboundMessagePool},
        peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer, PeerManager},
    };
    use std::{fs, path::PathBuf, sync::Arc, time::Duration};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};

    fn make_peer_connection_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            peer_connection_establish_timeout: Duration::from_millis(10),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 1,
            message_sink_address: consumer_address,
            socks_proxy_address: None,
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
        let _ = fs::create_dir(&path).unwrap_or_default();
        LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(10)
            .set_max_number_of_databases(2)
            .add_database(name, lmdb_zero::db::CREATE)
            .build()
    }

    fn clean_up_datastore(name: &str) {
        fs::remove_dir_all(get_path(name)).unwrap();
    }

    fn outbound_message_pool_setup(
        context: &ZmqContext,
        database_name: &str,
    ) -> (Arc<PeerManager>, Arc<ConnectionManager>, Arc<NodeIdentity>)
    {
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        // Peer Manager
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();
        let peer_manager = Arc::new(PeerManager::new(peer_database).unwrap());

        // Connection Manager
        let connection_manager = Arc::new(ConnectionManager::new(
            context.clone(),
            node_identity.clone(),
            peer_manager.clone(),
            make_peer_connection_config(InprocAddress::random()),
        ));

        (peer_manager, connection_manager, node_identity)
    }

    #[test]
    fn new() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context, "new");
        let omp_inbound_address = InprocAddress::random();
        let omp_config = OutboundMessagePoolConfig::default();
        let omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            peer_manager.clone(),
            connection_manager.clone(),
        );
        assert_eq!(omp.worker_thread_handles.len(), 0);
        assert_eq!(omp.worker_shutdown_signals.len(), 0);
        assert!(omp.retry_service_thread_handle.is_none());
        assert!(omp.retry_service_shutdown_signal.is_none());

        clean_up_datastore("new")
    }

    #[test]
    fn start_dealer_proxy() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context, "start_dealer_proxy");
        let omp_inbound_address = InprocAddress::random();
        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            peer_manager.clone(),
            connection_manager.clone(),
        );

        assert!(!omp.dealer_proxy.is_running());
        omp.start_dealer_proxy().unwrap();
        assert!(omp.dealer_proxy.is_running());

        omp.shutdown().unwrap();

        clean_up_datastore("start_dealer_proxy");
    }

    #[test]
    fn start_message_worker() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context, "start_message_worker");
        let omp_inbound_address = InprocAddress::random();
        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            peer_manager.clone(),
            connection_manager.clone(),
        );
        assert_eq!(omp.worker_shutdown_signals.len(), 0);
        assert_eq!(omp.worker_thread_handles.len(), 0);

        omp.start_message_worker().unwrap();

        assert_eq!(omp.worker_shutdown_signals.len(), 1);
        assert_eq!(omp.worker_thread_handles.len(), 1);

        omp.shutdown().unwrap();

        clean_up_datastore("start_message_worker");
    }

    #[test]
    fn clean_shutdown() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context, "clean_shutdown");

        // Add random peer
        let mut rng = rand::OsRng::new().unwrap();
        let (_dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk.clone()).unwrap();
        let net_addresses = "127.0.0.1:45325".parse::<NetAddress>().unwrap().into();
        let dest_peer = Peer::new(pk.clone(), node_id, net_addresses, PeerFlags::default());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let omp_inbound_address = InprocAddress::random();
        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            peer_manager.clone(),
            connection_manager.clone(),
        );

        omp.start().unwrap();

        omp.shutdown().unwrap();
        clean_up_datastore("clean_shutdown");
    }
}
