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
use super::{MessagePoolWorker, RetryQueue};
use crate::{
    connection_manager::ConnectionManager,
    outbound_message_service::{
        outbound_message_pool::error::OutboundMessagePoolError,
        OutboundError,
        OutboundMessage,
    },
    peer_manager::{NodeId, PeerManager},
};
use crossbeam_channel::{self as channel, Receiver, RecvTimeoutError, Sender};
use crossbeam_deque::{Stealer, Worker};
use log::*;
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};
use tari_utilities::thread_join::ThreadJoinWithTimeout;

/// The default number of processing worker threads that will be created by the OutboundMessageService
pub const DEFAULT_NUM_OUTBOUND_MSG_WORKERS: usize = 4;

const LOG_TARGET: &str = "comms::outbound_message_service::pool";

/// Set the maximum waiting time for Retry Service Threads and MessagePoolWorker threads to join
const MSG_POOL_WORKER_THREAD_JOIN_TIMEOUT: Duration = Duration::from_millis(3000);
const WORK_FORWARDER_THREAD_JOIN_TIMEOUT: Duration = Duration::from_millis(1500);

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
            num_workers: DEFAULT_NUM_OUTBOUND_MSG_WORKERS,
            max_retries: 10,
        }
    }
}

/// # OutboundMessagePool
///
/// The OutboundMessagePool receives messages and forwards them to a pool of [MsgPoolWorker]s who's job it is to
/// reliably send the message, if possible.
///
/// The pool starts the configured number of workers (see [OutboundMessagePoolConfig]) and distributes messages
/// between them using a [crossbeam_deque::Worker].
///
/// Messages to send are received on a [crossbeam_channel::Receiver]. A copy of the [Sender] side can be obtained
/// by calling the [OutboundMessagePool::sender] method.
///
/// [crossbeam_channel::Receiver]: https://docs.rs/crossbeam-channel/0.3.9/crossbeam_channel/struct.Receiver.html
/// [Sender]: https://docs.rs/crossbeam-channel/0.3.9/crossbeam_channel/struct.Sender.html
/// [OutboundMessagePool::sender]: #method.sender
/// [crossbeam_deque::Worker]: https://docs.rs/crossbeam/0.7.2/crossbeam/deque/struct.Worker.html
/// [OutboundMessage]: ../outbound_message/struct.OutboundMessage.html
/// [OutboundMessagePoolConfig]: ./struct.OutboundMessagePoolConfig.html
/// [MsgPoolWorker]: ../worker/struct.MsgPoolWorker.html
pub struct OutboundMessagePool {
    config: OutboundMessagePoolConfig,
    message_tx: Sender<OutboundMessage>,
    message_rx: Option<Receiver<OutboundMessage>>,
    peer_manager: Arc<PeerManager>,
    retry_queue: RetryQueue<NodeId, OutboundMessage>,
    connection_manager: Arc<ConnectionManager>,
    worker_thread_handles: Vec<JoinHandle<Result<(), OutboundMessagePoolError>>>,
    work_forwarder_handle: Option<JoinHandle<()>>,
    worker_shutdown_signals: Vec<Sender<()>>,
    work_forwarder_shutdown_tx: Sender<()>,
    work_forwarder_shutdown_rx: Option<Receiver<()>>,
}
impl OutboundMessagePool {
    /// Construct a new Outbound Message Pool.
    ///
    /// # Arguments
    /// `config` - The configuration struct to use for the Outbound Message Pool
    /// `peer_manager` - Arc to a PeerManager
    /// `connection_manager` - Arc to a ConnectionManager
    pub fn new(
        config: OutboundMessagePoolConfig,
        peer_manager: Arc<PeerManager>,
        connection_manager: Arc<ConnectionManager>,
    ) -> OutboundMessagePool
    {
        let (message_tx, message_rx) = channel::unbounded();
        let (shutdown_tx, shutdown_rx) = channel::bounded(1);
        let retry_queue = RetryQueue::new();
        OutboundMessagePool {
            config,
            message_rx: Some(message_rx),
            message_tx,
            peer_manager,
            connection_manager,
            retry_queue,
            worker_thread_handles: Vec::new(),
            worker_shutdown_signals: Vec::new(),
            work_forwarder_handle: None,
            work_forwarder_shutdown_tx: shutdown_tx,
            work_forwarder_shutdown_rx: Some(shutdown_rx),
        }
    }

    /// Returns a copy of the Sender which can be used to send messages for processing to the
    /// OutboundMessagePool workers
    pub fn sender(&self) -> Sender<OutboundMessage> {
        self.message_tx.clone()
    }

    /// Starts a thread that reads from the message_source and pushes worker on the worker queue
    fn start_work_forwarder(&mut self, worker: Worker<OutboundMessage>) -> JoinHandle<()> {
        let message_rx = self
            .message_rx
            .take()
            .expect("Invariant: OutboundMessagePool was initialized without a message_rx");

        let shutdown_rx = self
            .work_forwarder_shutdown_rx
            .take()
            .expect("Invariant: OutboundMessagePool was initialized without a shutdown_rx");

        thread::spawn(move || loop {
            match shutdown_rx.recv_timeout(Duration::from_millis(1)) {
                Ok(_) => break,
                Err(RecvTimeoutError::Timeout) => {},
                Err(RecvTimeoutError::Disconnected) => {
                    warn!(
                        target: LOG_TARGET,
                        "Work forwarder shutdown signal disconnected unexpectedly"
                    );
                    break;
                },
            }

            match message_rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(msg) => worker.push(msg),
                Err(RecvTimeoutError::Timeout) => {},
                Err(RecvTimeoutError::Disconnected) => {
                    warn!(target: LOG_TARGET, "Work forwarder sender disconnected unexpectedly");
                    break;
                },
            }
        })
    }

    /// Start the Outbound Message Pool.
    ///
    /// This starts the configured number of workers and a worker forwarder. The forwarder forwards
    /// work to the worker queue and the workers take work from the worker queue.
    pub fn start(&mut self) -> Result<(), OutboundMessagePoolError> {
        info!(target: LOG_TARGET, "Starting outbound message pool");

        let worker = Worker::new_fifo();

        info!(target: LOG_TARGET, "Starting {} OMP workers", self.config.num_workers);
        for _ in 0..self.config.num_workers {
            self.start_message_worker(worker.stealer(), self.retry_queue.clone())?;
        }

        info!(target: LOG_TARGET, "Starting OMP work producer");
        let handle = self.start_work_forwarder(worker);
        self.work_forwarder_handle = Some(handle);

        Ok(())
    }

    fn start_message_worker(
        &mut self,
        stealer: Stealer<OutboundMessage>,
        retry_queue: RetryQueue<NodeId, OutboundMessage>,
    ) -> Result<(), OutboundMessagePoolError>
    {
        let (worker_thread_handle, worker_shutdown_signal) = MessagePoolWorker::start(
            self.config,
            stealer,
            retry_queue,
            self.peer_manager.clone(),
            self.connection_manager.clone(),
        )?;

        self.worker_thread_handles.push(worker_thread_handle);
        self.worker_shutdown_signals.push(worker_shutdown_signal);

        Ok(())
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

        self.retry_queue.clear();
        // Send shutdown signal to message retry queue if it has been started
        self.work_forwarder_shutdown_tx.send(()).map_err(|e| {
            OutboundError::ShutdownSignalSendError(format!("Failed to send shutdown signal to work forwarder: {:?}", e))
        })?;

        if let Some(handle) = self.work_forwarder_handle {
            handle
                .timeout_join(WORK_FORWARDER_THREAD_JOIN_TIMEOUT)
                .map_err(OutboundError::ThreadJoinError)?;
        }

        // Join worker threads
        for worker_thread_handle in self.worker_thread_handles {
            worker_thread_handle
                .timeout_join(MSG_POOL_WORKER_THREAD_JOIN_TIMEOUT)
                .map_err(OutboundError::ThreadJoinError)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        connection::{InprocAddress, NetAddress, ZmqContext},
        connection_manager::{ConnectionManager, PeerConnectionConfig},
        outbound_message_service::{
            outbound_message_pool::{OutboundMessagePoolConfig, RetryQueue},
            OutboundMessagePool,
        },
        peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer, PeerManager},
    };
    use crossbeam_deque::Worker;
    use std::{sync::Arc, time::Duration};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HMapDatabase;
    use tari_utilities::thread_join::ThreadJoinWithTimeout;

    fn make_peer_connection_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            peer_connection_establish_timeout: Duration::from_millis(10),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 1,
            max_connections: 10,
            message_sink_address: consumer_address,
            socks_proxy_address: None,
        }
    }

    fn outbound_message_pool_setup(
        context: &ZmqContext,
    ) -> (Arc<PeerManager>, Arc<ConnectionManager>, Arc<NodeIdentity>) {
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let peer_manager = Arc::new(PeerManager::new(HMapDatabase::new()).unwrap());

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
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context);
        let omp_config = OutboundMessagePoolConfig::default();
        let omp = OutboundMessagePool::new(omp_config.clone(), peer_manager.clone(), connection_manager.clone());
        assert_eq!(omp.worker_thread_handles.len(), 0);
        assert_eq!(omp.worker_shutdown_signals.len(), 0);
        assert!(omp.work_forwarder_shutdown_rx.is_some());
        assert!(omp.work_forwarder_handle.is_none());
    }

    #[test]
    fn work_forwarder_shutdown() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context);
        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(omp_config.clone(), peer_manager.clone(), connection_manager.clone());

        let worker = Worker::new_fifo();
        let handle = omp.start_work_forwarder(worker);

        omp.shutdown().unwrap();
        handle.timeout_join(Duration::from_millis(3000)).unwrap();
    }

    #[test]
    fn start_message_worker() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context);
        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(omp_config.clone(), peer_manager.clone(), connection_manager.clone());
        assert_eq!(omp.worker_shutdown_signals.len(), 0);
        assert_eq!(omp.worker_thread_handles.len(), 0);

        let worker = Worker::new_fifo();
        let retry_queue = RetryQueue::new();

        omp.start_message_worker(worker.stealer(), retry_queue).unwrap();

        assert_eq!(omp.worker_shutdown_signals.len(), 1);
        assert_eq!(omp.worker_thread_handles.len(), 1);

        omp.shutdown().unwrap();
    }

    #[test]
    fn clean_shutdown() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_pool_setup(&context);

        // Add random peer
        let mut rng = rand::OsRng::new().unwrap();
        let (_dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk.clone()).unwrap();
        let net_addresses = "127.0.0.1:45325".parse::<NetAddress>().unwrap().into();
        let dest_peer = Peer::new(pk.clone(), node_id, net_addresses, PeerFlags::default());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(omp_config.clone(), peer_manager.clone(), connection_manager.clone());

        omp.start().unwrap();

        omp.shutdown().unwrap();
    }
}
