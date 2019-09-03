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

use crate::pub_sub_channel::{TopicPayload, TopicPublisher};
use derive_error::Error;
use futures::{executor::block_on, prelude::*};
use log::*;
use std::{fmt::Debug, sync::Mutex};
use tari_broadcast_channel::Publisher;

const LOG_TARGET: &str = "comms::inbound_message_service::inbound_message_publisher";

#[derive(Clone, Debug, Error)]
pub enum PublisherError {
    /// The Thread Safety has been breached and data access has become poisoned
    PoisonedAccess,
    /// Publisher is None inside Mutex, indicates dead lock
    PublisherLock,
    /// Publisher could not send message
    PublisherSendError,
}

pub struct InboundMessagePublisher<MType, T>
where
    MType: Send + Sync + Debug,
    T: Clone + Send + Sync,
{
    publisher: Mutex<Option<TopicPublisher<MType, T>>>,
}

impl<MType, T> InboundMessagePublisher<MType, T>
where
    MType: Send + Sync + 'static + Debug,
    T: Clone + Send + Sync + 'static,
{
    pub fn new(publisher: Publisher<TopicPayload<MType, T>>) -> InboundMessagePublisher<MType, T> {
        info!(target: LOG_TARGET, "Inbound Message Publisher created");
        InboundMessagePublisher {
            publisher: Mutex::new(Some(publisher)),
        }
    }

    pub fn publish(&self, message_type: MType, message: T) -> Result<(), PublisherError> {
        // TODO This mutex should not be required and is only present due the IMS workers being in their own threads.
        // Future refactor will remove the need for the lock and this Option container
        let mut publisher_lock = self.publisher.lock().map_err(|_| PublisherError::PoisonedAccess)?;
        match publisher_lock.take() {
            Some(mut p) => {
                info!(
                    target: LOG_TARGET,
                    "Inbound message of type {:?} about to be published", message_type
                );

                block_on(async { p.send(TopicPayload::new(message_type, message)).await })
                    .map_err(|_| PublisherError::PublisherSendError)?;

                *publisher_lock = Some(p);

                Ok(())
            },
            None => Err(PublisherError::PublisherLock),
        }
    }
}
