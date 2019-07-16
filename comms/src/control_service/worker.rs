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
    error::ControlServiceError,
    messages::{ControlServiceMessageType, RequestConnection},
    service::ControlServiceConfig,
    types::{ControlMessage, Result},
};
use crate::{
    connection::{connection::EstablishedConnection, types::Direction, Connection, PeerConnection, ZmqContext},
    connection_manager::ConnectionManager,
    control_service::messages::{ConnectRequestOutcome, Pong},
    message::{
        p2p::Accept,
        Frame,
        FrameSet,
        Message,
        MessageEnvelope,
        MessageEnvelopeHeader,
        MessageFlags,
        MessageHeader,
        NodeDestination,
    },
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags, PeerManagerError},
    types::{CommsCipher, CommsPublicKey},
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    convert::TryInto,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};
use tari_crypto::keys::DiffieHellmanSharedSecret;
use tari_utilities::{byte_array::ByteArray, ciphers::cipher::Cipher, message_format::MessageFormat};

const LOG_TARGET: &str = "comms::control_service::worker";
/// The maximum message size allowed for the control service.
/// Messages will transparently drop if this size is exceeded.
const CONTROL_SERVICE_MAX_MSG_SIZE: u64 = 1024; // 1kb

/// Set the allocated stack size for each ControlServiceWorker thread
const THREAD_STACK_SIZE: usize = 256 * 1024; // 256kb

/// The [ControlService] worker is responsible for handling incoming messages
/// to the control port and dispatching them using the message dispatcher.
pub struct ControlServiceWorker<MType>
where MType: Clone
{
    config: ControlServiceConfig<MType>,
    receiver: Receiver<ControlMessage>,
    is_running: bool,
    connection_manager: Arc<ConnectionManager>,
    node_identity: Arc<NodeIdentity>,
    listener: EstablishedConnection,
}

impl<MType> ControlServiceWorker<MType>
where
    MType: Send + Sync + 'static,
    MType: Serialize + DeserializeOwned,
    MType: Clone,
{
    /// Start the worker
    ///
    /// # Arguments
    /// - `context` - Connection context
    /// - `config` - ControlServiceConfig
    /// - `connection_manager` - the `ConnectionManager`
    pub fn start(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        config: ControlServiceConfig<MType>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(thread::JoinHandle<Result<()>>, SyncSender<ControlMessage>)>
    {
        let (sender, receiver) = sync_channel(5);

        let handle = thread::Builder::new()
            .name("control-service-worker-thread".to_string())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || {
                info!(
                    target: LOG_TARGET,
                    "Control service starting on {}...", config.listener_address
                );

                let listener = Self::establish_listener(&context, &config)?;
                let mut worker = Self::new(node_identity, config, connection_manager, receiver, listener);

                loop {
                    match worker.run() {
                        Ok(_) => {
                            info!(target: LOG_TARGET, "Control service exiting loop.");
                            break;
                        },

                        Err(err) => {
                            error!(target: LOG_TARGET, "Worker exited with an error: {:?}", err);
                            info!(target: LOG_TARGET, "Restarting control service after 1 second.");
                            thread::sleep(Duration::from_millis(1000));
                        },
                    }
                }

                Ok(())
            })
            .map_err(|_| ControlServiceError::WorkerThreadFailedToStart)?;

        Ok((handle, sender))
    }

    fn new(
        node_identity: Arc<NodeIdentity>,
        config: ControlServiceConfig<MType>,
        connection_manager: Arc<ConnectionManager>,
        receiver: Receiver<ControlMessage>,
        listener: EstablishedConnection,
    ) -> Self
    {
        Self {
            config,
            connection_manager,
            is_running: true,
            node_identity,
            receiver,
            listener,
        }
    }

    fn run(&mut self) -> Result<()> {
        debug!(target: LOG_TARGET, "Control service started");
        loop {
            // Read incoming messages
            if let Some(frames) = connection_try!(self.listener.receive(100)) {
                debug!(target: LOG_TARGET, "Received {} frames", frames.len());
                match self.process_message(frames) {
                    Ok(_) => info!(target: LOG_TARGET, "Message processed"),
                    Err(err) => error!(target: LOG_TARGET, "Error when processing message: {:?}", err),
                }
            }

            // Process control messages
            self.process_control_messages()?;

            if !self.is_running {
                break;
            }
        }

        Ok(())
    }

    fn process_control_messages(&mut self) -> Result<()> {
        if let Some(msg) = self.receiver.recv_timeout(Duration::from_millis(5)).ok() {
            debug!(target: LOG_TARGET, "Received control message: {:?}", msg);
            match msg {
                ControlMessage::Shutdown => {
                    info!(target: LOG_TARGET, "Shutting down control service");
                    self.is_running = false;
                },
            }
        }
        Ok(())
    }

    fn process_message(&self, mut frames: FrameSet) -> Result<()> {
        if frames.is_empty() {
            // This case should never happen as ZMQ_ROUTER adds the identity frame
            warn!(target: LOG_TARGET, "Received empty frames from socket.");
            return Ok(());
        }

        let envelope: MessageEnvelope = frames
            .drain(1..)
            .collect::<FrameSet>()
            .try_into()
            .map_err(ControlServiceError::MessageError)?;

        let identity = frames
            .pop()
            .expect("Should not happen: drained all frames but the first, but then could not pop the first frame.");

        let envelope_header = envelope.deserialize_header()?;
        if !envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
            return Err(ControlServiceError::ReceivedUnencryptedMessage);
        }

        let maybe_peer = self.get_peer(&envelope_header.source)?;
        if maybe_peer.map(|p| p.is_banned()).unwrap_or(false) {
            return Err(ControlServiceError::PeerBanned);
        }

        let decrypted_body = self.decrypt_body(envelope.body_frame(), &envelope_header.source)?;
        let message =
            Message::from_binary(decrypted_body.as_bytes()).map_err(ControlServiceError::MessageFormatError)?;

        debug!(target: LOG_TARGET, "Handling message");
        self.handle_message(identity, envelope_header, message)
    }

    fn handle_message(
        &self,
        identity_frame: Frame,
        envelope_header: MessageEnvelopeHeader,
        msg: Message,
    ) -> Result<()>
    {
        let header = msg.deserialize_header().map_err(ControlServiceError::MessageError)?;

        match header.message_type {
            ControlServiceMessageType::Ping => self.handle_ping(envelope_header, identity_frame),
            ControlServiceMessageType::RequestConnection => {
                self.handle_request_connection(envelope_header, identity_frame, msg.deserialize_message()?)
            },
            _ => Err(ControlServiceError::InvalidMessageReceived),
        }
    }

    fn handle_ping(&self, envelope_header: MessageEnvelopeHeader, identity_frame: Frame) -> Result<()> {
        debug!(target: LOG_TARGET, "Got ping message");
        self.send_reply(
            &envelope_header.source,
            identity_frame,
            ControlServiceMessageType::Pong,
            Pong {},
        )
    }

    fn handle_request_connection(
        &self,
        envelope_header: MessageEnvelopeHeader,
        identity_frame: Frame,
        message: RequestConnection,
    ) -> Result<()>
    {
        debug!(
            target: LOG_TARGET,
            "RequestConnection message received (node_id={}, address={})", message.node_id, message.address
        );

        let pm = &self.connection_manager.peer_manager();
        let public_key = &envelope_header.source;
        let peer = match pm.find_with_public_key(&public_key) {
            Ok(peer) => {
                if peer.is_banned() {
                    return Err(ControlServiceError::PeerBanned);
                }

                pm.add_net_address(&peer.node_id, &message.control_service_address)
                    .map_err(ControlServiceError::PeerManagerError)?;

                peer
            },
            Err(PeerManagerError::PeerNotFoundError) => {
                let node_id = &message.node_id;

                let peer = Peer::new(
                    public_key.clone(),
                    node_id.clone(),
                    message.control_service_address.clone().into(),
                    PeerFlags::empty(),
                );

                pm.add_peer(peer.clone())
                    .map_err(ControlServiceError::PeerManagerError)?;
                peer
            },
            Err(err) => return Err(ControlServiceError::PeerManagerError(err)),
        };

        // TODO: SECURITY The node ID is not a verified value at this point (PeerNotFoundError branch above).
        //       An attacker can insert any node id they want to get information about other peers connections
        //       to this node. For instance, if they already have an active connection.
        //       The public key should be used as that is validated by the message signature.

        let conn_manager = &self.connection_manager;

        if conn_manager.has_establish_lock(&peer.node_id) {
            warn!(
                target: LOG_TARGET,
                "COLLISION DETECTED: this node is attempting to connect to the same node which is asking to connect."
            );
            if self.should_reject_collision(&peer.node_id) {
                warn!(
                    target: LOG_TARGET,
                    "This connection attempt should be rejected. Rejecting the request to connect"
                );
                self.reject_connection(envelope_header, identity_frame)?;
                return Ok(());
            }
        }

        if let Some(conn) = conn_manager.get_connection(&peer) {
            if conn.is_active() {
                debug!(
                    target: LOG_TARGET,
                    "Already have active connection to peer. Ignoring the request for connection."
                );
                self.reject_connection(envelope_header, identity_frame)?;
                return Ok(());
            }
        }

        self.accept_connection(envelope_header, identity_frame)?;

        debug!(
            target: LOG_TARGET,
            "Connecting to requested address {}", message.address
        );

        let conn = conn_manager
            .establish_requested_outbound_connection(&peer, message.address.clone(), message.server_key)
            .map_err(ControlServiceError::ConnectionManagerError)?;

        conn.wait_connected_or_failure(&self.config.requested_outbound_connection_timeout)
            .map_err(ControlServiceError::ConnectionError)?;
        debug!(
            target: LOG_TARGET,
            "Connection to requested address {} succeeded", message.address
        );

        self.send_message_to_peer(&conn, &peer, self.config.accept_message_type.clone(), Accept {})?;

        debug!(
            target: LOG_TARGET,
            "Sent 'Accept' message to address {:?}",
            conn.get_connected_address()
        );

        Ok(())
    }

    fn should_reject_collision(&self, node_id: &NodeId) -> bool {
        &self.node_identity.identity.node_id < node_id
    }

    fn reject_connection(&self, envelope_header: MessageEnvelopeHeader, identity: Frame) -> Result<()> {
        self.send_reply(
            &envelope_header.source,
            identity,
            ControlServiceMessageType::ConnectRequestOutcome,
            ConnectRequestOutcome::Rejected,
        )
    }

    fn accept_connection(&self, envelope_header: MessageEnvelopeHeader, identity: Frame) -> Result<()> {
        self.send_reply(
            &envelope_header.source,
            identity,
            ControlServiceMessageType::ConnectRequestOutcome,
            ConnectRequestOutcome::Accepted,
        )
    }

    fn get_peer(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>> {
        let peer_manager = &self.connection_manager.peer_manager();
        match peer_manager.find_with_public_key(public_key) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) => Ok(None),
            Err(err) => Err(ControlServiceError::PeerManagerError(err)),
        }
    }

    fn send_message_to_peer<T>(
        &self,
        peer_conn: &PeerConnection,
        peer: &Peer,
        message_type: MType,
        msg: T,
    ) -> Result<()>
    where
        T: MessageFormat,
    {
        let envelope = self.construct_envelope(&peer.public_key, message_type, msg, MessageFlags::ENCRYPTED)?;

        peer_conn
            .send(envelope.into_frame_set())
            .map_err(ControlServiceError::ConnectionError)?;

        Ok(())
    }

    fn construct_envelope<T, MT>(
        &self,
        dest_public_key: &CommsPublicKey,
        message_type: MT,
        msg: T,
        flags: MessageFlags,
    ) -> Result<MessageEnvelope>
    where
        T: MessageFormat,
        MT: Serialize + DeserializeOwned,
        MT: MessageFormat,
    {
        let header = MessageHeader { message_type };
        let msg = Message::from_message_format(header, msg).map_err(ControlServiceError::MessageError)?;

        MessageEnvelope::construct(
            &self.node_identity,
            dest_public_key.clone(),
            NodeDestination::PublicKey(dest_public_key.clone()),
            msg.to_binary().map_err(ControlServiceError::MessageFormatError)?,
            flags,
        )
        .map_err(ControlServiceError::MessageError)
    }

    fn send_reply<T>(
        &self,
        dest_public_key: &CommsPublicKey,
        identity_frame: Frame,
        message_type: ControlServiceMessageType,
        msg: T,
    ) -> Result<()>
    where
        T: MessageFormat,
    {
        let envelope = self.construct_envelope(dest_public_key, message_type, msg, MessageFlags::ENCRYPTED)?;
        let mut frames = vec![identity_frame];

        frames.extend(envelope.into_frame_set());

        self.listener.send(frames).map_err(ControlServiceError::ConnectionError)
    }

    fn decrypt_body(&self, body: &Frame, public_key: &CommsPublicKey) -> Result<Frame> {
        let ecdh_shared_secret = CommsPublicKey::shared_secret(&self.node_identity.secret_key, public_key).to_vec();
        CommsCipher::open_with_integral_nonce(&body, &ecdh_shared_secret).map_err(ControlServiceError::CipherError)
    }

    fn establish_listener(context: &ZmqContext, config: &ControlServiceConfig<MType>) -> Result<EstablishedConnection> {
        debug!(target: LOG_TARGET, "Binding on address: {}", config.listener_address);
        Connection::new(&context, Direction::Inbound)
            .set_name("Control Service Listener")
            .set_receive_hwm(10)
            .set_max_message_size(Some(CONTROL_SERVICE_MAX_MSG_SIZE))
            .set_socks_proxy_addr(config.socks_proxy_address.clone())
            .establish(&config.listener_address)
            .map_err(ControlServiceError::BindFailed)
    }
}
