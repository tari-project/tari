// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
use std::thread;

use crate::{
    connection::{
        zmq::{InprocAddress, ZmqContext},
        DealerProxy,
        DealerProxyError,
    },
    connection_manager::ConnectionManager,
    outbound_message_service::{MessagePoolWorker, OutboundError},
    peer_manager::PeerManager,
    types::{CommsDataStore, CommsPublicKey},
};
use chrono::Duration;
use log::*;
#[cfg(test)]
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;

/// The maximum number of processing worker threads that will be created by the OutboundMessageService
pub const MAX_OUTBOUND_MSG_PROCESSING_WORKERS: u8 = 8;

const LOG_TARGET: &'static str = "comms::outbound_message_service::pool";

#[derive(Clone, Copy)]
pub struct OutboundMessagePoolConfig {
    /// How many times the pool will requeue a message to be sent
    pub max_num_of_retries: u32,
    pub retry_wait_time: Duration,
    pub worker_timeout_in_ms: u32,
}

impl Default for OutboundMessagePoolConfig {
    fn default() -> Self {
        OutboundMessagePoolConfig {
            max_num_of_retries: 10,
            retry_wait_time: Duration::seconds(3600),
            worker_timeout_in_ms: 100,
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
    message_queue_address: InprocAddress,
    message_requeue_address: InprocAddress,
    worker_dealer_address: InprocAddress,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    connection_manager: Arc<ConnectionManager>,
    #[cfg(test)]
    test_sync_sender: Vec<SyncSender<String>>, /* These channels will be to test the pool workers threaded
                                                * operation */
}
impl OutboundMessagePool {
    /// Construct a new Outbound Message Pool.
    /// # Arguments
    /// `config` - The configuration struct to use for the Outbound Message Pool
    /// `context` - A ZeroMQ context
    /// `message_queue_address` - The InProc address that will be used to send message to this message pool
    /// `message_requeue_address` - The InProc address that messages that are being requeued is sent to. Typically this
    /// will be same as the `message_queue_address` but this allows for a requeue proxy to be introduced
    /// `peer_manager` - a reference to the peer manager to be used when
    /// sending messages
    pub fn new(
        config: OutboundMessagePoolConfig,
        context: ZmqContext,
        message_queue_address: InprocAddress,
        message_requeue_address: InprocAddress,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<OutboundMessagePool, OutboundError>
    {
        Ok(OutboundMessagePool {
            config,
            context,
            message_queue_address,
            message_requeue_address,
            worker_dealer_address: InprocAddress::random(),
            peer_manager,
            connection_manager,
            #[cfg(test)]
            test_sync_sender: Vec::new(),
        })
    }

    fn start_dealer(&self) -> Result<(), DealerProxyError> {
        DealerProxy::new(self.message_queue_address.clone(), self.worker_dealer_address.clone()).proxy(&self.context)
    }

    /// Start the Outbound Message Pool. This will spawn a thread that services the message queue that is sent to the
    /// Inproc address.
    pub fn start(self) {
        info!(target: LOG_TARGET, "Starting outbound message pool");
        thread::spawn(move || {
            // Start workers
            for _i in 0..MAX_OUTBOUND_MSG_PROCESSING_WORKERS as usize {
                #[allow(unused_mut)] // For testing purposes
                let mut worker = MessagePoolWorker::new(
                    self.config.clone(),
                    self.context.clone(),
                    self.worker_dealer_address.clone(),
                    self.message_queue_address.clone(),
                    self.message_requeue_address.clone(),
                    self.peer_manager.clone(),
                    self.connection_manager.clone(),
                );

                #[cfg(test)]
                worker.set_test_channel(self.test_sync_sender[_i].clone());

                worker.start();
            }

            // Start dealer
            loop {
                if let Err(e) = self.start_dealer() {
                    error!(
                        target: LOG_TARGET,
                        "Could not start dealer for Outbound Message Pool - Error {:?}", e
                    );
                }
            }
        });
    }

    /// Create a channel pairs for use during testing the workers, the sync sender will be passed into the worker's
    /// threads and the receivers returned to the test function.
    #[cfg(test)]
    fn create_test_channels(&mut self) -> Vec<Receiver<String>> {
        let mut receivers = Vec::new();
        for _ in 0..MAX_OUTBOUND_MSG_PROCESSING_WORKERS {
            let (tx, rx) = sync_channel::<String>(0);
            self.test_sync_sender.push(tx);
            receivers.push(rx);
        }
        receivers
    }
}

#[cfg(test)]
mod test {
    use crate::{
        connection::{InprocAddress, NetAddress, NetAddressesWithStats, ZmqContext},
        connection_manager::{ConnectionManager, PeerConnectionConfig},
        message::MessageFlags,
        outbound_message_service::{
            outbound_message_pool::{OutboundMessagePoolConfig, MAX_OUTBOUND_MSG_PROCESSING_WORKERS},
            outbound_message_service::OutboundMessageService,
            BroadcastStrategy,
            OutboundMessagePool,
        },
        peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer, PeerManager},
        types::{CommsDataStore, CommsPublicKey},
    };
    use std::{sync::Arc, thread, time::Duration};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    const LOG_TARGET: &'static str = "comms::outbound_message_service::pool";

    pub fn init() {
        let _ = simple_logger::init();
    }

    fn make_peer_connection_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            control_service_establish_timeout: Duration::from_millis(2000),
            peer_connection_establish_timeout: Duration::from_secs(5),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 3,
            message_sink_address: consumer_address,
            socks_proxy_address: None,
        }
    }

    #[test]
    fn outbound_message_pool_threading_test() {
        init();
        let mut rng = rand::OsRng::new().unwrap();
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        let peer_manager = Arc::new(PeerManager::<CommsPublicKey, CommsDataStore>::new(None).unwrap());

        let local_consumer_address = InprocAddress::random();
        let connection_manager = Arc::new(ConnectionManager::new(
            context.clone(),
            node_identity.clone(),
            peer_manager.clone(),
            make_peer_connection_config(local_consumer_address.clone()),
        ));

        let (_dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk.clone()).unwrap();
        let net_addresses = NetAddressesWithStats::from("1.2.3.4:45325".parse::<NetAddress>().unwrap());
        let dest_peer: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk.clone(), node_id, net_addresses, PeerFlags::default());
        peer_manager.add_peer(dest_peer.clone()).unwrap();

        let omp_inbound_address = InprocAddress::random();
        let omp_config = OutboundMessagePoolConfig::default();
        let mut omp = OutboundMessagePool::new(
            omp_config.clone(),
            context.clone(),
            omp_inbound_address.clone(),
            omp_inbound_address.clone(),
            peer_manager.clone(),
            connection_manager.clone(),
        )
        .unwrap();

        let oms =
            OutboundMessageService::new(context, node_identity, omp_inbound_address, peer_manager.clone()).unwrap();
        let receivers = omp.create_test_channels();

        let _omp = omp.start();
        let message_envelope_body: Vec<u8> = vec![0, 1, 2, 3];

        // Send a message for each thread so we can test that each worker receives one
        for _ in 0..MAX_OUTBOUND_MSG_PROCESSING_WORKERS {
            oms.send(
                BroadcastStrategy::DirectNodeId(dest_peer.node_id.clone()),
                MessageFlags::ENCRYPTED,
                message_envelope_body.clone(),
            )
            .unwrap();
            thread::sleep(Duration::from_millis(100));
        }

        // This array marks which workers responded. If fairly dealt each index should be set to 1
        let mut worker_responses = [0; MAX_OUTBOUND_MSG_PROCESSING_WORKERS as usize];

        let mut resp_count = 0;
        loop {
            for i in 0..MAX_OUTBOUND_MSG_PROCESSING_WORKERS as usize {
                if let Ok(_recv) = receivers[i].try_recv() {
                    resp_count += 1;
                    // If this worker responded multiple times then the message were not fairly dealt so bork the count
                    if worker_responses[i] > 0 {
                        worker_responses[i] = MAX_OUTBOUND_MSG_PROCESSING_WORKERS + 1;
                    } else {
                        worker_responses[i] = 1;
                    }
                }
            }

            // For this test we expect 1 message to reach each worker
            if resp_count >= MAX_OUTBOUND_MSG_PROCESSING_WORKERS as usize {
                break;
            }
        }

        // Confirm that the messages were fairly dealt to different worker threads
        assert_eq!(
            worker_responses.iter().fold(0, |acc, x| acc + x),
            MAX_OUTBOUND_MSG_PROCESSING_WORKERS
        );
    }

}
