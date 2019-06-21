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
use derive_error::Error;
use log::*;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tari_comms::{
    domain_connector::ConnectorError,
    message::{Message, MessageError, MessageFlags, MessageHeader},
    outbound_message_service::{outbound_message_service::OutboundMessageService, BroadcastStrategy, OutboundError},
    types::CommsPublicKey,
    DomainConnector,
};
use tari_utilities::message_format::{MessageFormat, MessageFormatError};

const LOG_TARGET: &'static str = "base_layer::p2p::ping_pong";

#[derive(Debug, Error)]
pub enum PingPongError {
    OutboundError(OutboundError),
    /// OMS has not been initialized
    OMSNotInitialized,
    SerializationFailed(MessageFormatError),
    ReceiveError(ConnectorError),
    MessageError(MessageError),
}

/// The PingPong message
#[derive(Serialize, Deserialize)]
pub enum PingPong {
    Ping,
    Pong,
}

/// Periodically
pub struct PingPongService {
    // Needed because the public ping method needs OMS
    oms: Option<Arc<OutboundMessageService>>,
}

impl PingPongService {
    pub fn new() -> Self {
        Self { oms: None }
    }

    pub fn ping(&self, pub_key: CommsPublicKey) -> Result<(), PingPongError> {
        self.send_msg(BroadcastStrategy::DirectPublicKey(pub_key), PingPong::Ping)
    }

    fn send_msg(
        &self,
        broadcast_strategy: BroadcastStrategy<CommsPublicKey>,
        msg: PingPong,
    ) -> Result<(), PingPongError>
    {
        let oms = self.oms.clone().ok_or(PingPongError::OMSNotInitialized)?;

        let msg = Message::from_message_format(
            MessageHeader {
                message_type: NetMessage::PingPong,
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

    fn run(&self, connector: &DomainConnector<'static>) -> Result<(), PingPongError> {
        if let Some((info, msg)) = connector
            .receive_timeout(Duration::from_millis(500))
            .map_err(PingPongError::ReceiveError)?
        {
            match msg {
                PingPong::Ping => {
                    debug!(
                        target: LOG_TARGET,
                        "Received ping from Public Key {:?}", info.source_identity.public_key
                    );
                    // Reply with Pong
                    self.send_msg(
                        BroadcastStrategy::DirectNodeId(info.source_identity.node_id.clone()),
                        PingPong::Pong,
                    )?;
                },
                PingPong::Pong => {
                    debug!(
                        target: LOG_TARGET,
                        "Received pong from Public Key {:?}", info.source_identity.public_key
                    );
                },
            }
        }

        Ok(())
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

            match self.run(&connector) {
                Ok(_) => {},
                Err(err) => {
                    error!(target: LOG_TARGET, "PingPong service had error: {}", err);
                },
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_name() {
        let service = PingPongService::new();
        assert_eq!(service.get_name(), "ping-pong");
    }

    #[test]
    fn get_message_types() {
        let service = PingPongService::new();
        assert_eq!(service.get_message_types(), vec![NetMessage::PingPong.into()]);
    }
}
