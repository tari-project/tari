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
        models::{Instruction, Payload, View, ViewId},
        services::{BftReplicaService, MempoolService, PayloadProvider},
    },
    digital_assets_error::DigitalAssetError,
};

pub struct MockMempoolService {}

impl MempoolService for MockMempoolService {
    fn submit_instruction(&mut self, instruction: Instruction) -> Result<(), DigitalAssetError> {
        todo!()
    }
}

pub fn mock_mempool() -> MockMempoolService {
    MockMempoolService {}
}

pub struct MockBftReplicaService {
    current_view: View,
}

impl MockBftReplicaService {
    pub fn new() -> Self {
        Self {
            current_view: View {
                view_id: ViewId(0),
                is_leader: false,
            },
        }
    }
}

impl BftReplicaService for MockBftReplicaService {
    fn current_view(&self) -> View {
        self.current_view.clone()
    }
}

pub fn mock_bft() -> MockBftReplicaService {
    MockBftReplicaService::new()
}

pub fn mock_static_payload_provider<TPayload: Payload>(
    static_payload: TPayload,
) -> MockStaticPayloadProvider<TPayload> {
    MockStaticPayloadProvider { static_payload }
}

pub struct MockStaticPayloadProvider<TPayload: Payload> {
    static_payload: TPayload,
}

impl<TPayload: Payload> PayloadProvider<TPayload> for MockStaticPayloadProvider<TPayload> {
    fn create_payload(&self) -> TPayload {
        self.static_payload.clone()
    }

    fn create_genesis_payload(&self) -> TPayload {
        self.static_payload.clone()
    }
}

pub fn mock_payload_provider() -> MockStaticPayloadProvider<&'static str> {
    MockStaticPayloadProvider {
        static_payload: "<Empty>",
    }
}
