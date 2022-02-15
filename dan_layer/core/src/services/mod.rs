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

mod asset_processor;
mod base_node_client;
mod committee_manager;
mod events_publisher;
pub mod infrastructure_services;
mod mempool_service;
mod payload_processor;
mod payload_provider;
mod signing_service;

pub use asset_processor::{AssetProcessor, ConcreteAssetProcessor, MemoryInstructionLog};
pub use asset_proxy::{AssetProxy, ConcreteAssetProxy};
pub use base_node_client::BaseNodeClient;
pub use committee_manager::{CommitteeManager, ConcreteCommitteeManager};
pub use events_publisher::{EventsPublisher, LoggingEventsPublisher};
pub use mempool_service::{ConcreteMempoolService, MempoolService, MempoolServiceHandle};
pub use payload_processor::{PayloadProcessor, TariDanPayloadProcessor};
pub use payload_provider::{PayloadProvider, TariDanPayloadProvider};
pub use signing_service::{NodeIdentitySigningService, SigningService};

mod asset_proxy;
mod checkpoint_manager;
pub mod mocks;
mod service_specification;
mod validator_node_rpc_client;
mod wallet_client;
pub use checkpoint_manager::{CheckpointManager, ConcreteCheckpointManager};
pub use service_specification::ServiceSpecification;
pub use validator_node_rpc_client::{ValidatorNodeClientError, ValidatorNodeClientFactory, ValidatorNodeRpcClient};
pub use wallet_client::WalletClient;
