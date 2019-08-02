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
    saf_service::{RetrieveMsgsMessage, SAFError, StoredMsgsMessage},
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
    domain_connector::MessageInfo,
    message::MessageEnvelope,
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::PeerManager,
    DomainConnector,
};

const LOG_TARGET: &str = "base_layer::p2p::saf";

/// The Store-and-forward Service manages the storage of forwarded message and provides an api for neighbouring peers to
/// retrieve the stored messages.
pub struct SAFService {
    oms: Option<Arc<OutboundMessageService>>,
    peer_manager: Option<Arc<PeerManager>>,
    api: ServiceApiWrapper<SAFServiceApi, SAFApiRequest, SAFApiResult>,
}

impl SAFService {
    /// Create a new Store-and-forward service.
    pub fn new() -> Self {
        Self {
            oms: None,
            peer_manager: None,
            api: Self::setup_api(),
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
    fn send_retrieval_request(&self) -> Result<(), SAFError> {
        let _oms = self.oms.as_ref().ok_or(SAFError::OMSUndefined)?;

        // TODO: construct and send a message retrieval request to all neighbouring peers using oms

        Ok(())
    }

    /// Process an incoming message retrieval request.
    fn receive_retrieval_request(&mut self, connector: &DomainConnector<'static>) -> Result<(), SAFError> {
        let _oms = self.oms.as_ref().ok_or(SAFError::OMSUndefined)?;

        let incoming_msg: Option<(MessageInfo, StoredMsgsMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(SAFError::ConnectorError)?;
        if let Some((_info, _stored_msgs)) = incoming_msg {

            // TODO: check that the request came from a peer that is in a similar region of the network
            // TODO: construct a response message with all the messages that are applicable to that peer
        }

        Ok(())
    }

    /// Process an incoming set of retrieved messages.
    fn receive_stored_messages(&mut self, connector: &DomainConnector<'static>) -> Result<(), SAFError> {
        let _oms = self.oms.as_ref().ok_or(SAFError::OMSUndefined)?;

        let incoming_msg: Option<(MessageInfo, RetrieveMsgsMessage)> = connector
            .receive_timeout(Duration::from_millis(1))
            .map_err(SAFError::ConnectorError)?;
        if let Some((_info, _retrieve_msg)) = incoming_msg {

            // TODO: submit each message in the message set to the InboundMessageService to get handled and forwarded to
            // the correct service. Duplicate retrieved messages will be discarded by the MessageCache of
            // the comms system
        }

        Ok(())
    }

    /// Store message of known neighbouring peers and forwarded messages
    fn store_message(&self, _message_envelope: MessageEnvelope) -> Result<(), SAFError> {
        // TODO store a single copy of the message when:
        //   (a) it was a forwarded messages or
        //   (b) message is for current network region or
        //   (c) the message is for a known neighbouring peer
        // Old messages should be removed to make space for new messages

        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    fn handle_api_message(&self, msg: SAFApiRequest) -> Result<(), ServiceError> {
        trace!(
            target: LOG_TARGET,
            "[{}] Received API message: {:?}",
            self.get_name(),
            msg
        );
        let resp = match msg {
            SAFApiRequest::SendRetrievalRequest => self
                .send_retrieval_request()
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

        self.oms = Some(context.outbound_message_service());
        self.peer_manager = Some(context.peer_manager());
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
    SendRetrievalRequest,
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

    pub fn retrieve(&self) -> Result<(), SAFError> {
        self.send_recv(SAFApiRequest::SendRetrievalRequest)
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
