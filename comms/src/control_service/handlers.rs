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

use tari_utilities::message_format::MessageFormat;

use super::{error::ControlServiceError, messages::EstablishConnection, types::ControlServiceMessageContext};

#[allow(dead_code)]
const LOG_TARGET: &'static str = "comms::control_service::handlers";

/// Establish connection handler. This is the default handler which can be used to handle
/// the EstablishConnection message.
/// This handler:
/// - Will check if the connecting peer/public key should be allowed to connect
/// - Will open an outbound [PeerConnection] to that peer (using [ConnectionManager])
/// - If that connection is successful, add the peer to the routing table (using [PeerManager])
/// - Send an Accept message over the new [PeerConnection]
#[allow(dead_code)]
pub fn establish_connection(context: ControlServiceMessageContext) -> Result<(), ControlServiceError> {
    let message = EstablishConnection::from_binary(context.message.body.as_slice())
        .map_err(|e| ControlServiceError::MessageFormatError(e))?;

    debug!(target: LOG_TARGET, "EstablishConnection message: {:#?}", message);

    // TODO:
    // - Add peer to routing table
    // - Open a port with connection manager
    // - Send Accept message

    Ok(())
}

/// Discards (does nothing) with the given message.
pub fn discard(_: ControlServiceMessageContext) -> Result<(), ControlServiceError> {
    debug!(target: LOG_TARGET, "Discarding message");
    Ok(())
}
