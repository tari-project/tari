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

use diesel::result::{ConnectionError as DieselConnectionError, Error as DieselError};
use tari_comms::{builder::CommsError, connection::NetAddressError, message::MessageError};
use tari_comms_dht::outbound::DhtOutboundError;
use tari_crypto::tari_utilities::{hex::HexError, message_format::MessageFormatError};
use tari_p2p::services::liveness::error::LivenessError;
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;
use tokio_executor::threadpool::BlockingError;

#[derive(Debug, Error)]
pub enum TextMessageError {
    #[error("Message format error: `{0}`")]
    MessageFormatError(#[from] MessageFormatError),
    #[error("Message error: `{0}`")]
    MessageError(#[from] MessageError),
    #[error("Outbound error: `{0}`")]
    OutboundError(#[from] DhtOutboundError),
    #[error("Comms services error: `{0}`")]
    CommsServicesError(#[from] CommsError),
    #[error("Hex error: `{0}`")]
    HexError(#[from] HexError),
    #[error("Database error: `{0}`")]
    DatabaseError(#[from] DieselError),
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Database migration error: `{0}`")]
    DatabaseMigrationError(String),
    #[error("Net address error: `{0}`")]
    NetAddressError(#[from] NetAddressError),
    #[error("Database connection error: `{0}`")]
    DatabaseConnectionError(#[from] DieselConnectionError),
    #[error("Blocking error: `{0}`")]
    BlockingError(#[from] BlockingError),
    #[error("R2d2 error")]
    R2d2Error,
    #[error("Liveness error: `{0}`")]
    LivenessError(#[from] LivenessError),
    #[error("An error has occurred reading or writing the event subscriber stream")]
    EventStreamError,
    #[error("If a received TextMessageAck doesn't matching any pending messages")]
    MessageNotFound,
    #[error("Failed to send from API")]
    ApiSendFailed,
    #[error("Failed to receive in API from service")]
    ApiReceiveFailed,
    #[error("The Outbound Message Service is not initialized")]
    OMSNotInitialized,
    #[error("The Comms service stack is not initialized")]
    CommsNotInitialized,
    #[error("Received an unexpected API response")]
    UnexpectedApiResponse,
    #[error("Contact not found")]
    ContactNotFound,
    #[error("Contact already exists")]
    ContactAlreadyExists,
    #[error("There was an error updating a row in the database")]
    DatabaseUpdateError,
    #[error("Error retrieving settings")]
    SettingsReadError,
}
