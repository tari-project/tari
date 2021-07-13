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

use crate::{
    dan_layer::{
        models::{Committee, HotStuffMessage},
        services::infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub fn mock_inbound() -> MockInboundConnectionService {
    MockInboundConnectionService::new()
}

#[derive()]
pub struct MockInboundConnectionService {
    messages: (Sender<HotStuffMessage>, Receiver<HotStuffMessage>),
}

impl Clone for MockInboundConnectionService {
    fn clone(&self) -> Self {
        // Not a true clone
        MockInboundConnectionService::new()
    }
}

#[async_trait]
impl InboundConnectionService for MockInboundConnectionService {
    async fn receive_message(&mut self) -> HotStuffMessage {
        self.messages.1.recv().await.unwrap()
    }
}

impl MockInboundConnectionService {
    pub fn new() -> Self {
        Self { messages: channel(10) }
    }

    pub fn push(&mut self, message: HotStuffMessage) {
        self.messages.0.try_send(message).unwrap()
    }
}

pub fn mock_outbound<TAddr: NodeAddressable>(committee: Vec<TAddr>) -> MockOutboundService<TAddr> {
    MockOutboundService::new(committee)
}

pub struct MockOutboundService<TAddr: NodeAddressable> {
    inbound_senders: HashMap<TAddr, Sender<HotStuffMessage>>,
    inbounds: HashMap<TAddr, MockInboundConnectionService>,
}

impl<TAddr: NodeAddressable> MockOutboundService<TAddr> {
    pub fn new(committee: Vec<TAddr>) -> Self {
        let mut inbounds = HashMap::new();
        let mut inbound_senders = HashMap::new();
        for member in committee {
            let inbound = mock_inbound();
            inbound_senders.insert(member.clone(), inbound.messages.0.clone());
            inbounds.insert(member.clone(), inbound);
        }
        Self {
            inbounds,
            inbound_senders,
        }
    }

    pub fn take_inbound(&mut self, member: &TAddr) -> Option<MockInboundConnectionService> {
        self.inbounds.remove(member)
    }
}

#[async_trait]
impl<TAddr: NodeAddressable + Send> OutboundService for MockOutboundService<TAddr> {
    async fn send<TAddr2: NodeAddressable + Send>(
        &mut self,
        to: TAddr2,
        message: HotStuffMessage,
    ) -> Result<(), DigitalAssetError> {
        todo!()
    }
}
