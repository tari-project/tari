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
    consts::DHT_BROADCAST_NODE_COUNT,
    dht_service::{
        dht_messages::{DiscoverMessage, JoinMessage},
        DHTError,
    },
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
use crossbeam_channel as channel;
use log::*;
use std::{
    convert::TryInto,
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    connection::NetAddress,
    domain_connector::MessageInfo,
    message::{Frame, Message, MessageEnvelope, MessageFlags, NodeDestination},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy, ClosestRequest},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags, PeerManager},
    types::CommsPublicKey,
    DomainConnector,
};
use tari_utilities::message_format::MessageFormat;

const LOG_TARGET: &str = "base_layer::p2p::dht";

/// The DHTService manages joining the network and discovery of peers.
pub struct DHTService {
    node_identity: Option<Arc<NodeIdentity>>,
    prev_control_service_address: Option<NetAddress>,
    oms: Option<Arc<OutboundMessageService>>,
    peer_manager: Option<Arc<PeerManager>>,
    api: ServiceApiWrapper<DHTServiceApi, DHTApiRequest, DHTApiResult>,
}

impl DHTService {
    /// Create a new DHT service
    pub fn new() -> Self {
        Self {
            node_identity: None,
            prev_control_service_address: None,
            oms: None,
            peer_manager: None,
            api: Self::setup_api(),
        }
    }

    /// Return this services API
    pub fn get_api(&self) -> Arc<DHTServiceApi> {
        self.api.get_api()
    }

    fn setup_api() -> ServiceApiWrapper<DHTServiceApi, DHTApiRequest, DHTApiResult> {
        let (api_sender, service_receiver) = channel::bounded(0);
        let (service_sender, api_receiver) = channel::bounded(0);

        let api = Arc::new(DHTServiceApi::new(api_sender, api_receiver));
        ServiceApiWrapper::new(service_receiver, service_sender, api)
    }

    /// Construct a new join message that contains the current nodes identity and net_addresses
    fn construct_join_msg(&self) -> Result<JoinMessage, DHTError> {
        let node_identity = self.node_identity.as_ref().ok_or(DHTError::NodeIdentityUndefined)?;
        Ok(JoinMessage {
            node_id: node_identity.identity.node_id.clone(),
            net_address: vec![node_identity.control_service_address()?],
        })
    }

    /// Construct a new discover message that contains the current nodes identity and net_addresses
    fn construct_discover_msg(&self) -> Result<DiscoverMessage, DHTError> {
        let node_identity = self.node_identity.as_ref().ok_or(DHTError::NodeIdentityUndefined)?;
        Ok(DiscoverMessage {
            node_id: node_identity.identity.node_id.clone(),
            net_address: vec![node_identity.control_service_address()?],
        })
    }

    /// Send a new network join request to the peers that are closest to the current nodes network location. The Join
    /// Request will allow other peers to be able to find this node on the network.
    fn send_join(&self) -> Result<(), DHTError> {
        let oms = self.oms.as_ref().ok_or(DHTError::OMSUndefined)?;
        let node_identity = self.node_identity.as_ref().ok_or(DHTError::NodeIdentityUndefined)?;

        oms.send_message(
            BroadcastStrategy::Closest(ClosestRequest {
                n: DHT_BROADCAST_NODE_COUNT,
                node_id: node_identity.identity.node_id.clone(),
                excluded_peers: Vec::new(),
            }),
            MessageFlags::NONE,
            self.construct_join_msg()?,
        )?;
        trace!(target: LOG_TARGET, "Join Request Sent");

        Ok(())
    }

    /// Send a network join update request directly to a specific known peer
    fn send_join_direct(&self, dest_public_key: CommsPublicKey) -> Result<(), DHTError> {
        let oms = self.oms.as_ref().ok_or(DHTError::OMSUndefined)?;

        oms.send_message(
            BroadcastStrategy::DirectPublicKey(dest_public_key),
            MessageFlags::ENCRYPTED,
            self.construct_join_msg()?,
        )?;
        trace!(target: LOG_TARGET, "Direct Join Request Sent");

        Ok(())
    }

    /// Send a discover request to find a specific peer on the network
    fn send_discover(
        &self,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        header_dest: NodeDestination<CommsPublicKey>,
    ) -> Result<(), DHTError>
    {
        let oms = self.oms.as_ref().ok_or(DHTError::OMSUndefined)?;
        let node_identity = self.node_identity.as_ref().ok_or(DHTError::NodeIdentityUndefined)?;

        let discover_msg: Message = self
            .construct_discover_msg()?
            .try_into()
            .map_err(DHTError::MessageSerializationError)?;
        let message_envelope_body: Frame = discover_msg.to_binary().map_err(DHTError::MessageFormatError)?;
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_public_key,
            header_dest.clone(),
            message_envelope_body.clone(),
            MessageFlags::ENCRYPTED,
        )
        .map_err(DHTError::MessageSerializationError)?;

        let broadcast_strategy = BroadcastStrategy::discover(
            node_identity.identity.node_id.clone(),
            dest_node_id,
            header_dest,
            Vec::new(),
        );
        oms.forward_message(broadcast_strategy, message_envelope)?;

        Ok(())
    }

    /// Process an incoming join request. The peer specified in the join request will be added to the PeerManager. If
    /// the current Node and the join request Node are from the same region of the network then the current node will
    /// send a join request back to that peer informing it that the current node is a neighbouring node. The join
    /// request is then forwarded to closer nodes.
    fn receive_join(&mut self, connector: &DomainConnector<'static>) -> Result<(), DHTError> {
        let oms = self.oms.as_ref().ok_or(DHTError::OMSUndefined)?;
        let peer_manager = self.peer_manager.as_ref().ok_or(DHTError::PeerManagerUndefined)?;
        let node_identity = self.node_identity.as_ref().ok_or(DHTError::NodeIdentityUndefined)?;

        let incoming_msg: Option<(MessageInfo, JoinMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(DHTError::ConnectorError)?;
        if let Some((info, join_msg)) = incoming_msg {
            // TODO: Check/Verify the received peers information

            // Add peer or modify existing peer using received join request
            if peer_manager.exists(&info.origin_source)? {
                peer_manager.update_peer(
                    &info.origin_source,
                    Some(join_msg.node_id.clone()),
                    Some(join_msg.net_address.clone()),
                    None,
                )?;
            } else {
                let peer = Peer::new(
                    info.origin_source.clone(),
                    join_msg.node_id.clone(),
                    join_msg.net_address.clone().into(),
                    PeerFlags::default(),
                );
                peer_manager.add_peer(peer)?;
            }

            // Send a join request back to the source peer of the join request if that peer is from the same region
            // of network. Also, only Send a join request back if this copy of the received join
            // request was not sent directly from the original source peer but was forwarded. If it
            // was not forwarded then that source peer already has the current peers info in its
            // PeerManager.
            if (info.origin_source != info.peer_source.public_key) &&
                (peer_manager.in_network_region(
                    &join_msg.node_id,
                    &node_identity.identity.node_id,
                    DHT_BROADCAST_NODE_COUNT,
                )?)
            {
                self.send_join_direct(info.origin_source.clone())?;
            }

            // Propagate message to closer peers
            //            oms.forward_message(
            //                BroadcastStrategy::Closest(ClosestRequest {
            //                    n: DHT_BROADCAST_NODE_COUNT,
            //                    node_id: join_msg.node_id.clone(),
            //                    excluded_peers: vec![info.origin_source, info.peer_source.public_key],
            //                }),
            //                info.message_envelope,
            //            )?;
        }

        Ok(())
    }

    /// Process an incoming discover request that was meant for the current node
    fn receive_discover(&mut self, connector: &DomainConnector<'static>) -> Result<(), DHTError> {
        let peer_manager = self.peer_manager.as_ref().ok_or(DHTError::PeerManagerUndefined)?;

        let incoming_msg: Option<(MessageInfo, DiscoverMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(DHTError::ConnectorError)?;
        if let Some((info, discover_msg)) = incoming_msg {
            // TODO: Check/Verify the received peers information

            // Add peer or modify existing peer using received discover request
            if peer_manager.exists(&info.origin_source)? {
                peer_manager.update_peer(
                    &info.origin_source,
                    Some(discover_msg.node_id.clone()),
                    Some(discover_msg.net_address.clone()),
                    None,
                )?;
            } else {
                let peer = Peer::new(
                    info.origin_source.clone(),
                    discover_msg.node_id.clone(),
                    discover_msg.net_address.clone().into(),
                    PeerFlags::default(),
                );
                peer_manager.add_peer(peer)?;
            }

            // Send the origin the current nodes latest contact info
            self.send_join_direct(info.origin_source)?;
        }

        Ok(())
    }

    /// The auto_join function sends a join request on startup or on the detection of a control_service_address change
    fn auto_join(&mut self) -> Result<(), DHTError> {
        let node_identity = self.node_identity.as_ref().ok_or(DHTError::NodeIdentityUndefined)?;

        if match self.prev_control_service_address.as_ref() {
            Some(control_service_address) => *control_service_address != node_identity.control_service_address()?, /* Identity change detected */
            None => true, // Startup detected
        } {
            self.prev_control_service_address = Some(node_identity.control_service_address()?);
            self.send_join()?;
        }

        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(&self, msg: DHTApiRequest) -> Result<(), ServiceError> {
        trace!(
            target: LOG_TARGET,
            "[{}] Received API message: {:?}",
            self.get_name(),
            msg
        );
        let resp = match msg {
            DHTApiRequest::SendJoin => self.send_join().map(|_| DHTApiResponse::JoinSent),
            DHTApiRequest::SendDiscover(dest_public_key, dest_node_id, header_dest) => self
                .send_discover(dest_public_key, dest_node_id, header_dest)
                .map(|_| DHTApiResponse::DiscoverSent),
        };

        trace!(target: LOG_TARGET, "[{}] Replying to API: {:?}", self.get_name(), resp);
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }
}

/// The Domain Service trait implementation for the DHTService
impl Service for DHTService {
    fn get_name(&self) -> String {
        "dht".to_string()
    }

    fn get_message_types(&self) -> Vec<TariMessageType> {
        vec![NetMessage::Join.into(), NetMessage::Discover.into()]
    }

    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
        let connector_join = context.create_connector(&NetMessage::Join.into()).map_err(|err| {
            ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
        })?;

        let connector_discover = context.create_connector(&NetMessage::Discover.into()).map_err(|err| {
            ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
        })?;

        self.oms = Some(context.outbound_message_service());
        self.peer_manager = Some(context.peer_manager());
        self.node_identity = Some(context.node_identity());
        debug!(target: LOG_TARGET, "Starting DHT Service executor");
        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            match self.receive_join(&connector_join) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "DHT service had error: {:?}", err);
                },
            }

            match self.receive_discover(&connector_discover) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "DHT service had error: {:?}", err);
                },
            }

            match self.auto_join() {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "DHT service had an auto join error: {:?}", err);
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
pub enum DHTApiRequest {
    /// Send a join request to neighbouring peers on the network
    SendJoin,
    /// Send a discovery request to find a selected peer
    SendDiscover(CommsPublicKey, Option<NodeId>, NodeDestination<CommsPublicKey>),
}

/// API Response enum
#[derive(Debug)]
pub enum DHTApiResponse {
    JoinSent,
    DiscoverSent,
}

/// Result for all API requests
pub type DHTApiResult = Result<DHTApiResponse, DHTError>;

/// The DHT service public API that other services and application will use to interact with this service.
/// The requests and responses are transmitted via channels into the Service Executor thread where this service is
/// running
pub struct DHTServiceApi {
    sender: channel::Sender<DHTApiRequest>,
    receiver: channel::Receiver<DHTApiResult>,
    mutex: Mutex<()>,
    timeout: Duration,
}

impl DHTServiceApi {
    fn new(sender: channel::Sender<DHTApiRequest>, receiver: channel::Receiver<DHTApiResult>) -> Self {
        Self {
            sender,
            receiver,
            mutex: Mutex::new(()),
            timeout: Duration::from_millis(DEFAULT_API_TIMEOUT_MS),
        }
    }

    pub fn send_join(&self) -> Result<(), DHTError> {
        self.send_recv(DHTApiRequest::SendJoin).and_then(|resp| match resp {
            DHTApiResponse::JoinSent => Ok(()),
            _ => Err(DHTError::UnexpectedApiResponse),
        })
    }

    pub fn send_discover(
        &self,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        header_dest: NodeDestination<CommsPublicKey>,
    ) -> Result<(), DHTError>
    {
        self.send_recv(DHTApiRequest::SendDiscover(dest_public_key, dest_node_id, header_dest))
            .and_then(|resp| match resp {
                DHTApiResponse::DiscoverSent => Ok(()),
                _ => Err(DHTError::UnexpectedApiResponse),
            })
    }

    fn send_recv(&self, msg: DHTApiRequest) -> DHTApiResult {
        self.lock(|| -> DHTApiResult {
            self.sender.send(msg).map_err(|_| DHTError::ApiSendFailed)?;
            self.receiver
                .recv_timeout(self.timeout)
                .map_err(|_| DHTError::ApiReceiveFailed)?
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
