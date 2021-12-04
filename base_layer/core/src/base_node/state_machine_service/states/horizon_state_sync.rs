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

mod horizon_state_synchronization;
use horizon_state_synchronization::HorizonStateSynchronization;

use super::{
    events_and_states::{HorizonSyncInfo, HorizonSyncStatus},
    StateEvent,
    StateInfo,
};
use crate::{
    base_node::{sync::SyncPeer, BaseNodeStateMachine},
    chain_storage::BlockchainBackend,
    transactions::CryptoFactories,
};
use log::*;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

#[derive(Clone, Debug)]
pub struct HorizonStateSync {
    sync_peer: SyncPeer,
}

impl HorizonStateSync {
    pub fn new(sync_peer: SyncPeer) -> Self {
        Self { sync_peer }
    }

    pub fn into_sync_peer(self) -> SyncPeer {
        self.sync_peer
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        let local_metadata = match shared.db.get_chain_metadata().await {
            Ok(metadata) => metadata,
            Err(err) => return err.into(),
        };

        let last_header = match shared.db.fetch_last_header().await {
            Ok(h) => h,
            Err(err) => return err.into(),
        };

        let horizon_sync_height = local_metadata.horizon_block(last_header.height);
        if local_metadata.pruned_height() >= horizon_sync_height {
            info!(target: LOG_TARGET, "Horizon state was already synchronized.");
            return StateEvent::HorizonStateSynchronized;
        }

        // We're already synced because we have full blocks higher than our target pruned height
        if local_metadata.height_of_longest_chain() >= horizon_sync_height {
            info!(
                target: LOG_TARGET,
                "Tip height is higher than our pruned height. Horizon state is already synchronized."
            );
            return StateEvent::HorizonStateSynchronized;
        }

        let info = HorizonSyncInfo::new(vec![self.sync_peer.node_id().clone()], HorizonSyncStatus::Starting);
        shared.set_state_info(StateInfo::HorizonSync(info));

        let prover = CryptoFactories::default().range_proof;
        let mut horizon_state = HorizonStateSynchronization::new(shared, &self.sync_peer, horizon_sync_height, prover);

        match horizon_state.synchronize().await {
            Ok(()) => {
                info!(target: LOG_TARGET, "Horizon state has synchronized.");
                StateEvent::HorizonStateSynchronized
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Synchronizing horizon state has failed. {}", err);
                StateEvent::HorizonStateSyncFailure
            },
        }
    }
}

impl From<SyncPeer> for HorizonStateSync {
    fn from(sync_peer: SyncPeer) -> Self {
        Self { sync_peer }
    }
}
