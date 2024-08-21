// Copyright 2023. The Tari Project
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

use std::{convert::TryFrom, fmt::Display};

use tari_max_size::MaxSizeBytes;
use tari_utilities::ByteArray;

use crate::contacts_service::{error::ContactsServiceError, proto, types::MessageId};

#[derive(Clone, Debug, Default)]
pub struct Confirmation {
    pub message_id: MessageId,
    pub timestamp: u64,
}

impl TryFrom<proto::Confirmation> for Confirmation {
    type Error = ContactsServiceError;

    fn try_from(confirmation: proto::Confirmation) -> Result<Self, Self::Error> {
        Ok(Self {
            message_id: MaxSizeBytes::try_from(confirmation.message_id)?,
            timestamp: confirmation.timestamp,
        })
    }
}

impl From<Confirmation> for proto::Confirmation {
    fn from(confirmation: Confirmation) -> Self {
        Self {
            message_id: confirmation.message_id.to_vec(),
            timestamp: confirmation.timestamp,
        }
    }
}

impl Display for Confirmation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Confirmation: message_id: {}, timestamp: {}",
            self.message_id, self.timestamp
        )
    }
}
