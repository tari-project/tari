//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use tari_common_types::types::PublicKey;
use tari_dan_core::{
    models::{domain_events::ConsensusWorkerDomainEvent, TariDanPayload},
    services::{
        BaseLayerCheckpointManager,
        BaseLayerCommitteeManager,
        ConcreteAssetProcessor,
        ConcreteAssetProxy,
        LoggingEventsPublisher,
        MempoolServiceHandle,
        NodeIdentitySigningService,
        ServiceSpecification,
        TariDanPayloadProcessor,
        TariDanPayloadProvider,
    },
};
use tari_dan_storage_sqlite::{
    SqliteChainBackendAdapter,
    SqliteDbFactory,
    SqliteStateDbBackendAdapter,
    SqliteStorageService,
};

use crate::{
    dan_node::RunningServiceSpecification,
    grpc::services::{base_node_client::GrpcBaseNodeClient, wallet_client::GrpcWalletClient},
    p2p::services::{
        inbound_connection_service::TariCommsInboundReceiverHandle,
        outbound_connection_service::TariCommsOutboundService,
        rpc_client::TariCommsValidatorNodeClientFactory,
    },
};

#[derive(Default, Clone)]
pub struct DefaultServiceSpecification;

impl ServiceSpecification for DefaultServiceSpecification {
    type Addr = PublicKey;
    type AssetProcessor = ConcreteAssetProcessor;
    type AssetProxy = ConcreteAssetProxy<Self>;
    type BaseNodeClient = GrpcBaseNodeClient;
    type ChainDbBackendAdapter = SqliteChainBackendAdapter;
    type ChainStorageService = SqliteStorageService;
    type CheckpointManager = BaseLayerCheckpointManager<Self::WalletClient>;
    type CommitteeManager = BaseLayerCommitteeManager<Self::BaseNodeClient>;
    type DbFactory = SqliteDbFactory;
    type EventsPublisher = LoggingEventsPublisher<ConsensusWorkerDomainEvent>;
    type InboundConnectionService = TariCommsInboundReceiverHandle;
    type MempoolService = MempoolServiceHandle;
    type OutboundService = TariCommsOutboundService<Self::Payload>;
    type Payload = TariDanPayload;
    type PayloadProcessor = TariDanPayloadProcessor<Self::AssetProcessor>;
    type PayloadProvider = TariDanPayloadProvider<Self::MempoolService>;
    type SigningService = NodeIdentitySigningService;
    type StateDbBackendAdapter = SqliteStateDbBackendAdapter;
    type ValidatorNodeClientFactory = TariCommsValidatorNodeClientFactory;
    type WalletClient = GrpcWalletClient;
}

impl RunningServiceSpecification for DefaultServiceSpecification {}
