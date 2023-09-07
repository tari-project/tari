// Copyright 2019. The Tari Project
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

use std::time::Instant;

use log::*;

use crate::{
    base_node::{
        comms_interface::BlockEvent,
        metrics,
        state_machine_service::states::{BlockSyncInfo, HorizonStateSync, StateEvent, StateInfo, StatusInfo},
        sync::{BlockSynchronizer, SyncPeer},
        BaseNodeStateMachine,
    },
    chain_storage::{BlockAddResult, BlockchainBackend},
};

const LOG_TARGET: &str = "c::bn::block_sync";

#[derive(Debug)]
pub struct BlockSync {
    sync_peers: Vec<SyncPeer>,
    is_synced: bool,
}

impl BlockSync {
    // converting u64 to i64 is okay as the its only used for metrics
    #[allow(clippy::cast_possible_wrap)]
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        let mut synchronizer = BlockSynchronizer::new(
            shared.config.blockchain_sync_config.clone(),
            shared.db.clone(),
            shared.connectivity.clone(),
            &mut self.sync_peers,
            shared.sync_validators.block_body.clone(),
        );

        let status_event_sender = shared.status_event_sender.clone();
        let bootstrapped = shared.is_bootstrapped();
        let local_nci = shared.local_node_interface.clone();
        let randomx_vm_cnt = shared.get_randomx_vm_cnt();
        let randomx_vm_flags = shared.get_randomx_vm_flags();
        let tip_height_metric = metrics::tip_height();
        synchronizer.on_starting(move |sync_peer| {
            let _result = status_event_sender.send(StatusInfo {
                bootstrapped,
                state_info: StateInfo::Connecting(sync_peer.clone()),
                randomx_vm_cnt,
                randomx_vm_flags,
            });
        });

        let status_event_sender = shared.status_event_sender.clone();
        synchronizer.on_progress(move |block, remote_tip_height, sync_peer| {
            let local_height = block.height();
            local_nci.publish_block_event(BlockEvent::ValidBlockAdded(
                block.block().clone().into(),
                BlockAddResult::Ok(block),
            ));

            tip_height_metric.set(local_height as i64);
            let _result = status_event_sender.send(StatusInfo {
                bootstrapped,
                state_info: StateInfo::BlockSync(BlockSyncInfo {
                    tip_height: remote_tip_height,
                    local_height,
                    sync_peer: sync_peer.clone(),
                }),
                randomx_vm_cnt,
                randomx_vm_flags,
            });
        });

        let local_nci = shared.local_node_interface.clone();
        synchronizer.on_complete(move |block, starting_height| {
            local_nci.publish_block_event(BlockEvent::BlockSyncComplete(block, starting_height));
        });

        let timer = Instant::now();
        let state_event = match synchronizer.synchronize().await {
            Ok(()) => {
                info!(target: LOG_TARGET, "Blocks synchronized in {:.0?}", timer.elapsed());
                self.is_synced = true;
                StateEvent::BlocksSynchronized
            },
            Err(err) => {
                let _ignore = shared.status_event_sender.send(StatusInfo {
                    bootstrapped,
                    state_info: StateInfo::SyncFailed(err.to_short_str().to_string()),
                    randomx_vm_cnt,
                    randomx_vm_flags,
                });
                warn!(target: LOG_TARGET, "Block sync failed: {}", err);
                if let Err(e) = shared.db.swap_to_highest_pow_chain().await {
                    error!(
                        target: LOG_TARGET,
                        "Failed to reset chain to highest proof of work: {}", e
                    );
                }
                StateEvent::BlockSyncFailed
            },
        };

        // Cleanup
        if let Err(e) = shared.db.cleanup_orphans().await {
            warn!(target: LOG_TARGET, "Failed to remove orphan blocks: {}", e);
        }
        match shared.db.clear_all_pending_headers().await {
            Ok(num_cleared) => {
                debug!(
                    target: LOG_TARGET,
                    "Cleared {} pending headers from database", num_cleared
                );
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Failed to clear pending headers: {}", e);
            },
        }

        state_event
    }

    pub fn is_synced(&self) -> bool {
        self.is_synced
    }
}

impl From<HorizonStateSync> for BlockSync {
    fn from(sync: HorizonStateSync) -> Self {
        sync.into_sync_peers().into()
    }
}

impl From<Vec<SyncPeer>> for BlockSync {
    fn from(sync_peers: Vec<SyncPeer>) -> Self {
        Self {
            sync_peers,
            is_synced: false,
        }
    }
}
