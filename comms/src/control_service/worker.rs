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
    messages::{
        MessageHeader,
        MessageType,
        PongMessage,
        RejectReason,
        RequestConnectionMessage,
        RequestConnectionOutcome,
    },
    service::ControlServiceConfig,
    types::{ControlMessage, Result},
};
use crate::{
    connection::{
        connection::EstablishedConnection,
        types::Direction,
        Connection,
        ConnectionError,
        CurvePublicKey,
        NetAddress,
        ZmqContext,
    },
    connection_manager::{ConnectionManager, EstablishLockResult},
    message::{Envelope, EnvelopeBody, Frame, FrameSet, MessageEnvelopeHeader, MessageExt, MessageFlags},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManagerError},
    types::CommsPublicKey,
    utils::crypt,
};
use log::*;
use prost::Message;
use std::{
    convert::TryInto,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};
use tari_utilities::byte_array::ByteArray;

const LOG_TARGET: &str = "comms::control_service::worker";
/// The maximum message size allowed for the control service.
/// Messages will transparently drop if this size is exceeded.
const CONTROL_SERVICE_MAX_MSG_SIZE: u64 = 1024; // 1kb

/// Set the allocated stack size for each ControlServiceWorker thread
const THREAD_STACK_SIZE: usize = 256 * 1024; // 256kb

/// The [ControlService] worker is responsible for handling incoming messages
/// to the control port and dispatching them using the message dispatcher.
pub struct ControlServiceWorker {
    config: ControlServiceConfig,
    receiver: Receiver<ControlMessage>,
    is_running: bool,
    connection_manager: Arc<ConnectionManager>,
    node_identity: Arc<NodeIdentity>,
    listener: EstablishedConnection,
}

impl ControlServiceWorker {
    /// Start the worker
    ///
    /// # Arguments
    /// - `context` - Connection context
    /// - `config` - ControlServiceConfig
    /// - `connection_manager` - the `ConnectionManager`
    pub fn start(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        config: ControlServiceConfig,
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
        config: ControlServiceConfig,
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
        if frames.len() < 2 {
            debug!(
                target: LOG_TARGET,
                "Insufficient frames received (Received: {}, Want: 2)",
                frames.len()
            );
            return Ok(());
        }

        let identity_frame = frames.remove(0);
        let envelope_frame = frames.remove(0);

        let envelope = Envelope::decode(envelope_frame)?;

        let envelope_header: MessageEnvelopeHeader = envelope
            .header
            .ok_or(ControlServiceError::InvalidEnvelope)?
            .try_into()?;

        if !envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
            return Err(ControlServiceError::ReceivedUnencryptedMessage);
        }

        let maybe_peer = self.get_peer(&envelope_header.public_key)?;
        if maybe_peer.map(|p| p.is_banned()).unwrap_or(false) {
            return Err(ControlServiceError::PeerBanned);
        }

        let decrypted_body = self.decrypt_body(&envelope.body, &envelope_header.public_key)?;
        let body = EnvelopeBody::decode(decrypted_body)?;

        debug!(target: LOG_TARGET, "Handling message");
        self.handle_message(envelope_header, identity_frame, body)
    }

    fn handle_message(
        &self,
        envelope_header: MessageEnvelopeHeader,
        identity_frame: Frame,
        envelope_body: EnvelopeBody,
    ) -> Result<()>
    {
        let header = envelope_body
            .decode_part::<MessageHeader>(0)?
            .ok_or(ControlServiceError::InvalidEnvelopeBody)?;

        match MessageType::from_i32(header.message_type).ok_or(ControlServiceError::InvalidMessageType)? {
            MessageType::None => {
                debug!(
                    target: LOG_TARGET,
                    "Received None message type from public key '{}'", envelope_header.public_key
                );
                Err(ControlServiceError::UnrecognisedMessageType)
            },
            MessageType::Ping => self.handle_ping(envelope_header, identity_frame),
            MessageType::RequestConnection => {
                let msg = envelope_body
                    .decode_part(1)?
                    .ok_or(ControlServiceError::InvalidEnvelopeBody)?;
                self.handle_request_connection(envelope_header, identity_frame, msg)
            },
            _ => Err(ControlServiceError::UnrecognisedMessageType),
        }
    }

    fn handle_ping(&self, envelope_header: MessageEnvelopeHeader, identity_frame: Frame) -> Result<()> {
        debug!(target: LOG_TARGET, "Got ping message");
        self.send_reply(
            &envelope_header.public_key,
            identity_frame,
            MessageType::Pong,
            PongMessage {},
        )
    }

    fn handle_request_connection(
        &self,
        envelope_header: MessageEnvelopeHeader,
        identity_frame: Frame,
        message: RequestConnectionMessage,
    ) -> Result<()>
    {
        let RequestConnectionMessage {
            node_id,
            control_service_address,
            features,
        } = message;

        let node_id = self.validate_node_id(&envelope_header.public_key, &node_id)?;
        let control_service_address = control_service_address.parse::<NetAddress>()?;
        let peer_features = PeerFeatures::from_bits_truncate(features);

        debug!(
            target: LOG_TARGET,
            "RequestConnection message received with NodeId {} (features: {:?})", node_id, peer_features,
        );

        let pm = &self.connection_manager.peer_manager();
        let public_key = &envelope_header.public_key;
        let peer = match pm.find_by_public_key(&public_key) {
            Ok(peer) => {
                if peer.is_banned() {
                    return Err(ControlServiceError::PeerBanned);
                }

                pm.update_peer(
                    &peer.public_key,
                    None,
                    Some(vec![control_service_address.clone()]),
                    None,
                    Some(peer_features),
                    None,
                )?;

                peer
            },
            Err(PeerManagerError::PeerNotFoundError) => {
                let peer = Peer::new(
                    public_key.clone(),
                    node_id,
                    control_service_address.into(),
                    PeerFlags::empty(),
                    peer_features,
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
        let establish_lock_result = conn_manager.try_acquire_establish_lock(&peer.node_id, || {
            self.establish_connection_protocol(&peer, &envelope_header, identity_frame.clone())
        });

        match establish_lock_result {
            EstablishLockResult::Ok(result) => result,
            EstablishLockResult::Collision => {
                warn!(
                    target: LOG_TARGET,
                    "COLLISION DETECTED: this node is attempting to connect to the same node which is asking to \
                     connect."
                );
                if self.should_reject_collision(&peer.node_id) {
                    warn!(
                        target: LOG_TARGET,
                        "This connection attempt should be rejected. Rejecting the request to connect"
                    );
                    self.reject_connection(&envelope_header, identity_frame, RejectReason::CollisionDetected)?;
                    Ok(())
                } else {
                    conn_manager.with_establish_lock(&peer.node_id, || {
                        self.establish_connection_protocol(&peer, &envelope_header, identity_frame)
                    })
                }
            },
        }
    }

    fn validate_node_id(&self, public_key: &CommsPublicKey, raw_node_id: &[u8]) -> Result<NodeId> {
        // The reason that we check the given node id against what we expect instead of just using the given node id
        // is in future the NodeId may not necessarily be derived from the public key (i.e. DAN node is registered on
        // the base layer)
        let expected_node_id = NodeId::from_key(public_key).map_err(|_| ControlServiceError::InvalidNodeId)?;
        let node_id = NodeId::from_bytes(&raw_node_id).map_err(|_| ControlServiceError::InvalidNodeId)?;
        if expected_node_id == node_id {
            Ok(expected_node_id)
        } else {
            // TODO: Misbehaviour?
            Err(ControlServiceError::InvalidNodeId)
        }
    }

    fn establish_connection_protocol(
        &self,
        peer: &Peer,
        envelope_header: &MessageEnvelopeHeader,
        identity_frame: Frame,
    ) -> Result<()>
    {
        let conn_manager = &self.connection_manager;
        if let Some(conn) = conn_manager.get_connection(peer) {
            if conn.is_active() {
                debug!(
                    target: LOG_TARGET,
                    "Already have active connection to peer. Rejecting the request for connection."
                );
                self.reject_connection(&envelope_header, identity_frame, RejectReason::ExistingConnection)?;
                return Ok(());
            }
        }

        conn_manager
            .with_new_inbound_connection(&peer, |new_inbound_conn, curve_public_key| {
                let address = new_inbound_conn
                    .get_address()
                    .ok_or(ControlServiceError::ConnectionAddressNotEstablished)?;

                debug!(
                    target: LOG_TARGET,
                    "[NodeId={}] Inbound peer connection established on address {}", peer.node_id, address
                );

                // Create an address which can be connected to externally
                let our_host = self.node_identity.control_service_address().host();
                let external_address = address
                    .maybe_port()
                    .map(|port| format!("{}:{}", our_host, port))
                    .or(Some(our_host))
                    .unwrap()
                    .parse()
                    .map_err(ControlServiceError::NetAddressError)?;

                debug!(
                    target: LOG_TARGET,
                    "Accepting peer connection request for NodeId={:?} on address {}", peer.node_id, external_address
                );

                self.accept_connection_request(&envelope_header, identity_frame, curve_public_key, external_address)?;

                match new_inbound_conn.wait_connected_or_failure(&self.config.requested_connection_timeout) {
                    Ok(_) => {
                        debug!(
                            target: LOG_TARGET,
                            "Connection to peer connection for NodeId {} succeeded", peer.node_id,
                        );

                        Ok(Some(new_inbound_conn))
                    },
                    Err(ConnectionError::Timeout) => Ok(None),
                    Err(err) => Err(ControlServiceError::ConnectionError(err)),
                }
            })
            .map_err(|err| ControlServiceError::ConnectionProtocolFailed(format!("{}", err)))?;

        Ok(())
    }

    fn should_reject_collision(&self, node_id: &NodeId) -> bool {
        self.node_identity.node_id() < node_id
    }

    fn reject_connection(
        &self,
        envelope_header: &MessageEnvelopeHeader,
        identity: Frame,
        reject_reason: RejectReason,
    ) -> Result<()>
    {
        self.send_reply(
            &envelope_header.public_key,
            identity,
            MessageType::ConnectRequestOutcome,
            RequestConnectionOutcome {
                accepted: false,
                curve_public_key: Default::default(),
                address: Default::default(),
                reject_reason: reject_reason as i32,
            },
        )
    }

    fn accept_connection_request(
        &self,
        envelope_header: &MessageEnvelopeHeader,
        identity: Frame,
        curve_public_key: CurvePublicKey,
        address: NetAddress,
    ) -> Result<()>
    {
        self.send_reply(
            &envelope_header.public_key,
            identity,
            MessageType::ConnectRequestOutcome,
            RequestConnectionOutcome {
                accepted: true,
                curve_public_key: curve_public_key.to_vec(),
                address: address.to_string(),
                reject_reason: RejectReason::None as i32,
            },
        )
    }

    fn get_peer(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>> {
        let peer_manager = &self.connection_manager.peer_manager();
        match peer_manager.find_by_public_key(public_key) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) => Ok(None),
            Err(err) => Err(ControlServiceError::PeerManagerError(err)),
        }
    }

    fn construct_envelope<T>(
        &self,
        dest_public_key: &CommsPublicKey,
        message_type: MessageType,
        msg: T,
    ) -> Result<Envelope>
    where
        T: prost::Message,
    {
        let header = MessageHeader::new(message_type);
        let body_bytes = wrap_in_envelope_body!(header, msg)?.to_encoded_bytes()?;
        let encrypted_bytes = crypt::encrypt(&self.shared_secret(dest_public_key), &body_bytes)?;

        Envelope::construct_signed(
            self.node_identity.secret_key(),
            self.node_identity.public_key(),
            encrypted_bytes,
            MessageFlags::ENCRYPTED,
        )
        .map_err(ControlServiceError::MessageError)
    }

    fn shared_secret(&self, public_key: &CommsPublicKey) -> CommsPublicKey {
        crypt::generate_ecdh_secret(self.node_identity.secret_key(), public_key)
    }

    fn send_reply<T>(
        &self,
        dest_public_key: &CommsPublicKey,
        identity_frame: Frame,
        message_type: MessageType,
        msg: T,
    ) -> Result<()>
    where
        T: prost::Message,
    {
        let envelope = self.construct_envelope(dest_public_key, message_type, msg)?;
        let mut frames = vec![identity_frame];

        frames.push(envelope.to_encoded_bytes()?);

        self.listener.send(frames).map_err(ControlServiceError::ConnectionError)
    }

    fn decrypt_body(&self, body: &Vec<u8>, public_key: &CommsPublicKey) -> Result<Vec<u8>> {
        let ecdh_shared_secret = crypt::generate_ecdh_secret(self.node_identity.secret_key(), public_key);
        crypt::decrypt(&ecdh_shared_secret, &body).map_err(ControlServiceError::CipherError)
    }

    fn establish_listener(context: &ZmqContext, config: &ControlServiceConfig) -> Result<EstablishedConnection> {
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
