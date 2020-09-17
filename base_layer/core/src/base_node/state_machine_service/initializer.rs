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
        state_machine_service::{
            handle::StateMachineHandle,
            state_machine::{BaseNodeStateMachine, BaseNodeStateMachineConfig},
            states::{BlockSyncStrategy, StatusInfo},
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
use std::sync::Arc;
use tari_broadcast_channel::{bounded, Publisher, Subscriber};
use tari_comms::{connectivity::ConnectivityRequester, PeerManager};
use tari_service_framework::{handles::ServiceHandlesFuture, ServiceInitializationError, ServiceInitializer};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::runtime;

const LOG_TARGET: &str = "c::bn::state_machine_service::initializer";

pub struct BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    db: BlockchainDatabase<B>,
    rules: ConsensusManager,
    factories: CryptoFactories,
    sync_strategy: BlockSyncStrategy,
    peer_manager: Arc<PeerManager>,
    connectivity_requester: ConnectivityRequester,
    interrupt_signal: ShutdownSignal,
}

impl<B> BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        db: BlockchainDatabase<B>,
        rules: ConsensusManager,
        factories: CryptoFactories,
        sync_strategy: BlockSyncStrategy,
        peer_manager: Arc<PeerManager>,
        connectivity_requester: ConnectivityRequester,
        interrupt_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            db,
            rules,
            factories,
            sync_strategy,
            peer_manager,
            connectivity_requester,
            interrupt_signal,
        }
    }
}

impl<B> ServiceInitializer for BaseNodeStateMachineInitializer<B>
where B: BlockchainBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        _shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        let (state_event_publisher, state_event_subscriber): (Publisher<_>, Subscriber<_>) = bounded(10, 3);
        let (status_event_sender, status_event_receiver) = tokio::sync::watch::channel(StatusInfo::new());

        let shutdown = Shutdown::new();
        let handle = StateMachineHandle::new(state_event_subscriber, status_event_receiver, shutdown.to_signal());
        handles_fut.register(handle);

        let factories = self.factories.clone();
        let sync_strategy = self.sync_strategy;
        let peer_manager = self.peer_manager.clone();
        let connectivity_requester = self.connectivity_requester.clone();
        let rules = self.rules.clone();
        let db = self.db.clone();
        let interrupt_signal = self.interrupt_signal.clone();
        executor.spawn(async move {
            let handles = handles_fut.await;

            let outbound_interface = handles
                .get_handle::<OutboundNodeCommsInterface>()
                .expect("Problem getting node interface handle.");
            let chain_metadata_service = handles
                .get_handle::<ChainMetadataHandle>()
                .expect("Problem getting chain metadata interface handle.");
            let node_local_interface = handles
                .get_handle::<LocalNodeCommsInterface>()
                .expect("Problem getting node local interface handle.");

            let mut state_machine_config = BaseNodeStateMachineConfig::default();
            state_machine_config.block_sync_config.sync_strategy = sync_strategy;

            state_machine_config.horizon_sync_config.horizon_sync_height_offset =
                rules.consensus_constants().coinbase_lock_height() + 50;

            let sync_validators = SyncValidators::full_consensus(db.clone(), rules.clone(), factories.clone());
            let node = BaseNodeStateMachine::new(
                &db,
                &node_local_interface,
                &outbound_interface,
                peer_manager,
                connectivity_requester,
                chain_metadata_service.get_event_stream(),
                state_machine_config,
                sync_validators,
                interrupt_signal,
                shutdown,
                status_event_sender,
                state_event_publisher,
            );

            node.run().await;
            info!(target: LOG_TARGET, "Base Node State Machine Service has shut down");
        });

        future::ready(Ok(()))
    }
}
