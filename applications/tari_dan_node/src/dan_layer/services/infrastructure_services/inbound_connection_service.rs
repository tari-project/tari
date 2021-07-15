// Copyright 2021. The Tari Project
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

use crate::dan_layer::models::{HotStuffMessage, InstructionSet, Payload};

use crate::dan_layer::services::infrastructure_services::NodeAddressable;
use async_trait::async_trait;
use futures::Stream;
use std::{marker::PhantomData, sync::Arc};
use tari_comms::types::CommsPublicKey;
use tari_p2p::{comms_connector::PeerMessage, domain_message::DomainMessage};

#[async_trait]
pub trait InboundConnectionService<TAddr: NodeAddressable, TPayload: Payload> {
    async fn receive_message(&mut self) -> (TAddr, HotStuffMessage<TPayload>);
}

pub struct TariCommsInboundConnectionService<TPayload: Payload> {
    stream: Box<dyn Stream<Item = Arc<PeerMessage>>>,
    // TODO: remove
    phantom: PhantomData<TPayload>,
}

impl<TPayload: Payload> TariCommsInboundConnectionService<TPayload> {
    pub fn new(stream: impl Stream<Item = Arc<PeerMessage>>) -> Self {
        Self {
            stream: Box::new(stream),
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<TPayload: Payload> InboundConnectionService<CommsPublicKey, TPayload>
    for TariCommsInboundConnectionService<TPayload>
{
    async fn receive_message(&mut self) -> (CommsPublicKey, HotStuffMessage<TPayload>) {
        todo!()
    }
}
