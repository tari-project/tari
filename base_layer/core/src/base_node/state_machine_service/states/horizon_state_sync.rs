//   Copyright 2020, The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//! # Horizon state sync
//!
//! Horizon state synchronisation module for pruned mode.

// TODO: Move the horizon synchronizer to the `sync` module

mod config;
pub use self::config::HorizonSyncConfig;

mod error;
pub use error::HorizonSyncError;

mod state_sync;
use state_sync::HorizonStateSynchronization;

use super::{BlockSyncInfo, StateEvent, StateInfo};
use crate::{
    base_node::{sync::SyncPeers, BaseNodeStateMachine},
    chain_storage::BlockchainBackend,
};
use log::*;
use tari_common_types::chain_metadata::ChainMetadata;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HorizonStateSync {
    pub local_metadata: ChainMetadata,
    pub network_metadata: ChainMetadata,
    pub sync_peers: SyncPeers,
    pub sync_height: u64,
}

impl HorizonStateSync {
    pub fn new(
        local_metadata: ChainMetadata,
        network_metadata: ChainMetadata,
        sync_peers: SyncPeers,
        sync_height: u64,
    ) -> Self
    {
        Self {
            local_metadata,
            network_metadata,
            sync_peers,
            sync_height,
        }
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        shared.set_state_info(StateInfo::HorizonSync(BlockSyncInfo::new(
            self.network_metadata.height_of_longest_chain(),
            self.local_metadata.height_of_longest_chain(),
            self.sync_peers.iter().map(|p| p.node_id.clone()).collect(),
        )));

        assert!(
            self.local_metadata.is_pruned_node(),
            "Entered horizon state sync but node is not in pruned mode"
        );

        info!(
            target: LOG_TARGET,
            "Synchronizing horizon state to height {}. Network tip height is {}.",
            self.sync_height,
            self.network_metadata.height_of_longest_chain()
        );
        let local_tip_height = self.local_metadata.height_of_longest_chain();
        if local_tip_height >= self.sync_height {
            debug!(target: LOG_TARGET, "Horizon state already synchronized.");
            return StateEvent::HorizonStateSynchronized;
        }
        debug!(
            target: LOG_TARGET,
            "Horizon sync starting to height {}", self.sync_height
        );

        let mut horizon_header_sync =
            HorizonStateSynchronization::new(shared, &mut self.sync_peers, &self.local_metadata, self.sync_height);
        match horizon_header_sync.synchronize().await {
            Ok(()) => {
                info!(target: LOG_TARGET, "Horizon state has synchronised.");
                StateEvent::HorizonStateSynchronized
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Synchronizing horizon state has failed. {}", err);
                StateEvent::HorizonStateSyncFailure
            },
        }
    }
}
