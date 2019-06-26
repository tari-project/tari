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
    types::{ControlServiceMessageContext, ControlServiceMessageType},
};
use crate::{
    dispatcher::{DispatchError, DispatchResolver},
    message::{
        p2p::{Accept, EstablishConnection},
        Message,
        MessageEnvelope,
        MessageFlags,
        MessageHeader,
        NodeDestination,
    },
    peer_manager::{peer_manager::PeerManagerError, Peer, PeerFlags},
    types::CommsPublicKey,
};
use log::*;
use serde::{de::DeserializeOwned, export::PhantomData, Serialize};
use tari_utilities::message_format::MessageFormat;

#[allow(dead_code)]
const LOG_TARGET: &'static str = "comms::control_service::handlers";

#[derive(Default)]
pub struct ControlServiceResolver<MType>(PhantomData<MType>);

impl<MType> ControlServiceResolver<MType> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<MType> DispatchResolver<ControlServiceMessageType, ControlServiceMessageContext<MType>>
    for ControlServiceResolver<MType>
where MType: Clone
{
    fn resolve(&self, msg: &ControlServiceMessageContext<MType>) -> Result<ControlServiceMessageType, DispatchError> {
        let header: MessageHeader<ControlServiceMessageType> = msg
            .message
            .to_header()
            .map_err(|err| DispatchError::ResolveFailed(format!("{}", err)))?;

        Ok(header.message_type)
    }
}

/// Establish connection handler. This is the default handler which can be used to handle
/// the EstablishConnection message.
/// This handler:
/// - checks if the connecting peer/public key should be allowed to connect
/// - opens an outbound [PeerConnection] to that peer (using [ConnectionManager])
/// - If that connection is successful, add the peer to the routing table (using [PeerManager])
/// - Send an Accept message over the new [PeerConnection]
pub fn establish_connection<MType>(context: ControlServiceMessageContext<MType>) -> Result<(), ControlServiceError>
where
    MType: Serialize + DeserializeOwned,
    MType: Clone,
{
    let message = EstablishConnection::<CommsPublicKey>::from_binary(context.message.body.as_slice())
        .map_err(|e| ControlServiceError::MessageFormatError(e))?;

    debug!(
        target: LOG_TARGET,
        "EstablishConnection message received (node_id={}, address={})", message.node_id, message.address
    );

    let pm = &context.peer_manager;
    let public_key = message.public_key.clone();
    let node_id = message.node_id.clone();
    let peer = match pm.find_with_public_key(&public_key) {
        Ok(peer) => {
            if peer.is_banned() {
                return Err(ControlServiceError::PeerBanned);
            }

            // TODO(sdbondi): add_net_address should be idempotent, once it is, we can remove this check
            match pm.find_with_net_address(&message.control_service_address) {
                Ok(found_peer) => {
                    if found_peer == peer {
                        info!(
                            target: LOG_TARGET,
                            "Address {} already exists for node_id={}", message.address, found_peer.node_id
                        );
                    } else {
                        warn!(
                            target: LOG_TARGET,
                            "Address {} already used for another peer with node_id={}. Address will be replaced",
                            message.address,
                            found_peer.node_id
                        );
                    }
                },
                Err(PeerManagerError::PeerNotFoundError) => pm
                    .add_net_address(&node_id, &message.control_service_address)
                    .map_err(ControlServiceError::PeerManagerError)?,
                Err(err) => return Err(err.into()),
            }

            peer
        },
        Err(PeerManagerError::PeerNotFoundError) => {
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

    let conn_manager = &context.connection_manager.clone();

    debug!(
        target: LOG_TARGET,
        "Connecting to requested address {}", message.address
    );
    let conn = conn_manager
        .establish_requested_outbound_connection(&peer, message.address.clone(), message.server_key)
        .map_err(ControlServiceError::ConnectionManagerError)?;

    conn.wait_connected_or_failure(&context.config.requested_outbound_connection_timeout)
        .map_err(ControlServiceError::ConnectionError)?;
    debug!(
        target: LOG_TARGET,
        "Connection to requested address {} succeeded", message.address
    );

    let header = MessageHeader {
        message_type: context.config.accept_message_type,
    };
    let msg = Message::from_message_format(header, Accept {}).map_err(ControlServiceError::MessageError)?;

    let envelope = MessageEnvelope::construct(
        &context.node_identity,
        public_key.clone(),
        NodeDestination::PublicKey(public_key),
        msg.to_binary().map_err(ControlServiceError::MessageFormatError)?,
        MessageFlags::empty(),
    )
    .map_err(ControlServiceError::MessageError)?;

    debug!(
        target: LOG_TARGET,
        "Sending 'Accept' message to address {:?}",
        conn.get_connected_address()
    );
    conn.send(envelope.into_frame_set())
        .map_err(ControlServiceError::ConnectionError)?;

    Ok(())
}

/// Discard
pub fn discard<MType>(_: ControlServiceMessageContext<MType>) -> Result<(), ControlServiceError>
where MType: Clone {
    debug!(target: LOG_TARGET, "Message discarded");

    Ok(())
}
