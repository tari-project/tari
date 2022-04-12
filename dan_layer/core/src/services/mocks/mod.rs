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
use tari_comms::types::CommsPublicKey;
use tari_crypto::ristretto::RistrettoPublicKey;

use crate::{
    digital_assets_error::DigitalAssetError,
    fixed_hash::FixedHash,
    models::{
        AssetDefinition,
        BaseLayerMetadata,
        BaseLayerOutput,
        Committee,
        Event,
        HotStuffTreeNode,
        Instruction,
        InstructionSet,
        Node,
        Payload,
        SchemaState,
        SideChainBlock,
        SidechainMetadata,
        Signature,
        StateOpLogEntry,
        StateRoot,
        TariDanPayload,
        TemplateId,
        TreeNodeHash,
    },
    services::{
        base_node_client::BaseNodeClient,
        infrastructure_services::NodeAddressable,
        AssetProcessor,
        CommitteeManager,
        ConcreteCheckpointManager,
        EventsPublisher,
        MempoolService,
        PayloadProcessor,
        PayloadProvider,
        SigningService,
        ValidatorNodeClientError,
        ValidatorNodeClientFactory,
        ValidatorNodeRpcClient,
        WalletClient,
    },
    storage::{
        chain::ChainDbUnitOfWork,
        state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
        ChainStorageService,
        StorageError,
    },
};
#[cfg(test)]
use crate::{
    models::domain_events::ConsensusWorkerDomainEvent,
    services::infrastructure_services::mocks::{MockInboundConnectionService, MockOutboundService},
    services::{ConcreteAssetProxy, ServiceSpecification},
    storage::mocks::{chain_db::MockChainDbBackupAdapter, state_db::MockStateDbBackupAdapter, MockDbFactory},
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

pub fn mock_static_payload_provider() -> MockStaticPayloadProvider<TariDanPayload> {
    let instruction_set = InstructionSet::empty();
    let payload = TariDanPayload::new(instruction_set, None);
    MockStaticPayloadProvider {
        static_payload: payload,
    }
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

pub fn mock_signing_service() -> MockSigningService<RistrettoPublicKey> {
    MockSigningService::<RistrettoPublicKey> { p: PhantomData }
}

pub struct MockSigningService<TAddr: NodeAddressable> {
    p: PhantomData<TAddr>,
}

impl<TAddr: NodeAddressable> SigningService<TAddr> for MockSigningService<TAddr> {
    fn sign(&self, _identity: &TAddr, _challenge: &[u8]) -> Result<Signature, DigitalAssetError> {
        Ok(Signature {})
    }
}

#[derive(Clone)]
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

    async fn check_if_in_committee(
        &mut self,
        _asset_public_key: PublicKey,
        _dan_node_public_key: PublicKey,
    ) -> Result<(bool, u64), DigitalAssetError> {
        todo!();
    }

    async fn get_assets_for_dan_node(
        &mut self,
        _dan_node_public_key: PublicKey,
    ) -> Result<Vec<(AssetDefinition, u64)>, DigitalAssetError> {
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
    pub committee: Committee<RistrettoPublicKey>,
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

#[derive(Default, Clone)]
pub struct MockWalletClient;

#[async_trait]
impl WalletClient for MockWalletClient {
    async fn create_new_checkpoint(
        &mut self,
        _asset_public_key: &PublicKey,
        _checkpoint_unique_id: &[u8],
        _state_root: &StateRoot,
        _next_committee: Vec<CommsPublicKey>,
    ) -> Result<(), DigitalAssetError> {
        Ok(())
    }
}

pub fn mock_wallet_client() -> MockWalletClient {
    MockWalletClient {}
}

#[derive(Default, Clone)]
pub struct MockValidatorNodeClientFactory;

#[derive(Default, Clone)]
pub struct MockValidatorNodeClient;

#[async_trait]
impl ValidatorNodeRpcClient for MockValidatorNodeClient {
    async fn invoke_read_method(
        &mut self,
        _asset_public_key: &PublicKey,
        _template_id: TemplateId,
        _method: String,
        _args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ValidatorNodeClientError> {
        Ok(None)
    }

    async fn invoke_method(
        &mut self,
        _asset_public_key: &PublicKey,
        _template_id: TemplateId,
        _method: String,
        _args: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ValidatorNodeClientError> {
        Ok(None)
    }

    async fn get_sidechain_blocks(
        &mut self,
        _asset_public_key: &PublicKey,
        _start_hash: TreeNodeHash,
        _end_hash: Option<TreeNodeHash>,
    ) -> Result<Vec<SideChainBlock>, ValidatorNodeClientError> {
        Ok(vec![])
    }

    async fn get_sidechain_state(
        &mut self,
        _asset_public_key: &PublicKey,
    ) -> Result<Vec<SchemaState>, ValidatorNodeClientError> {
        Ok(vec![])
    }

    async fn get_op_logs(
        &mut self,
        _asset_public_key: &PublicKey,
        _height: u64,
    ) -> Result<Vec<StateOpLogEntry>, ValidatorNodeClientError> {
        Ok(vec![])
    }

    async fn get_tip_node(&mut self, _asset_public_key: &PublicKey) -> Result<Option<Node>, ValidatorNodeClientError> {
        Ok(None)
    }
}

impl ValidatorNodeClientFactory for MockValidatorNodeClientFactory {
    type Addr = PublicKey;
    type Client = MockValidatorNodeClient;

    fn create_client(&self, _address: &Self::Addr) -> Self::Client {
        MockValidatorNodeClient::default()
    }
}

#[derive(Default, Clone)]
pub struct MockChainStorageService;

#[async_trait]
impl ChainStorageService<TariDanPayload> for MockChainStorageService {
    async fn get_metadata(&self) -> Result<SidechainMetadata, StorageError> {
        todo!()
    }

    async fn add_node<TUnitOfWork: ChainDbUnitOfWork>(
        &self,
        _node: &HotStuffTreeNode<TariDanPayload>,
        _db: TUnitOfWork,
    ) -> Result<(), StorageError> {
        Ok(())
    }
}

pub fn mock_checkpoint_manager() -> ConcreteCheckpointManager<MockWalletClient> {
    ConcreteCheckpointManager::<MockWalletClient>::new(AssetDefinition::default(), MockWalletClient::default())
}

#[derive(Default, Clone)]
pub struct MockServiceSpecification;

#[cfg(test)]
impl ServiceSpecification for MockServiceSpecification {
    type Addr = RistrettoPublicKey;
    type AssetProcessor = MockAssetProcessor;
    type AssetProxy = ConcreteAssetProxy<Self>;
    type BaseNodeClient = MockBaseNodeClient;
    type ChainDbBackendAdapter = MockChainDbBackupAdapter;
    type ChainStorageService = MockChainStorageService;
    type CheckpointManager = ConcreteCheckpointManager<Self::WalletClient>;
    type CommitteeManager = MockCommitteeManager;
    type DbFactory = MockDbFactory;
    type EventsPublisher = MockEventsPublisher<ConsensusWorkerDomainEvent>;
    type InboundConnectionService = MockInboundConnectionService<Self::Addr, Self::Payload>;
    type MempoolService = MockMempoolService;
    type OutboundService = MockOutboundService<Self::Addr, Self::Payload>;
    type Payload = TariDanPayload;
    type PayloadProcessor = MockPayloadProcessor;
    type PayloadProvider = MockStaticPayloadProvider<Self::Payload>;
    type SigningService = MockSigningService<Self::Addr>;
    type StateDbBackendAdapter = MockStateDbBackupAdapter;
    type ValidatorNodeClientFactory = MockValidatorNodeClientFactory;
    type WalletClient = MockWalletClient;
}
