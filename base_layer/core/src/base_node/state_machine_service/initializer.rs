// Copyright 2020. The Tari Project
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

use std::sync::Arc;

use log::*;
use tari_network::NetworkHandle;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::sync::{broadcast, watch};

use crate::{
    base_node::{
        chain_metadata_service::ChainMetadataHandle,
        state_machine_service::{
            handle::StateMachineHandle,
            state_machine::{BaseNodeStateMachine, BaseNodeStateMachineConfig},
            states::StatusInfo,
        },
        sync::SyncValidators,
        LocalNodeCommsInterface,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend},
    consensus::ConsensusManager,
    proof_of_work::randomx_factory::RandomXFactory,
    transactions::CryptoFactories,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::initializer";

pub struct BaseNodeStateMachineInitializer<B> {
    db: AsyncBlockchainDb<B>,
    config: BaseNodeStateMachineConfig,
    rules: ConsensusManager,
    factories: CryptoFactories,
    randomx_factory: RandomXFactory,
    bypass_range_proof_verification: bool,
}

impl<B> BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        db: AsyncBlockchainDb<B>,
        config: BaseNodeStateMachineConfig,
        rules: ConsensusManager,
        factories: CryptoFactories,
        randomx_factory: RandomXFactory,
        bypass_range_proof_verification: bool,
    ) -> Self {
        Self {
            db,
            config,
            rules,
            factories,
            randomx_factory,
            bypass_range_proof_verification,
        }
    }
}

#[async_trait]
impl<B> ServiceInitializer for BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Base Node State Machine Service");
        let (state_event_publisher, _) = broadcast::channel(500);
        let (status_event_sender, status_event_receiver) = watch::channel(StatusInfo::new());

        let handle = StateMachineHandle::new(
            state_event_publisher.clone(),
            status_event_receiver,
            context.get_shutdown_signal(),
        );
        let peer_metadata = handle.peer_metadata_store();
        context.register_handle(handle);

        let factories = self.factories.clone();
        let rules = self.rules.clone();
        let db = self.db.clone();
        let config = self.config.clone();
        let randomx_factory = self.randomx_factory.clone();
        let bypass_range_proof_verification = self.bypass_range_proof_verification;

        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        context.spawn_when_ready(move |handles| async move {
            log_mdc::extend(mdc);
            let chain_metadata_service = handles.expect_handle::<ChainMetadataHandle>();
            let node_local_interface = handles.expect_handle::<LocalNodeCommsInterface>();

            let sync_validators =
                SyncValidators::full_consensus(rules.clone(), factories, bypass_range_proof_verification);
            let network = handles.expect_handle::<NetworkHandle>();

            let node = BaseNodeStateMachine::new(
                db,
                node_local_interface,
                network,
                chain_metadata_service.get_event_stream(),
                config,
                sync_validators,
                status_event_sender,
                state_event_publisher,
                randomx_factory,
                rules,
                peer_metadata,
                handles.get_shutdown_signal(),
            );

            node.run().await;
            info!(target: LOG_TARGET, "Base Node State Machine Service has shut down");
        });
        debug!(target: LOG_TARGET, "Base Node State Machine Service initialized");

        Ok(())
    }
}
