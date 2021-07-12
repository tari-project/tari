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

use crate::dan_layer::{models::HotStuffMessage, services::infrastructure_services::InboundConnectionService};
use async_trait::async_trait;
use std::collections::VecDeque;
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
