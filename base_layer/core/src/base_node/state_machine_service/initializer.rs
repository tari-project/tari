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

use crate::{
    base_node::{
        chain_metadata_service::ChainMetadataHandle,
        consts::FETCH_BLOCKS_SERVICE_REQUEST_MIN_TIMEOUT,
        state_machine_service::{
            handle::StateMachineHandle,
            state_machine::{BaseNodeStateMachine, BaseNodeStateMachineConfig},
            states::{BlockSyncConfig, BlockSyncStrategy, StatusInfo},
        },
        LocalNodeCommsInterface,
        OutboundNodeCommsInterface,
        SyncValidators,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    transactions::types::CryptoFactories,
};
use futures::{future, Future};
use log::*;
use std::{cmp, sync::Arc, time::Duration};
use tari_comms::{connectivity::ConnectivityRequester, PeerManager};
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::sync::{broadcast, watch};

const LOG_TARGET: &str = "c::bn::state_machine_service::initializer";

pub struct BaseNodeStateMachineInitializer<B> {
    db: BlockchainDatabase<B>,
    rules: ConsensusManager,
    factories: CryptoFactories,
    sync_strategy: BlockSyncStrategy,
    fetch_blocks_timeout: Duration,
}

impl<B> BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        db: BlockchainDatabase<B>,
        rules: ConsensusManager,
        factories: CryptoFactories,
        sync_strategy: BlockSyncStrategy,
        fetch_blocks_timeout: Duration,
    ) -> Self
    {
        Self {
            db,
            rules,
            factories,
            sync_strategy,
            fetch_blocks_timeout,
        }
    }
}

impl<B> ServiceInitializer for BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        trace!(target: LOG_TARGET, "init of base_node");
        let (state_event_publisher, _) = broadcast::channel(10);
        let (status_event_sender, status_event_receiver) = watch::channel(StatusInfo::new());

        let handle = StateMachineHandle::new(
            state_event_publisher.clone(),
            status_event_receiver,
            context.get_shutdown_signal(),
        );
        context.register_handle(handle);

        let factories = self.factories.clone();
        let sync_strategy = self.sync_strategy;
        let rules = self.rules.clone();
        let db = self.db.clone();
        let fetch_blocks_timeout = self.fetch_blocks_timeout.clone();
        context.spawn_when_ready(move |handles| async move {
            let outbound_interface = handles.expect_handle::<OutboundNodeCommsInterface>();
            let chain_metadata_service = handles.expect_handle::<ChainMetadataHandle>();
            let node_local_interface = handles.expect_handle::<LocalNodeCommsInterface>();
            let connectivity_requester = handles.expect_handle::<ConnectivityRequester>();
            let peer_manager = handles.expect_handle::<Arc<PeerManager>>();

            let mut state_machine_config = BaseNodeStateMachineConfig {
                block_sync_config: BlockSyncConfig {
                    fetch_blocks_timeout: cmp::max(FETCH_BLOCKS_SERVICE_REQUEST_MIN_TIMEOUT, fetch_blocks_timeout),
                    ..Default::default()
                },
                ..Default::default()
            };
            state_machine_config.block_sync_config.sync_strategy = sync_strategy;

            // TODO: This should move to checking each time
            state_machine_config.horizon_sync_config.horizon_sync_height_offset =
                rules.consensus_constants(0).coinbase_lock_height() + 50;

            let sync_validators = SyncValidators::full_consensus(db.clone(), rules.clone(), factories.clone());
            let node = BaseNodeStateMachine::new(
                &db,
                &node_local_interface,
                &outbound_interface,
                connectivity_requester,
                peer_manager,
                chain_metadata_service.get_event_stream(),
                state_machine_config,
                sync_validators,
                handles.get_shutdown_signal(),
                status_event_sender,
                state_event_publisher,
            );

            node.run().await;
            info!(target: LOG_TARGET, "Base Node State Machine Service has shut down");
        });

        future::ready(Ok(()))
    }
}
