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
    services::{Service, ServiceContext, ServiceControlMessage, ServiceError},
    tari_message::{NetMessage, TariMessageType},
};
use crossbeam_channel as channel;
use derive_error::Error;
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    domain_connector::ConnectorError,
    message::{Message, MessageError, MessageFlags, MessageHeader},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy, OutboundError},
    types::CommsPublicKey,
    DomainConnector,
};
use tari_utilities::{
    hex::Hex,
    message_format::{MessageFormat, MessageFormatError},
};

const LOG_TARGET: &'static str = "base_layer::p2p::ping_pong";

#[derive(Debug, Error)]
pub enum PingPongError {
    OutboundError(OutboundError),
    /// OMS has not been initialized
    OMSNotInitialized,
    SerializationFailed(MessageFormatError),
    ReceiveError(ConnectorError),
    MessageError(MessageError),
    /// Failed to send from API
    ApiSendFailed,
    /// Failed to receive in API from service
    ApiReceiveFailed,
    /// Received an unexpected response type from the API
    UnexpectedApiResponse,
}

/// The PingPong message
#[derive(Serialize, Deserialize)]
pub enum PingPong {
    Ping,
    Pong,
}

/// Thin convenience wrapper for any service api
struct ApiWrapper<T, Req, Res> {
    api: Arc<T>,
    receiver: channel::Receiver<Req>,
    sender: channel::Sender<Res>,
}

impl<T, Req, Res> ApiWrapper<T, Req, Res> {
    /// Create a new service API
    pub fn new(receiver: channel::Receiver<Req>, sender: channel::Sender<Res>, api: Arc<T>) -> Self {
        Self { api, receiver, sender }
    }

    /// Send a reply to the calling API
    pub fn send_reply(&self, msg: Res) -> Result<(), channel::SendError<Res>> {
        self.sender.send(msg)
    }

    /// Attempt to receive a service API message
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<Req>, channel::RecvTimeoutError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(msg) => Ok(Some(msg)),
            Err(channel::RecvTimeoutError::Timeout) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Return the API
    fn get_api(&self) -> Arc<T> {
        self.api.clone()
    }
}

pub struct PingPongService {
    // Needed because the public ping method needs OMS
    oms: Option<Arc<OutboundMessageService>>,
    ping_count: usize,
    pong_count: usize,
    api: ApiWrapper<PingPongServiceApi, PingPongApiRequest, PingPongApiResult>,
}

impl PingPongService {
    /// Create a new ping pong service
    pub fn new() -> Self {
        Self {
            oms: None,
            ping_count: 0,
            pong_count: 0,
            api: Self::setup_api(),
        }
    }

    /// Return this services API
    pub fn get_api(&self) -> Arc<PingPongServiceApi> {
        self.api.get_api()
    }

    fn setup_api() -> ApiWrapper<PingPongServiceApi, PingPongApiRequest, PingPongApiResult> {
        let (api_sender, service_receiver) = channel::bounded(0);
        let (service_sender, api_receiver) = channel::bounded(0);

        let api = Arc::new(PingPongServiceApi::new(api_sender, api_receiver));
        ApiWrapper::new(service_receiver, service_sender, api)
    }

    fn send_msg(&self, broadcast_strategy: BroadcastStrategy, msg: PingPong) -> Result<(), PingPongError> {
        let oms = self.oms.clone().ok_or(PingPongError::OMSNotInitialized)?;

        let msg = Message::from_message_format(
            MessageHeader {
                message_type: TariMessageType::new(NetMessage::PingPong),
            },
            msg,
        )
        .map_err(PingPongError::MessageError)?;

        oms.send(
            broadcast_strategy,
            MessageFlags::empty(),
            msg.to_binary().map_err(PingPongError::SerializationFailed)?,
        )
        .map_err(PingPongError::OutboundError)
    }

    fn receive_ping(&mut self, connector: &DomainConnector<'static>) -> Result<(), PingPongError> {
        if let Some((info, msg)) = connector
            .receive_timeout(Duration::from_millis(500))
            .map_err(PingPongError::ReceiveError)?
        {
            match msg {
                PingPong::Ping => {
                    debug!(
                        target: LOG_TARGET,
                        "Received ping from {}",
                        info.source_identity.public_key.to_hex(),
                    );

                    self.ping_count += 1;

                    // Reply with Pong
                    self.send_msg(
                        BroadcastStrategy::DirectNodeId(info.source_identity.node_id.clone()),
                        PingPong::Pong,
                    )?;
                },
                PingPong::Pong => {
                    debug!(
                        target: LOG_TARGET,
                        "Received pong from {}",
                        info.source_identity.public_key.to_hex()
                    );

                    self.pong_count += 1;
                },
            }
        }

        Ok(())
    }

    fn ping(&self, pub_key: CommsPublicKey) -> Result<(), PingPongError> {
        self.send_msg(BroadcastStrategy::DirectPublicKey(pub_key), PingPong::Ping)
    }

    fn handle_api_message(&self, msg: PingPongApiRequest) -> Result<(), ServiceError> {
        debug!(
            target: LOG_TARGET,
            "[{}] Received API message: {:?}",
            self.get_name(),
            msg
        );
        let resp = match msg {
            PingPongApiRequest::Ping(pk) => self.ping(pk).map(|_| PingPongApiResponse::PingSent),
            PingPongApiRequest::GetPingCount => Ok(PingPongApiResponse::Count(self.ping_count)),
            PingPongApiRequest::GetPongCount => Ok(PingPongApiResponse::Count(self.pong_count)),
        };

        debug!(target: LOG_TARGET, "[{}] Replying to API: {:?}", self.get_name(), resp);
        self.api
            .send_reply(resp)
            .map_err(ServiceError::internal_service_error())
    }
}

impl Service for PingPongService {
    fn get_name(&self) -> String {
        "ping-pong".to_string()
    }

    fn get_message_types(&self) -> Vec<TariMessageType> {
        vec![NetMessage::PingPong.into()]
    }

    fn execute(&mut self, context: ServiceContext) -> Result<(), ServiceError> {
        let connector = context.create_connector(&NetMessage::PingPong.into()).map_err(|err| {
            ServiceError::ServiceInitializationFailed(format!("Failed to create connector for service: {}", err))
        })?;

        self.oms = Some(context.get_outbound_message_service());

        loop {
            if let Some(msg) = context.get_control_message(Duration::from_millis(5)) {
                match msg {
                    ServiceControlMessage::Shutdown => break,
                }
            }

            match self.receive_ping(&connector) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "PingPong service had error: {}", err);
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
pub enum PingPongApiRequest {
    /// Send a ping to the given public key
    Ping(CommsPublicKey),
    /// Retrieve the total number of pings received
    GetPingCount,
    /// Retrieve the total number of pongs received
    GetPongCount,
}

/// API Response enum
#[derive(Debug)]
pub enum PingPongApiResponse {
    PingSent,
    Count(usize),
}

impl fmt::Display for PingPongApiResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PingPongApiResponse::PingSent => write!(f, "PingSent"),
            PingPongApiResponse::Count(n) => write!(f, "Count({})", n),
        }
    }
}

/// Result for all API requests
pub type PingPongApiResult = Result<PingPongApiResponse, PingPongError>;

/// Default duration that a API 'client' will wait for a response from the service before returning a timeout error
const DEFAULT_API_TIMEOUT_MS: u64 = 200;

/// The PingPong service public api
pub struct PingPongServiceApi {
    sender: channel::Sender<PingPongApiRequest>,
    receiver: channel::Receiver<PingPongApiResult>,
    mutex: Mutex<()>,
    timeout: Duration,
}

impl PingPongServiceApi {
    fn new(sender: channel::Sender<PingPongApiRequest>, receiver: channel::Receiver<PingPongApiResult>) -> Self {
        Self {
            sender,
            receiver,
            mutex: Mutex::new(()),
            timeout: Duration::from_millis(DEFAULT_API_TIMEOUT_MS),
        }
    }

    /// Send a ping message to the given peer
    pub fn ping(&self, public_key: CommsPublicKey) -> Result<(), PingPongError> {
        self.send_recv(PingPongApiRequest::Ping(public_key))
            .and_then(|resp| match resp {
                PingPongApiResponse::PingSent => Ok(()),
                _ => Err(PingPongError::UnexpectedApiResponse),
            })
    }

    /// Fetch the ping count from the service
    pub fn ping_count(&self) -> Result<usize, PingPongError> {
        self.send_recv(PingPongApiRequest::GetPingCount)
            .and_then(|resp| match resp {
                PingPongApiResponse::Count(n) => Ok(n),
                _ => Err(PingPongError::UnexpectedApiResponse),
            })
    }

    /// Fetch the pong count from the service
    pub fn pong_count(&self) -> Result<usize, PingPongError> {
        self.send_recv(PingPongApiRequest::GetPongCount)
            .and_then(|resp| match resp {
                PingPongApiResponse::Count(n) => Ok(n),
                _ => Err(PingPongError::UnexpectedApiResponse),
            })
    }

    fn send_recv(&self, msg: PingPongApiRequest) -> PingPongApiResult {
        self.lock(|| -> PingPongApiResult {
            self.sender.send(msg).map_err(|_| PingPongError::ApiSendFailed)?;
            self.receiver
                .recv_timeout(self.timeout.clone())
                .map_err(|_| PingPongError::ApiReceiveFailed)?
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new() {
        let service = PingPongService::new();
        assert_eq!(service.get_name(), "ping-pong");
        assert_eq!(service.get_message_types(), vec![NetMessage::PingPong.into()]);
        assert_eq!(service.ping_count, 0);
        assert_eq!(service.pong_count, 0);
    }
}
