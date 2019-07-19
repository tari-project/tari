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
    dht_service::DHTError,
    services::{
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
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::PeerManager,
    types::CommsPublicKey,
    DomainConnector,
};

const LOG_TARGET: &str = "base_layer::p2p::dht";

/// The DHTService manages joining the network and discovery of peers.
pub struct DHTService {
    oms: Option<Arc<OutboundMessageService>>,
    peer_manager: Option<Arc<PeerManager>>,
    api: ServiceApiWrapper<DHTServiceApi, DHTApiRequest, DHTApiResult>,
}

impl DHTService {
    /// Create a new DHT service
    pub fn new() -> Self {
        Self {
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

    /// Send a new network join request so that other peers are able to find this node on the network
    fn send_join(&self) -> Result<(), DHTError> {
        let _oms = self.oms.clone().ok_or(DHTError::OMSUndefined)?;

        // TODO: Construct join message and send to closest peers using OMS

        Ok(())
    }

    /// Send a discover request to find a specific peer on the network
    fn send_discover(&self, _public_key: CommsPublicKey) -> Result<(), DHTError> {
        let _oms = self.oms.clone().ok_or(DHTError::OMSUndefined)?;

        // TODO: Construct discover message and send to closest peers using OMS

        Ok(())
    }

    /// Process an incoming join request
    fn receive_join(&mut self, _connector: &DomainConnector<'static>) -> Result<(), DHTError> {
        let _oms = self.oms.clone().ok_or(DHTError::OMSUndefined)?;

        // TODO: receive join request from another peer
        // - Check information and add to peer manager
        // - If part of k nearest peers then send private join request back
        // - Propagate to closer peers

        Ok(())
    }

    /// Process an incoming discover request
    fn receive_discover(&mut self, _connector: &DomainConnector<'static>) -> Result<(), DHTError> {
        let _oms = self.oms.clone().ok_or(DHTError::OMSUndefined)?;

        // TODO: receive discovery request from another peer
        // - Check information and add/update peer in PeerManager
        // - Send join back

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
            DHTApiRequest::SendDiscover(public_key) => {
                self.send_discover(public_key).map(|_| DHTApiResponse::DiscoverSent)
            },
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
        vec![NetMessage::Join.into(), NetMessage::Discover.into()] // TODO: Where is / What is forward?
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
    SendDiscover(CommsPublicKey),
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

    pub fn send_discover(&self, public_key: CommsPublicKey) -> Result<(), DHTError> {
        self.send_recv(DHTApiRequest::SendDiscover(public_key))
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
