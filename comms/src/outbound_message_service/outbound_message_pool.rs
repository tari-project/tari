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
        zmq::{Context, InprocAddress},
        DealerProxy,
        DealerProxyError,
    },
    outbound_message_service::{MessagePoolWorker, OutboundError},
    peer_manager::PeerManager,
};

use log::*;
#[cfg(test)]
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::{hash::Hash, sync::Arc};
use tari_crypto::keys::PublicKey;
use tari_storage::keyvalue_store::DataStore;

/// The maximum number of processing worker threads that will be created by the OutboundMessageService
const MAX_OUTBOUND_MSG_PROCESSING_WORKERS: u8 = 8;

const LOG_TARGET: &'static str = "comms::outbound_message_service::pool";

/// The OutboundMessagePool will field outbound messages received from multiple OutboundMessageService instance that
/// it will receive via the Inbound Inproc connection. It will handle the messages in the queue one at a time and
/// attempt to send them. If they cannot be sent then the Retry count will be incremented and the message pushed to
/// the back of the queue.
struct OutboundMessagePool<P, DS> {
    context: Context,
    message_queue_address: InprocAddress,
    worker_dealer_address: InprocAddress,
    peer_manager: Arc<PeerManager<P, DS>>,
    #[cfg(test)]
    test_sync_sender: Vec<SyncSender<String>>, /* These channels will be to test the pool workers threaded
                                                * operation */
}
impl<P, DS> OutboundMessagePool<P, DS>
where
    P: PublicKey + Hash + Send + Sync + 'static,
    DS: DataStore + Send + Sync + 'static,
{
    /// Construct a new Outbound Message Pool.
    /// # Arguments
    /// `context` - A ZeroMQ context  
    /// `message_queue_address` - The InProc address that will be used to send message to this message pool  
    /// `peer_manager` - a reference to the peer manager to be used when sending messages
    pub fn new(
        context: Context,
        message_queue_address: InprocAddress,
        peer_manager: Arc<PeerManager<P, DS>>,
    ) -> Result<OutboundMessagePool<P, DS>, OutboundError>
    {
        Ok(OutboundMessagePool {
            context,
            message_queue_address,
            worker_dealer_address: InprocAddress::random(),
            peer_manager,
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
                    self.context.clone(),
                    self.worker_dealer_address.clone(),
                    self.message_queue_address.clone(),
                    self.peer_manager.clone(),
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
    use super::*;
    use crate::{connection::Context, outbound_message_service::outbound_message_service::OutboundMessageService};
    use std::sync::Arc;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    use crate::{message::MessageFlags, outbound_message_service::BroadcastStrategy};

    use crate::{
        connection::{net_address::net_addresses::MAX_CONNECTION_ATTEMPTS, NetAddress, NetAddresses},
        peer_manager::{peer::PeerFlags, NodeId, Peer},
    };

    #[test]
    /// Test that when a message is sent via the pool that it is retried and requeued the correct amount of times and
    /// that ConnectionRetryAttempts error is thrown
    fn test_requeuing_messages() {
        let mut rng = rand::OsRng::new().unwrap();
        let context = Context::new();
        let omp_inbound_address = InprocAddress::random();
        let peer_manager = Arc::new(PeerManager::new(None).unwrap());

        let mut omp =
            OutboundMessagePool::new(context.clone(), omp_inbound_address.clone(), peer_manager.clone()).unwrap();

        let (_dest_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = NetAddresses::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let dest_peer: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());
        peer_manager.add_peer(dest_peer.clone()).unwrap();
        let oms = OutboundMessageService::new(context, omp_inbound_address, peer_manager.clone()).unwrap();
        let receivers = omp.create_test_channels();
        let _omp = omp.start();
        let message_envelope_body: Vec<u8> = vec![0, 1, 2, 3];
        oms.send(
            BroadcastStrategy::Direct(dest_peer.node_id.clone()),
            MessageFlags::ENCRYPTED,
            &message_envelope_body,
            &mut rng,
        )
        .unwrap();

        // This array marks which workers responded. If fairly dealt each index should be set to 1
        let mut worker_responses = [0; MAX_OUTBOUND_MSG_PROCESSING_WORKERS as usize];
        let expected_responses = vec!["Attempt 1", "Attempt 2", "Attempt 3", "Connection Attempts Exceeded"];

        let mut resp_count = 0;
        loop {
            for i in 0..MAX_OUTBOUND_MSG_PROCESSING_WORKERS as usize {
                if let Ok(recv) = receivers[i].try_recv() {
                    assert_eq!(recv, expected_responses[resp_count].to_string());
                    resp_count += 1;

                    // If this worker responded multiple times then the message were not fairly dealt so bork the count
                    if worker_responses[i] > 0 {
                        worker_responses[i] = MAX_OUTBOUND_MSG_PROCESSING_WORKERS + 1;
                    } else {
                        worker_responses[i] = 1;
                    }
                }
            }

            // For this test we expect 3 retries + the response for the Connection Attempts Exceeded error
            if resp_count >= MAX_CONNECTION_ATTEMPTS as usize + 1 {
                break;
            }
        }

        // Confirm that the messages were fairly dealt to different worker threads
        assert_eq!(
            worker_responses.iter().fold(0, |acc, x| acc + x),
            MAX_CONNECTION_ATTEMPTS as u8 + 1
        );
    }
}
