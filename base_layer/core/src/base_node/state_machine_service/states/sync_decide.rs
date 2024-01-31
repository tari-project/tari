//  Copyright 2021, The Tari Project
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

use log::*;

use crate::{
    base_node::{
        state_machine_service::{
            states::{HeaderSyncState, StateEvent},
            BaseNodeStateMachine,
        },
        sync::SyncPeer,
    },
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::sync_decide";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecideNextSync {
    sync_peers: Vec<SyncPeer>,
    is_synced: bool,
}

impl DecideNextSync {
    pub fn is_synced(&self) -> bool {
        self.is_synced
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(&mut self, shared: &BaseNodeStateMachine<B>) -> StateEvent {
        use StateEvent::{Continue, FatalError, ProceedToBlockSync, ProceedToHorizonSync};
        let local_metadata = match shared.db.get_chain_metadata().await {
            Ok(m) => m,
            Err(e) => {
                return FatalError(format!("Could not get local blockchain metadata. {}", e));
            },
        };

        debug!(
            target: LOG_TARGET,
            "Selecting a suitable sync peer from {} peer(s)",
            self.sync_peers.len()
        );

        if local_metadata.pruning_horizon() > 0 {
            // Filter sync peers that claim to be able to provide blocks up until our pruned height
            debug!(target: LOG_TARGET, "Local metadata: {}", local_metadata);
            let mut sync_peers = self.sync_peers.clone();
            let sync_peers = sync_peers
                .drain(..)
                .filter(|sync_peer| {
                    let remote_metadata = sync_peer.claimed_chain_metadata();
                    debug!(target: LOG_TARGET, "Peer metadata: {}", remote_metadata);
                    let remote_is_archival_node = remote_metadata.pruned_height() == 0;
                    let general_sync_conditions =
                        // Must be able to provide the correct amount of full blocks past the pruned height (i.e. the
                        // pruning horizon), otherwise our horizon spec will not be met
                        remote_metadata.best_block_height().saturating_sub(remote_metadata.pruned_height()) >=
                            local_metadata.pruning_horizon() &&
                        // Must have a better blockchain tip than us
                        remote_metadata.best_block_height() > local_metadata.best_block_height() &&
                        // Must be able to provide full blocks from the height we need detailed information
                        remote_metadata.pruned_height() <= local_metadata.best_block_height();
                    let sync_from_prune_node = !remote_is_archival_node &&
                        // Must have done initial sync (to detect genesis TXO spends)
                        local_metadata.best_block_height() > 0;
                    general_sync_conditions && (remote_is_archival_node || sync_from_prune_node)
                })
                .collect::<Vec<_>>();

            if sync_peers.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Unable to find any appropriate sync peers for horizon sync, trying for block sync"
                );
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Proceeding to horizon sync with {} sync peer(s) with a best latency of {:.2?}",
                    sync_peers.len(),
                    sync_peers.first().map(|p| p.latency()).unwrap_or_default()
                );
                return ProceedToHorizonSync(sync_peers);
            }
        }

        // This is not a pruned node or horizon sync is not possible, try for block sync

        // Filter sync peers that are able to provide full blocks from our current tip
        let sync_peers = self
            .sync_peers
            .drain(..)
            .filter(|sync_peer| {
                let remote_metadata = sync_peer.claimed_chain_metadata();
                remote_metadata.pruned_height() <= local_metadata.best_block_height()
            })
            .collect::<Vec<_>>();

        if sync_peers.is_empty() {
            warn!(target: LOG_TARGET, "Unable to find any appropriate sync peers for block sync");
            return Continue;
        }

        debug!(
            target: LOG_TARGET,
            "Proceeding to block sync with {} sync peer(s) with a best latency of {:.2?}",
            sync_peers.len(),
            sync_peers.first().map(|p| p.latency()).unwrap_or_default()
        );
        ProceedToBlockSync(sync_peers)
    }
}

impl From<HeaderSyncState> for DecideNextSync {
    fn from(sync: HeaderSyncState) -> Self {
        let is_synced = sync.is_synced();
        let mut sync_peers = sync.into_sync_peers();
        sync_peers.sort();
        DecideNextSync { sync_peers, is_synced }
    }
}
