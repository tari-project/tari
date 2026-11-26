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
use tari_common_types::chain_metadata::ChainMetadata;

use crate::{
    base_node::{
        state_machine_service::{
            BaseNodeStateMachine,
            states::{HeaderSyncState, StateEvent},
        },
        sync::SyncPeer,
    },
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::sync_decide";

/// Returns true if `remote` has not pruned past our current local tip and can supply sequential full blocks above it.
pub(super) fn can_sync_blocks_from(local: &ChainMetadata, remote: &ChainMetadata) -> bool {
    remote.pruned_height() <= local.best_block_height()
}

/// Returns true if `local` is in pruned mode and `remote` is ahead of `local` while retaining at least
/// `local.pruning_horizon()` full blocks above its `pruned_height`.
pub(super) fn can_sync_horizon_from(local: &ChainMetadata, remote: &ChainMetadata) -> bool {
    local.pruning_horizon() > 0 &&
        remote.best_block_height() > local.best_block_height() &&
        remote.best_block_height().saturating_sub(remote.pruned_height()) >= local.pruning_horizon()
}

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
                return FatalError(format!("Could not get local blockchain metadata. {e}"));
            },
        };

        debug!(
            target: LOG_TARGET,
            "Selecting a suitable sync peer from {} peer(s)",
            self.sync_peers.len()
        );

        if local_metadata.pruning_horizon() > 0 {
            // Filter sync peers that claim to be able to provide blocks up until our pruned height
            debug!(target: LOG_TARGET, "Local metadata: {local_metadata}");
            let mut sync_peers = self.sync_peers.clone();
            let sync_peers = sync_peers
                .drain(..)
                .filter(|sync_peer| {
                    let remote_metadata = sync_peer.claimed_chain_metadata();
                    debug!(target: LOG_TARGET, "Peer metadata: {remote_metadata}");
                    can_sync_horizon_from(&local_metadata, remote_metadata)
                })
                .collect::<Vec<_>>();

            if sync_peers.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Unable to find any appropriate sync peers for horizon sync, trying for block sync"
                );
            } else {
                // If our tip already covers the new horizon we have the full blocks locally — use block
                // sync + prune rather than re-downloading the entire aggregate snapshot from genesis.
                // The horizon comes from our validated header tip, never from a peer's claimed height.
                let last_header = match shared.db.fetch_last_header().await {
                    Ok(h) => h,
                    Err(e) => return FatalError(format!("Could not fetch the last header. {e}")),
                };
                let new_horizon = local_metadata.pruned_height_at_given_chain_tip(last_header.height);
                if local_metadata.best_block_height() >= new_horizon {
                    debug!(
                        target: LOG_TARGET,
                        "Local tip {} >= new horizon {}, using block sync + prune instead of horizon sync",
                        local_metadata.best_block_height(),
                        new_horizon
                    );
                    // Keep at least the pruning horizon of full blocks below our local tip.
                    let prune_target = new_horizon
                        .min(local_metadata.pruned_height_at_given_chain_tip(local_metadata.best_block_height()));
                    if local_metadata.pruned_height() < prune_target {
                        debug!(target: LOG_TARGET, "Pruning to new horizon height {prune_target}");
                        if let Err(e) = shared.db.prune_to_height(prune_target).await {
                            return FatalError(format!("Failed to prune to height {prune_target}: {e}"));
                        }
                    }
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
        }

        // This is not a pruned node or horizon sync is not possible, try for block sync

        // Filter sync peers that are able to provide full blocks from our current tip
        let sync_peers = self
            .sync_peers
            .drain(..)
            .filter(|sync_peer| {
                let remote_metadata = sync_peer.claimed_chain_metadata();
                can_sync_blocks_from(&local_metadata, remote_metadata)
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

#[cfg(test)]
mod test {
    use std::time::Duration;

    use primitive_types::U512;
    use tari_common_types::types::FixedHash;
    use tari_comms::{peer_manager::NodeId, test_utils::mocks::create_connectivity_mock};
    use tari_service_framework::reply_channel;
    use tari_shutdown::Shutdown;
    use tokio::sync::{broadcast, watch};

    use super::*;
    use crate::{
        base_node::{
            BaseNodeStateMachineConfig,
            LocalNodeCommsInterface,
            chain_metadata_service::PeerChainMetadata,
            state_machine_service::states::StatusInfo,
            sync::SyncValidators,
        },
        chain_storage::{BlockchainDatabaseConfig, Validators},
        proof_of_work::randomx_factory::RandomXFactory,
        test_helpers::{
            blockchain::{TempDatabase, create_new_blockchain, create_store_with_consensus_and_validators_and_config},
            create_consensus_rules,
            create_peer_manager,
        },
        validation::mocks::MockValidator,
    };

    fn make_metadata(height: u64, pruning_horizon: u64, pruned_height: u64, difficulty: u64) -> ChainMetadata {
        ChainMetadata::new(
            height,
            FixedHash::zero(),
            pruning_horizon,
            pruned_height,
            U512::from(difficulty),
            0,
        )
        .expect("valid chain metadata")
    }

    fn make_sync_peer(metadata: ChainMetadata, latency: Option<Duration>) -> SyncPeer {
        SyncPeer::from(PeerChainMetadata::new(NodeId::new(), metadata, latency))
    }

    fn make_state_machine(pruning_horizon: u64) -> BaseNodeStateMachine<TempDatabase> {
        let db = if pruning_horizon == 0 {
            create_new_blockchain()
        } else {
            let config = BlockchainDatabaseConfig {
                pruning_horizon,
                ..Default::default()
            };
            create_store_with_consensus_and_validators_and_config(
                create_consensus_rules(),
                Validators::new(
                    MockValidator::new(true),
                    MockValidator::new(true),
                    MockValidator::new(true),
                ),
                config,
            )
        };
        let (connectivity, _connectivity_mock) = create_connectivity_mock();
        let (_metadata_tx, metadata_rx) = broadcast::channel(10);
        let (_dht_tx, dht_rx) = broadcast::channel(10);
        let (req_tx, _) = reply_channel::unbounded();
        let (block_tx, _) = reply_channel::unbounded();
        let (block_event_tx, _) = broadcast::channel(1);
        let (status_tx, _status_rx) = watch::channel(StatusInfo::new());
        let (event_tx, _event_rx) = broadcast::channel(10);
        let shutdown = Shutdown::new();
        BaseNodeStateMachine::new(
            db.into(),
            LocalNodeCommsInterface::new(req_tx, block_tx, block_event_tx),
            connectivity,
            metadata_rx,
            create_peer_manager(),
            dht_rx,
            BaseNodeStateMachineConfig::default(),
            SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
            status_tx,
            event_tx,
            RandomXFactory::default(),
            create_consensus_rules(),
            shutdown.to_signal(),
        )
    }

    #[test]
    fn can_sync_blocks_and_horizon_predicates_match_pruned_and_archival_cases() {
        let archival_local = make_metadata(10, 0, 0, 100);
        let pruned_local = make_metadata(10, 50, 0, 100);

        let pruned_above_tip = make_metadata(100, 0, 80, 1000);
        assert!(!can_sync_blocks_from(&archival_local, &pruned_above_tip));
        assert!(!can_sync_horizon_from(&archival_local, &pruned_above_tip));
        assert!(!can_sync_blocks_from(&pruned_local, &pruned_above_tip));
        assert!(!can_sync_horizon_from(&pruned_local, &pruned_above_tip));

        let valid_horizon_peer = make_metadata(100, 50, 50, 1000);
        assert!(!can_sync_blocks_from(&pruned_local, &valid_horizon_peer));
        assert!(can_sync_horizon_from(&pruned_local, &valid_horizon_peer));

        let tighter_horizon_but_has_blocks = make_metadata(25, 10, 10, 250);
        assert!(can_sync_blocks_from(&pruned_local, &tighter_horizon_but_has_blocks));
        assert!(!can_sync_horizon_from(&pruned_local, &tighter_horizon_but_has_blocks));
    }

    #[tokio::test]
    async fn decide_next_sync_returns_continue_when_peers_pruned_past_tip() {
        let state_machine = make_state_machine(50);
        let local = make_metadata(0, 50, 0, 1);
        let unusable_peer = make_sync_peer(make_metadata(40, 20, 20, 400), Some(Duration::from_millis(10)));

        let header_sync = HeaderSyncState::new(vec![unusable_peer], local);
        let mut decide = DecideNextSync::from(header_sync);
        let event = decide.next_event(&state_machine).await;
        assert_eq!(event, StateEvent::Continue);
    }
}
