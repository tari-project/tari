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

use crate::{
    consts::{
        DHT_BROADCAST_NODE_COUNT,
        SAF_HIGH_PRIORITY_MSG_STORAGE_TTL,
        SAF_LOW_PRIORITY_MSG_STORAGE_TTL,
        SAF_MSG_CACHE_STORAGE_CAPACITY,
    },
    saf_service::{RetrieveMsgsMessage, SAFError, StoredMsgsMessage},
    sync_services::{
        Service,
        ServiceApiWrapper,
        ServiceContext,
        ServiceControlMessage,
        ServiceError,
        DEFAULT_API_TIMEOUT_MS,
    },
    tari_message::{NetMessage, TariMessageType},
};
use chrono::prelude::*;
use crossbeam_channel as channel;
use log::*;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    connection::{Connection, Direction, InprocAddress, SocketEstablishment, ZmqContext},
    domain_connector::MessageInfo,
    message::{Frame, MessageData, MessageEnvelope, MessageEnvelopeHeader, MessageFlags, NodeDestination},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy, ClosestRequest},
    peer_manager::{NodeIdentity, PeerManager},
    DomainConnector,
};
use ttl_cache::TtlCache;

const LOG_TARGET: &str = "base_layer::p2p::saf";

/// Storage for a single message envelope, including the date and time when the element was stored
pub struct StoredMessage {
    store_time: DateTime<Utc>,
    message_envelope: MessageEnvelope,
    message_envelope_header: MessageEnvelopeHeader,
}

impl StoredMessage {
    /// Create a new StorageMessage from a MessageEnvelope
    pub fn from(message_envelope: MessageEnvelope, message_envelope_header: MessageEnvelopeHeader) -> Self {
        Self {
            store_time: Utc::now(),
            message_envelope,
            message_envelope_header,
        }
    }
}

/// The Store-and-forward Service manages the storage of forwarded message and provides an api for neighbouring peers to
/// retrieve the stored messages.
pub struct SAFService {
    node_identity: Option<Arc<NodeIdentity>>,
    oms: Option<Arc<OutboundMessageService>>,
    peer_manager: Option<Arc<PeerManager>>,
    zmq_context: Option<ZmqContext>,
    ims_message_sink_address: Option<InprocAddress>,
    api: ServiceApiWrapper<SAFServiceApi, SAFApiRequest, SAFApiResult>,
    msg_storage: TtlCache<Frame, StoredMessage>,
}

impl SAFService {
    /// Create a new Store-and-forward service.
    pub fn new() -> Self {
        Self {
            node_identity: None,
            oms: None,
            peer_manager: None,
            zmq_context: None,
            ims_message_sink_address: None,
            api: Self::setup_api(),
            msg_storage: TtlCache::new(SAF_MSG_CACHE_STORAGE_CAPACITY),
        }
    }

    /// Return this services API.
    pub fn get_api(&self) -> Arc<SAFServiceApi> {
        self.api.get_api()
    }

    fn setup_api() -> ServiceApiWrapper<SAFServiceApi, SAFApiRequest, SAFApiResult> {
        let (api_sender, service_receiver) = channel::bounded(0);
        let (service_sender, api_receiver) = channel::bounded(0);

        let api = Arc::new(SAFServiceApi::new(api_sender, api_receiver));
        ServiceApiWrapper::new(service_receiver, service_sender, api)
    }

    /// Send a message retrieval request to all neighbouring peers that are in the same network region.
    fn send_retrieval_request(&self, start_time: Option<DateTime<Utc>>) -> Result<(), SAFError> {
        let oms = self.oms.as_ref().ok_or(SAFError::OMSUndefined)?;
        let node_identity = self.node_identity.as_ref().ok_or(SAFError::NodeIdentityUndefined)?;

        oms.send_message(
            BroadcastStrategy::Closest(ClosestRequest {
                n: DHT_BROADCAST_NODE_COUNT,
                node_id: node_identity.identity.node_id.clone(),
                excluded_peers: Vec::new(),
            }),
            MessageFlags::ENCRYPTED,
            RetrieveMsgsMessage { start_time },
        )?;
        trace!(target: LOG_TARGET, "Message retrieval request sent");

        Ok(())
    }

    /// Process an incoming message retrieval request.
    fn receive_retrieval_request(&mut self, connector: &DomainConnector<'static>) -> Result<(), SAFError> {
        let oms = self.oms.as_ref().ok_or(SAFError::OMSUndefined)?;
        let peer_manager = self.peer_manager.as_ref().ok_or(SAFError::PeerManagerUndefined)?;
        let node_identity = self.node_identity.as_ref().ok_or(SAFError::NodeIdentityUndefined)?;

        let incoming_msg: Option<(MessageInfo, RetrieveMsgsMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(SAFError::ConnectorError)?;
        if let Some((info, retrieval_request_msg)) = incoming_msg {
            if peer_manager.in_network_region(
                &info.peer_source.node_id,
                &node_identity.identity.node_id,
                DHT_BROADCAST_NODE_COUNT,
            )? {
                // Compile a set of stored messages for the requesting peer
                // TODO: compiling the bundle of messages is slow, especially when there are many stored messages, a
                // better approach should be used
                let mut stored_msgs_response = StoredMsgsMessage {
                    message_envelopes: Vec::new(),
                };
                for (_, stored_message) in self.msg_storage.iter() {
                    if retrieval_request_msg
                        .start_time
                        .map(|start_time| start_time <= stored_message.store_time)
                        .unwrap_or(true)
                    {
                        match stored_message.message_envelope_header.dest.clone() {
                            NodeDestination::Unknown => {
                                stored_msgs_response
                                    .message_envelopes
                                    .push(stored_message.message_envelope.clone());
                            },
                            NodeDestination::PublicKey(dest_public_key) => {
                                if dest_public_key == info.peer_source.public_key {
                                    stored_msgs_response
                                        .message_envelopes
                                        .push(stored_message.message_envelope.clone());
                                }
                            },
                            NodeDestination::NodeId(dest_node_id) => {
                                if dest_node_id == info.peer_source.node_id {
                                    stored_msgs_response
                                        .message_envelopes
                                        .push(stored_message.message_envelope.clone());
                                }
                            },
                        };
                    }
                }

                oms.send_message(
                    BroadcastStrategy::DirectPublicKey(info.peer_source.public_key),
                    MessageFlags::ENCRYPTED,
                    stored_msgs_response,
                )?;
                trace!(target: LOG_TARGET, "Responded to received message retrieval request");
            }
        }

        Ok(())
    }

    /// Process an incoming set of retrieved messages.
    fn receive_stored_messages(&mut self, connector: &DomainConnector<'static>) -> Result<(), SAFError> {
        let ims_message_sink_address = self
            .ims_message_sink_address
            .as_ref()
            .ok_or(SAFError::IMSMessageSinkAddressUndefined)?;
        let zmq_context = self.zmq_context.as_ref().ok_or(SAFError::ZMQContextUndefined)?;

        let incoming_msg: Option<(MessageInfo, StoredMsgsMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(SAFError::ConnectorError)?;
        if let Some((info, stored_msgs)) = incoming_msg {
            // Send each received MessageEnvelope to the InboundMessageService
            let ims_connection = Connection::new(&zmq_context, Direction::Outbound)
                .set_socket_establishment(SocketEstablishment::Connect)
                .establish(&ims_message_sink_address)?;
            for message_envelope in stored_msgs.message_envelopes {
                let message_data = MessageData::new(info.peer_source.node_id.clone(), false, message_envelope);
                let message_data_frame_set = message_data.into_frame_set();
                ims_connection.send(message_data_frame_set.clone())?;
            }
            trace!(target: LOG_TARGET, "Received stored messages from neighbouring peer");
        }

        Ok(())
    }

    /// Store messages of known neighbouring peers, this network region and messages with undefined destinations.
    /// Undefined destinations have a lower priority TTL.
    fn store_message(&mut self, message_envelope: MessageEnvelope) -> Result<(), SAFError> {
        let peer_manager = self.peer_manager.as_ref().ok_or(SAFError::PeerManagerUndefined)?;
        let node_identity = self.node_identity.as_ref().ok_or(SAFError::NodeIdentityUndefined)?;

        let message_envelope_header = message_envelope.deserialize_header()?;
        match message_envelope_header.dest.clone() {
            NodeDestination::Unknown => {
                self.msg_storage.insert(
                    message_envelope.body_frame().clone(),
                    StoredMessage::from(message_envelope, message_envelope_header),
                    SAF_LOW_PRIORITY_MSG_STORAGE_TTL,
                );
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if peer_manager.exists(&dest_public_key)? {
                    self.msg_storage.insert(
                        message_envelope.body_frame().clone(),
                        StoredMessage::from(message_envelope, message_envelope_header),
                        SAF_HIGH_PRIORITY_MSG_STORAGE_TTL,
                    );
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                if (peer_manager.exists_node_id(&dest_node_id)?) |
                    (peer_manager.in_network_region(
                        &dest_node_id,
                        &node_identity.identity.node_id,
                        DHT_BROADCAST_NODE_COUNT,
                    )?)
                {
                    self.msg_storage.insert(
                        message_envelope.body_frame().clone(),
                        StoredMessage::from(message_envelope, message_envelope_header),
                        SAF_HIGH_PRIORITY_MSG_STORAGE_TTL,
                    );
                }
            },
        };

        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(&mut self, msg: SAFApiRequest) -> Result<(), ServiceError> {
        trace!(
            target: LOG_TARGET,
            "[{}] Received API message: {:?}",
            self.get_name(),
            msg
        );
        let resp = match msg {
            SAFApiRequest::SendRetrievalRequest(start_time) => self
                .send_retrieval_request(start_time)
                .map(|_| SAFApiResponse::RetrievalRequestSent),
            SAFApiRequest::StoreMessage(message_envelope) => self
                .store_message(message_envelope)
                .map(|_| SAFApiResponse::MessageStored),
        };

        trace!(target: LOG_TARGET, "[{}] Replying to API: {:?}", self.get_name(), resp);
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }
}

/// The Domain Service trait implementation for the Store-and-forward Service
impl Service for SAFService {
    fn get_name(&self) -> String {
        "store-and-forward".to_string()
    }

    fn get_message_types(&self) -> Vec<TariMessageType> {
        vec![NetMessage::RetrieveMessages.into(), NetMessage::StoredMessages.into()]
    }

    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
        let connector_retrieve_messages =
            context
                .create_connector(&NetMessage::RetrieveMessages.into())
                .map_err(|err| {
                    ServiceError::ServiceInitializationFailed(format!(
                        "Failed to create connector for service: {}",
                        err
                    ))
                })?;

        let connector_stored_messages =
            context
                .create_connector(&NetMessage::StoredMessages.into())
                .map_err(|err| {
                    ServiceError::ServiceInitializationFailed(format!(
                        "Failed to create connector for service: {}",
                        err
                    ))
                })?;

        self.node_identity = Some(context.node_identity());
        self.oms = Some(context.outbound_message_service());
        self.peer_manager = Some(context.peer_manager());
        self.zmq_context = Some(context.zmq_context().clone());
        self.ims_message_sink_address = Some(context.ims_message_sink_address().clone());

        debug!(target: LOG_TARGET, "Starting Store-and-forward Service executor");
        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            match self.receive_retrieval_request(&connector_retrieve_messages) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Store-and-forward service had error: {:?}", err);
                },
            }

            match self.receive_stored_messages(&connector_stored_messages) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "Store-and-forward service had error: {:?}", err);
                },
            }

            if let Some(msg) = self
                .api
                .recv_timeout(Duration::from_millis(5))
                .map_err(ServiceError::internal_service_error())?
            {
                self.handle_api_message(msg)?;
            }
        }

        Ok(())
    }
}

/// API Request enum
#[derive(Debug)]
pub enum SAFApiRequest {
    /// Send a request to retrieve stored messages from neighbouring peers
    SendRetrievalRequest(Option<DateTime<Utc>>),
    /// Store message of known neighbouring peers and forwarded messages
    StoreMessage(MessageEnvelope),
}

/// API Response enum
#[derive(Debug)]
pub enum SAFApiResponse {
    RetrievalRequestSent,
    MessageStored,
}

/// Result for all API requests
pub type SAFApiResult = Result<SAFApiResponse, SAFError>;

/// The Store-and-forward service public API that other services and application will use to interact with this service.
/// The requests and responses are transmitted via channels into the Service Executor thread where this service is
/// running
pub struct SAFServiceApi {
    sender: channel::Sender<SAFApiRequest>,
    receiver: channel::Receiver<SAFApiResult>,
    mutex: Mutex<()>,
    timeout: Duration,
}

impl SAFServiceApi {
    fn new(sender: channel::Sender<SAFApiRequest>, receiver: channel::Receiver<SAFApiResult>) -> Self {
        Self {
            sender,
            receiver,
            mutex: Mutex::new(()),
            timeout: Duration::from_millis(DEFAULT_API_TIMEOUT_MS),
        }
    }

    pub fn retrieve(&self, start_time: Option<DateTime<Utc>>) -> Result<(), SAFError> {
        self.send_recv(SAFApiRequest::SendRetrievalRequest(start_time))
            .and_then(|resp| match resp {
                SAFApiResponse::RetrievalRequestSent => Ok(()),
                _ => Err(SAFError::UnexpectedApiResponse),
            })
    }

    pub fn store(&self, message_envelope: MessageEnvelope) -> Result<(), SAFError> {
        self.send_recv(SAFApiRequest::StoreMessage(message_envelope))
            .and_then(|resp| match resp {
                SAFApiResponse::MessageStored => Ok(()),
                _ => Err(SAFError::UnexpectedApiResponse),
            })
    }

    fn send_recv(&self, msg: SAFApiRequest) -> SAFApiResult {
        self.lock(|| -> SAFApiResult {
            self.sender.send(msg).map_err(|_| SAFError::ApiSendFailed)?;
            self.receiver
                .recv_timeout(self.timeout)
                .map_err(|_| SAFError::ApiReceiveFailed)?
        })
    }

    fn lock<F, T>(&self, func: F) -> T
    where F: FnOnce() -> T {
        let lock = acquire_lock!(self.mutex);
        let res = func();
        drop(lock);
        res
    }
}
