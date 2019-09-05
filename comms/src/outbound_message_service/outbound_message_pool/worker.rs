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

use super::{
    error::OutboundMessagePoolError,
    pool::OutboundMessagePoolConfig,
    retry_queue::{RetryBucket, RetryQueue},
    OutboundMessage,
};
use crate::{
    connection::PeerConnection,
    connection_manager::ConnectionManager,
    peer_manager::{NodeId, Peer, PeerManager},
};
use crossbeam_channel::{self as channel, Receiver, RecvTimeoutError, Sender};
use crossbeam_deque::{Steal, Stealer};
use log::*;
use std::{sync::Arc, thread, time::Duration};

const LOG_TARGET: &str = "comms::outbound_message_service::pool::worker";
/// Set the allocated stack size for each MessagePoolWorker thread
const THREAD_STACK_SIZE: usize = 256 * 1024; // 256kb

/// This is an instance of a single Worker thread for the Outbound Message Pool
pub struct MessagePoolWorker {
    config: OutboundMessagePoolConfig,
    stealer: Stealer<OutboundMessage>,
    retry_queue: RetryQueue<NodeId, OutboundMessage>,
    peer_manager: Arc<PeerManager>,
    connection_manager: Arc<ConnectionManager>,
    shutdown_receiver: Receiver<()>,
}

impl MessagePoolWorker {
    /// Start the MessagePoolWorker thread
    pub fn start(
        config: OutboundMessagePoolConfig,
        stealer: Stealer<OutboundMessage>,
        retry_queue: RetryQueue<NodeId, OutboundMessage>,
        peer_manager: Arc<PeerManager>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(thread::JoinHandle<Result<(), OutboundMessagePoolError>>, Sender<()>), OutboundMessagePoolError>
    {
        let (shutdown_signal, shutdown_receiver) = channel::bounded(1);
        let mut worker = Self {
            config,
            stealer,
            retry_queue,
            peer_manager,
            connection_manager,
            shutdown_receiver,
        };

        let thread_handle = thread::Builder::new()
            .name("message-pool-worker-thread".to_string())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || loop {
                match worker.run() {
                    Ok(_) => break Ok(()),
                    Err(err @ OutboundMessagePoolError::WorkerShutdownSignalDisconnected) => {
                        error!(target: LOG_TARGET, "Message Pool worker exited: {:?}", err);
                        error!(
                            target: LOG_TARGET,
                            "The shutdown signal disconnected likely because the outbound message pool went out of \
                             scope before shutdown was called. Exiting worker with an error."
                        );
                        break Err(err);
                    },
                    Err(err) => {
                        error!(target: LOG_TARGET, "Outbound Message Pool worker exited: {:?}", err);
                        warn!(target: LOG_TARGET, "Restarting outbound message worker after failure.");
                        // Sleep so that if this service continually restarts, we don't get high CPU usage
                        thread::sleep(Duration::from_secs(1));
                    },
                }
            })
            .map_err(|_| OutboundMessagePoolError::ThreadInitializationError)?;

        Ok((thread_handle, shutdown_signal))
    }

    /// Start MessagePoolWorker which will connect to the inbound message dealer, accept messages from the queue,
    /// attempt to send them and if it cannot send then requeue the message
    fn run(&mut self) -> Result<(), OutboundMessagePoolError> {
        loop {
            match self.shutdown_receiver.recv_timeout(Duration::from_millis(5)) {
                // Shut down signal received
                Ok(_) => break,
                // Nothing received
                Err(RecvTimeoutError::Timeout) => {},
                // Sender disconnected before sending the shutdown signal, this is an error
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(OutboundMessagePoolError::WorkerShutdownSignalDisconnected)
                },
            }

            // Check for new messages
            match self.stealer.steal() {
                Steal::Success(msg) => {
                    if self.retry_queue.contains(msg.destination_node_id()) {
                        // The retry queue has scheduled messages for this node_id,
                        // so rather than trying to send now, we add this to the
                        // retry queue so that they can all be sent in a batch when scheduled
                        // to do so
                        let node_id = msg.destination_node_id().clone();
                        debug!(
                            target: LOG_TARGET,
                            "Messages for this NodeId ({}) are scheduled for retry, adding this message to the retry \
                             queue",
                            node_id
                        );
                        self.retry_queue.add_item(node_id, msg);
                    } else {
                        // Attempt to send a single message
                        match self.attempt(&msg) {
                            Ok(peer) => debug!(
                                target: LOG_TARGET,
                                "Message successfully sent to NodeId={}", peer.node_id
                            ),
                            Err(err) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Failed to send message to peer ({}). {:?}. Sending to failed queue.",
                                    msg.destination_node_id(),
                                    err,
                                );
                                // Add to failed message queue
                                self.retry_queue.add_item(msg.destination_node_id().clone(), msg);
                            },
                        }
                    }
                },
                Steal::Empty => {
                    // No incoming messages, maybe the retry_queue has some work
                    if self.retry_queue.is_empty() {
                        // Nothing in retry queue, sleep for a bit
                        thread::sleep(Duration::from_millis(100));
                    } else {
                        // Work on a bucket in the retry queue
                        self.process_retry_queue();
                    }
                },
                Steal::Retry => {},
            }
        }

        Ok(())
    }

    fn process_retry_queue(&self) {
        if let Some((node_id, bucket)) = self.retry_queue.lease_next() {
            self.try_send_bucket(&node_id, bucket);
        }
    }

    fn try_send_bucket(&self, node_id: &NodeId, mut bucket: RetryBucket<OutboundMessage>) {
        match self.attempt_batch(&node_id, &mut bucket) {
            Ok(_) => {
                // Bucket has been sent - now we must send any messages that could have been
                // scheduled while the lease on the bucket was out
                if let Some(left_over) = self.retry_queue.remove(&node_id) {
                    self.try_send_bucket(node_id, left_over);
                }
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "(Attempt {} of {}) Failed to send message to peer ({}). {:?}.",
                    bucket.attempts(),
                    self.config.max_retries,
                    node_id,
                    err,
                );
                // Message bucket failed to send - put the bucket (with remaining messages) back on the
                // retry queue.
                bucket.incr_attempts();
                if bucket.attempts() > self.config.max_retries {
                    debug!(
                        target: LOG_TARGET,
                        "Unable to send message to NodeId {} after {} attempts. Message bucket discarded.",
                        node_id,
                        bucket.attempts(),
                    );
                    self.retry_queue.remove(node_id);
                    return;
                }

                if !self.retry_queue.return_lease(&node_id, bucket) {
                    // This should never happen
                    debug_assert!(false, "return_lease called for bucket which doesn't exist");
                    warn!(
                        target: LOG_TARGET,
                        "Lease on message bucket for NodeId '{}' was not returned", node_id
                    );
                }
            },
        }
    }

    fn attempt_batch(
        &self,
        node_id: &NodeId,
        bucket: &mut RetryBucket<OutboundMessage>,
    ) -> Result<(), OutboundMessagePoolError>
    {
        self.attempt_establish_connection(&node_id)
            .and_then(|(_, conn)| self.send_batch(&conn, bucket))
            .or_else(|err| {
                debug!(
                    target: LOG_TARGET,
                    "(Attempt {} of {}) Failed to send message to peer ({}). {:?}. Sending to failed queue.",
                    bucket.attempts(),
                    self.config.max_retries,
                    node_id,
                    err,
                );
                Err(err)
            })
    }

    fn attempt(&self, msg: &OutboundMessage) -> Result<Peer, OutboundMessagePoolError> {
        self.attempt_establish_connection(msg.destination_node_id())
            .and_then(|(peer, conn)| self.send_msg(&conn, msg).map(|_| peer))
    }

    /// Attempt to send a message to the NodeId specified in the message. If the the attempt is not successful then mark
    /// the failed connection attempt and requeue the message for another attempt
    fn attempt_establish_connection(
        &self,
        node_id: &NodeId,
    ) -> Result<(Peer, Arc<PeerConnection>), OutboundMessagePoolError>
    {
        let peer = self
            .peer_manager
            .find_with_node_id(node_id)
            .map_err(OutboundMessagePoolError::PeerManagerError)?;

        let peer_connection = self
            .connection_manager
            .establish_connection_to_peer(&peer)
            .map_err(OutboundMessagePoolError::ConnectionManagerError)?;

        Ok((peer, peer_connection))
    }

    fn send_batch(
        &self,
        connection: &PeerConnection,
        bucket: &mut RetryBucket<OutboundMessage>,
    ) -> Result<(), OutboundMessagePoolError>
    {
        while let Some(msg) = bucket.pop_front() {
            self.send_msg(connection, &msg).or_else(|err| {
                bucket.push_front(msg);
                Err(err)
            })?;
        }

        Ok(())
    }

    fn send_msg(
        &self,
        peer_connection: &PeerConnection,
        msg: &OutboundMessage,
    ) -> Result<(), OutboundMessagePoolError>
    {
        // TODO: Cloning here due to PeerConnection requiring ownership. Investigate if PeerConnection
        //       could send an Arc<FrameSet> to eliminate the need to clone bytes.
        let frames = msg.message_frames().clone();

        peer_connection
            .send(frames)
            .map_err(OutboundMessagePoolError::ConnectionError)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{connection::ZmqContext, connection_manager::PeerConnectionConfig, peer_manager::NodeIdentity};
    use crossbeam_deque::Worker;
    use futures::channel::mpsc::channel;
    use tari_storage::HMapDatabase;
    use tari_utilities::thread_join::ThreadJoinWithTimeout;

    fn make_peer_connection_config() -> PeerConnectionConfig {
        PeerConnectionConfig {
            peer_connection_establish_timeout: Duration::from_millis(10),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 1,
            max_connections: 10,
            socks_proxy_address: None,
        }
    }

    fn outbound_message_worker_setup() -> (Arc<PeerManager>, Arc<ConnectionManager>, Arc<NodeIdentity>) {
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
        let peer_manager = Arc::new(PeerManager::new(HMapDatabase::new()).unwrap());
        let (tx, _rx) = channel(10);
        // Connection Manager
        let connection_manager = Arc::new(ConnectionManager::new(
            context,
            node_identity.clone(),
            peer_manager.clone(),
            make_peer_connection_config(),
            tx,
        ));

        (peer_manager, connection_manager, node_identity)
    }

    #[test]
    fn start_shutdown() {
        let (peer_manager, connection_manager, _) = outbound_message_worker_setup();
        let worker = Worker::new_fifo();
        let stealer = worker.stealer();
        let (handle, signal) = MessagePoolWorker::start(
            OutboundMessagePoolConfig::default(),
            stealer,
            RetryQueue::new(),
            peer_manager,
            connection_manager,
        )
        .unwrap();

        signal.send(()).unwrap();
        handle.timeout_join(Duration::from_millis(3000)).unwrap();
    }
}
