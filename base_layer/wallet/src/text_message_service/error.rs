// Copyright 2019. The Tari Project
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

use derive_error::Error;
use diesel::result::{ConnectionError as DieselConnectionError, Error as DieselError};
use tari_comms::{
    builder::CommsServicesError,
    connection::NetAddressError,
    message::MessageError,
    outbound_message_service::OutboundServiceError,
};
use tari_p2p::sync_services::ServiceError;
use tari_utilities::{hex::HexError, message_format::MessageFormatError};

#[derive(Debug, Error)]
pub enum TextMessageError {
    MessageFormatError(MessageFormatError),
    MessageError(MessageError),
    OutboundError(OutboundServiceError),
    ServiceError(ServiceError),
    CommsServicesError(CommsServicesError),
    HexError(HexError),
    DatabaseError(DieselError),
    NetAddressError(NetAddressError),
    DatabaseConnectionError(DieselConnectionError),
    /// If a received TextMessageAck doesn't matching any pending messages
    MessageNotFound,
    /// Failed to send from API
    ApiSendFailed,
    /// Failed to receive in API from service
    ApiReceiveFailed,
    /// The Outbound Message Service is not initialized
    OMSNotInitialized,
    /// The Comms service stack is not initialized
    CommsNotInitialized,
    /// Received an unexpected API response
    UnexpectedApiResponse,
    /// Contact not found
    ContactNotFound,
    /// Contact already exists
    ContactAlreadyExists,
    /// There was an error updating a row in the database
    DatabaseUpdateError,
    /// Error retrieving settings
    SettingsReadError,
}
