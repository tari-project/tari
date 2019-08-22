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

use crate::{
    message::InboundMessage,
    peer_manager::PeerNodeIdentity,
    pub_sub_channel::{SubscriptionReader, TopicPublisherSubscriberError, TopicSubscription},
    types::CommsPublicKey,
};
use derive_error::Error;
use std::{fmt::Debug, sync::Arc};
use tari_utilities::message_format::MessageFormat;
use tokio::runtime::Runtime;

#[derive(Debug, Error)]
pub enum DomainSubscriberError {
    /// Error reading from subscription
    TopicPublisherSubscriberError(TopicPublisherSubscriberError),
    /// Subscription stream ended
    SubscriptionStreamEnded,
    /// Message deserialization error
    MessageError,
    /// Subscription Reader is not initialized
    SubscriptionReaderNotInitialized,
}

/// Information about the message received
#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub peer_source: PeerNodeIdentity,
    pub origin_source: CommsPublicKey,
}

pub struct SyncDomainSubscription<MType>
where MType: Eq + Send + Debug
{
    reader: Option<SubscriptionReader<MType, InboundMessage>>,
    runtime: Runtime,
}

impl<MType> SyncDomainSubscription<MType>
where MType: Eq + Send + Debug + Sync + 'static
{
    pub fn new(subscription: TopicSubscription<MType, InboundMessage>) -> Self {
        SyncDomainSubscription {
            reader: Some(SubscriptionReader::new(Arc::new(subscription))),
            runtime: Runtime::new().expect("Tokio could not create a Runtime"),
        }
    }

    pub fn receive_messages<T>(&mut self) -> Result<Vec<(MessageInfo, T)>, DomainSubscriberError>
    where T: MessageFormat {
        if let Some(s) = self.reader.take() {
            let (messages, returned_arc) = self.runtime.block_on(s)?;
            self.reader = Some(SubscriptionReader::new(
                returned_arc.ok_or(DomainSubscriberError::SubscriptionStreamEnded)?,
            ));

            let mut result = Vec::new();

            for m in messages {
                result.push((
                    MessageInfo {
                        peer_source: m.peer_source,
                        origin_source: m.origin_source,
                    },
                    m.message
                        .deserialize_message()
                        .map_err(|_| DomainSubscriberError::MessageError)?,
                ));
            }

            Ok(result)
        } else {
            Err(DomainSubscriberError::SubscriptionReaderNotInitialized)
        }
    }
}
