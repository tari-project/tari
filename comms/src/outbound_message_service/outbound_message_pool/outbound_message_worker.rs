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

use super::{error::OutboundMessagePoolError, outbound_message_pool::OutboundMessagePoolConfig, OutboundMessage};
use crate::{
    connection::{Connection, Direction, InprocAddress, SocketEstablishment, ZmqContext},
    connection_manager::ConnectionManager,
    message::FrameSet,
    peer_manager::PeerManager,
};
use log::*;
use std::{
    sync::{
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};
use tari_utilities::message_format::MessageFormat;

const LOG_TARGET: &'static str = "comms::outbound_message_service::pool::worker";

/// Set the allocated stack size for each MessagePoolWorker thread
const THREAD_STACK_SIZE: usize = 256 * 1024; // 256kb

/// This is an instance of a single Worker thread for the Outbound Message Pool
pub struct MessagePoolWorker {
    config: OutboundMessagePoolConfig,
    context: ZmqContext,
    message_source_address: InprocAddress,
    failed_message_queue_address: InprocAddress,
    peer_manager: Arc<PeerManager>,
    connection_manager: Arc<ConnectionManager>,
    shutdown_receiver: Receiver<()>,
}

impl MessagePoolWorker {
    /// Start the MessagePoolWorker thread
    pub fn start(
        config: OutboundMessagePoolConfig,
        context: ZmqContext,
        message_source_address: InprocAddress,
        failed_message_queue_address: InprocAddress,
        peer_manager: Arc<PeerManager>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(thread::JoinHandle<Result<(), OutboundMessagePoolError>>, SyncSender<()>), OutboundMessagePoolError>
    {
        let (shutdown_signal, shutdown_receiver) = sync_channel(1);
        let mut worker = Self {
            config,
            context,
            message_source_address,
            failed_message_queue_address,
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
                             scope before shutdown was called. Exiting worker with and error."
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
        // Connection to the message dealer proxy
        let message_source_connection = Connection::new(&self.context, Direction::Inbound)
            .set_name("omp-message-source")
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.message_source_address)
            .map_err(OutboundMessagePoolError::ConnectionError)?;

        let failed_msg_connection = Connection::new(&self.context, Direction::Outbound)
            .set_name("omp-failed-message")
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.failed_message_queue_address)
            .map_err(OutboundMessagePoolError::ConnectionError)?;

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

            // Read any new messages to be sent.
            if let Some(frames) = connection_try!(message_source_connection.receive(1000)) {
                let _ = Self::deserialize_outbound_message(frames)
                    .or_else(|err| {
                        warn!(
                            target: LOG_TARGET,
                            "Unable to deserialize message ({:?}). Discarding it.", err
                        );
                        Err(err)
                    })
                    .and_then(|msg| {
                        match self.attempt_message_transmission(&msg) {
                            Ok(_) => debug!(
                                target: LOG_TARGET,
                                "Message successfully sent to NodeId {} after {} attempts",
                                msg.destination_node_id(),
                                msg.num_attempts()
                            ),
                            Err(err) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "(Attempt {} of {}) Failed to send message to peer ({}). {:?}. Sending to failed \
                                     queue.",
                                    msg.num_attempts(),
                                    self.config.max_retries,
                                    msg.destination_node_id(),
                                    err,
                                );
                                // Send to failed message queue
                                let msg_buf = msg.to_binary().map_err(OutboundMessagePoolError::MessageFormatError)?;
                                failed_msg_connection.send(&[msg_buf])?;
                            },
                        }

                        Ok(())
                    });
            }
        }
        Ok(())
    }

    /// Attempt to send a message to the NodeId specified in the message. If the the attempt is not successful then mark
    /// the failed connection attempt and requeue the message for another attempt
    fn attempt_message_transmission(&mut self, msg: &OutboundMessage) -> Result<(), OutboundMessagePoolError> {
        let peer = self
            .peer_manager
            .find_with_node_id(&msg.destination_node_id())
            .map_err(OutboundMessagePoolError::PeerManagerError)?;

        self.peer_manager
            .reset_connection_attempts(&peer.node_id)
            .map_err(OutboundMessagePoolError::PeerManagerError)?;
        let peer_connection = self
            .connection_manager
            .establish_connection_to_peer(&peer)
            .map_err(OutboundMessagePoolError::ConnectionManagerError)?;

        let frames = msg.message_frames().clone();
        debug!(
            target: LOG_TARGET,
            "Sending {} frames to NodeId {}",
            frames.len(),
            peer.node_id
        );

        peer_connection
            .send(frames)
            .map_err(OutboundMessagePoolError::ConnectionError)?;

        Ok(())
    }

    fn deserialize_outbound_message(mut frames: FrameSet) -> Result<OutboundMessage, OutboundMessagePoolError> {
        // Discard the first two identity frames from the dealer proxy
        match frames.drain(2..).next() {
            Some(frame) => OutboundMessage::from_binary(&frame).map_err(OutboundMessagePoolError::MessageFormatError),
            None => Err(OutboundMessagePoolError::InvalidFrameFormat(format!(
                "Outbound message pool worker received a frame set with invalid length"
            ))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection_manager::PeerConnectionConfig,
        peer_manager::{NodeId, NodeIdentity},
    };
    use std::{fs, path::PathBuf};
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

    fn outbound_message_worker_setup(
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
    fn start() {
        let context = ZmqContext::new();
        let (peer_manager, connection_manager, _) = outbound_message_worker_setup(&context, "omw_start");
        let (handle, signal) = MessagePoolWorker::start(
            OutboundMessagePoolConfig::default(),
            context,
            InprocAddress::random(),
            InprocAddress::random(),
            peer_manager,
            connection_manager,
        )
        .unwrap();

        signal.send(()).unwrap();
        handle.join().unwrap().unwrap();
        clean_up_datastore("omw_start");
    }

    #[test]
    fn deserialize_outbound_message() {
        let mut msg = OutboundMessage::new(NodeId::new(), vec![vec![]]);
        msg.mark_failed_attempt();
        let frames = vec![vec![1, 2, 3, 4], vec![1, 2, 3, 4], msg.to_binary().unwrap()];
        let result_msg = MessagePoolWorker::deserialize_outbound_message(frames).unwrap();
        assert_eq!(result_msg.num_attempts(), 1);
    }
}
