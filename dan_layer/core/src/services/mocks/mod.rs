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

use std::{
    collections::VecDeque,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use tari_common_types::types::PublicKey;

use super::CommitteeManager;
use crate::{
    digital_assets_error::DigitalAssetError,
    fixed_hash::FixedHash,
    models::{
        AssetDefinition,
        BaseLayerMetadata,
        BaseLayerOutput,
        Committee,
        Event,
        Instruction,
        Payload,
        Signature,
        StateRoot,
        TreeNodeHash,
    },
    services::{
        base_node_client::BaseNodeClient,
        infrastructure_services::NodeAddressable,
        AssetProcessor,
        EventsPublisher,
        MempoolService,
        PayloadProcessor,
        PayloadProvider,
        SigningService,
    },
    storage::state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
};

#[derive(Debug, Clone)]
pub struct MockMempoolService;

#[async_trait]
impl MempoolService for MockMempoolService {
    async fn submit_instruction(&mut self, _instruction: Instruction) -> Result<(), DigitalAssetError> {
        Ok(())
    }

    async fn read_block(&self, _limit: usize) -> Result<Vec<Instruction>, DigitalAssetError> {
        Ok(vec![])
    }

    async fn reserve_instruction_in_block(
        &mut self,
        _instruction_hash: &FixedHash,
        _block_hash: TreeNodeHash,
    ) -> Result<(), DigitalAssetError> {
        todo!()
    }

    async fn remove_all_in_block(&mut self, _block_hash: &TreeNodeHash) -> Result<(), DigitalAssetError> {
        todo!()
    }

    async fn release_reservations(&mut self, _block_hash: &TreeNodeHash) -> Result<(), DigitalAssetError> {
        todo!()
    }

    async fn size(&self) -> usize {
        0
    }
}

pub fn create_mempool_mock() -> MockMempoolService {
    MockMempoolService
}

pub fn mock_static_payload_provider<TPayload: Payload>(
    static_payload: TPayload,
) -> MockStaticPayloadProvider<TPayload> {
    MockStaticPayloadProvider { static_payload }
}

pub struct MockStaticPayloadProvider<TPayload: Payload> {
    static_payload: TPayload,
}

#[async_trait]
impl<TPayload: Payload> PayloadProvider<TPayload> for MockStaticPayloadProvider<TPayload> {
    async fn create_payload(&self) -> Result<TPayload, DigitalAssetError> {
        Ok(self.static_payload.clone())
    }

    fn create_genesis_payload(&self, _: &AssetDefinition) -> TPayload {
        self.static_payload.clone()
    }

    async fn get_payload_queue(&self) -> usize {
        1
    }

    async fn reserve_payload(
        &mut self,
        _payload: &TPayload,
        _reservation_key: &TreeNodeHash,
    ) -> Result<(), DigitalAssetError> {
        todo!()
    }

    async fn remove_payload(&mut self, _reservation_key: &TreeNodeHash) -> Result<(), DigitalAssetError> {
        todo!()
    }
}

pub fn mock_payload_provider() -> MockStaticPayloadProvider<&'static str> {
    MockStaticPayloadProvider {
        static_payload: "<Empty>",
    }
}

pub fn mock_events_publisher<TEvent: Event>() -> MockEventsPublisher<TEvent> {
    MockEventsPublisher::default()
}

#[derive(Clone)]
pub struct MockEventsPublisher<TEvent: Event> {
    events: Arc<Mutex<VecDeque<TEvent>>>,
}

impl<TEvent: Event> Default for MockEventsPublisher<TEvent> {
    fn default() -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<TEvent: Event> MockEventsPublisher<TEvent> {
    pub fn to_vec(&self) -> Vec<TEvent> {
        self.events.lock().unwrap().iter().cloned().collect()
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
    fn sign(&self, _identity: &TAddr, _challenge: &[u8]) -> Result<Signature, DigitalAssetError> {
        Ok(Signature {})
    }
}

pub struct MockBaseNodeClient {}

#[async_trait]
impl BaseNodeClient for MockBaseNodeClient {
    async fn get_tip_info(&mut self) -> Result<BaseLayerMetadata, DigitalAssetError> {
        todo!();
    }

    async fn get_current_checkpoint(
        &mut self,
        _height: u64,
        _asset_public_key: PublicKey,
        _checkpoint_unique_id: Vec<u8>,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError> {
        todo!();
    }

    async fn get_assets_for_dan_node(
        &mut self,
        _dan_node_public_key: PublicKey,
    ) -> Result<Vec<AssetDefinition>, DigitalAssetError> {
        todo!();
    }

    async fn get_asset_registration(
        &mut self,
        _asset_public_key: PublicKey,
    ) -> Result<Option<BaseLayerOutput>, DigitalAssetError> {
        todo!()
    }
}

pub fn mock_base_node_client() -> MockBaseNodeClient {
    MockBaseNodeClient {}
}

#[derive(Clone)]
pub struct MockCommitteeManager {
    pub committee: Committee<&'static str>,
}

impl<TAddr: NodeAddressable> CommitteeManager<TAddr> for MockCommitteeManager {
    fn current_committee(&self) -> Result<&Committee<TAddr>, DigitalAssetError> {
        todo!();
    }

    fn read_from_checkpoint(&mut self, _output: BaseLayerOutput) -> Result<(), DigitalAssetError> {
        todo!();
    }
}

// pub fn _mock_template_service() -> MockTemplateService {
//     MockTemplateService {}
// }
//
// pub struct MockTemplateService {}
//
// #[async_trait]
// impl TemplateService for MockTemplateService {
//     async fn execute_instruction(&mut self, _instruction: &Instruction) -> Result<(), DigitalAssetError> {
//         dbg!("Executing instruction as mock");
//         Ok(())
//     }
// }

pub fn mock_payload_processor() -> MockPayloadProcessor {
    MockPayloadProcessor {}
}

pub struct MockPayloadProcessor {}

#[async_trait]
impl<TPayload: Payload> PayloadProcessor<TPayload> for MockPayloadProcessor {
    async fn process_payload<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        _payload: &TPayload,
        _unit_of_work: TUnitOfWork,
    ) -> Result<StateRoot, DigitalAssetError> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct MockAssetProcessor;

impl AssetProcessor for MockAssetProcessor {
    fn execute_instruction<TUnitOfWork: StateDbUnitOfWork>(
        &self,
        _instruction: &Instruction,
        _db: &mut TUnitOfWork,
    ) -> Result<(), DigitalAssetError> {
        todo!()
    }

    fn invoke_read_method<TUnifOfWork: StateDbUnitOfWorkReader>(
        &self,
        _instruction: &Instruction,
        _state_db: &TUnifOfWork,
    ) -> Result<Option<Vec<u8>>, DigitalAssetError> {
        todo!()
    }
}
