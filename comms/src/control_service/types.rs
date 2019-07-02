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

use super::{error::ControlServiceError, handlers};
use crate::{
    connection_manager::ConnectionManager,
    control_service::{handlers::ControlServiceResolver, ControlServiceConfig},
    dispatcher::Dispatcher,
    message::{Message, MessageEnvelopeHeader},
    peer_manager::{NodeIdentity, PeerManager},
    types::CommsPublicKey,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::Arc;

/// Control Messgages for the control service worker
#[derive(Debug)]
pub enum ControlMessage {
    Shutdown,
}

/// ControlService result type
pub type Result<T> = std::result::Result<T, ControlServiceError>;

/// The [Dispatcher] required for ControlService.
pub type ControlServiceDispatcher<MType> = Dispatcher<
    ControlServiceMessageType,
    ControlServiceMessageContext<MType>,
    ControlServiceResolver<MType>,
    ControlServiceError,
>;

impl<MType> Default for ControlServiceDispatcher<MType>
where
    MType: Clone,
    MType: Serialize + DeserializeOwned,
{
    fn default() -> Self {
        ControlServiceDispatcher::new(ControlServiceResolver::new())
            .route(
                ControlServiceMessageType::EstablishConnection,
                handlers::establish_connection,
            )
            .catch_all(handlers::discard)
    }
}

/// The message required to use the default handlers.
/// This contains the serialized message and envelope header
pub struct ControlServiceMessageContext<MType>
where MType: Clone
{
    pub envelope_header: MessageEnvelopeHeader<CommsPublicKey>,
    pub message: Message,
    pub connection_manager: Arc<ConnectionManager>,
    pub peer_manager: Arc<PeerManager>,
    pub node_identity: Arc<NodeIdentity>,
    pub config: ControlServiceConfig<MType>,
}

/// Control service message types
#[derive(Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ControlServiceMessageType {
    EstablishConnection,
}
