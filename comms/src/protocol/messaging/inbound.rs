// Copyright 2020, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    message::{Envelope, InboundMessage},
    peer_manager::Peer,
    protocol::messaging::error::InboundMessagingError,
};
use bytes::Bytes;
use log::*;
use prost::Message;
use std::{convert::TryInto, sync::Arc};

const LOG_TARGET: &str = "comms::protocol::messaging::inbound";

pub struct InboundMessaging;

impl InboundMessaging {
    /// Process a single received message from its raw serialized form i.e. a FrameSet
    pub async fn process_message(
        &self,
        source_peer: Arc<Peer>,
        msg: &mut Bytes,
    ) -> Result<InboundMessage, InboundMessagingError>
    {
        let envelope = Envelope::decode(msg)?;

        let public_key = envelope
            .get_public_key()
            .ok_or_else(|| InboundMessagingError::InvalidEnvelope)?;

        trace!(
            target: LOG_TARGET,
            "Received message envelope version {} from peer '{}'",
            envelope.version,
            source_peer.node_id.short_str()
        );

        if source_peer.public_key != public_key {
            return Err(InboundMessagingError::PeerPublicKeyMismatch);
        }

        if !envelope.verify_signature()? {
            return Err(InboundMessagingError::InvalidMessageSignature);
        }

        // -- Message is authenticated --
        let Envelope { header, body, .. } = envelope;
        let header = header.expect("already checked").try_into().expect("already checked");

        let inbound_message = InboundMessage::new(source_peer, header, body.into());

        Ok(inbound_message)
    }
}
