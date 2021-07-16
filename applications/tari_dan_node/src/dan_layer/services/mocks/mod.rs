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
        models::{Event, Instruction, Payload, Signature, View, ViewId},
        services::{
            infrastructure_services::NodeAddressable,
            BftReplicaService,
            EventsPublisher,
            MempoolService,
            PayloadProvider,
            SigningService,
        },
    },
    digital_assets_error::DigitalAssetError,
};
use std::{
    collections::{vec_deque::Iter, VecDeque},
    marker::PhantomData,
    sync::{Arc, Mutex},
};

pub struct MockMempoolService {}

impl MempoolService for MockMempoolService {
    fn submit_instruction(&mut self, instruction: Instruction) -> Result<(), DigitalAssetError> {
        todo!()
    }

    fn read_block(&self, limit: usize) -> Result<&[Instruction], DigitalAssetError> {
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
    fn create_payload(&self) -> Result<TPayload, DigitalAssetError> {
        Ok(self.static_payload.clone())
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

pub fn mock_events_publisher<TEvent: Event>() -> MockEventsPublisher<TEvent> {
    MockEventsPublisher::new()
}

#[derive(Clone)]
pub struct MockEventsPublisher<TEvent: Event> {
    events: Arc<Mutex<VecDeque<TEvent>>>,
}

impl<TEvent: Event> MockEventsPublisher<TEvent> {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn to_vec(&self) -> Vec<TEvent> {
        self.events.lock().unwrap().iter().map(|s| s.clone()).collect()
    }
}

impl<TEvent: Event> EventsPublisher<TEvent> for MockEventsPublisher<TEvent> {
    fn publish(&mut self, event: TEvent) {
        self.events.lock().unwrap().push_back(event)
    }
}

pub fn mock_signing_service<TAddr: NodeAddressable>() -> MockSigningService<TAddr> {
    MockSigningService::<TAddr> { p: PhantomData }
}

pub struct MockSigningService<TAddr: NodeAddressable> {
    p: PhantomData<TAddr>,
}

impl<TAddr: NodeAddressable> SigningService<TAddr> for MockSigningService<TAddr> {
    fn sign(&self, identity: &TAddr, challenge: &[u8]) -> Result<Signature, DigitalAssetError> {
        Ok(Signature {})
    }
}
