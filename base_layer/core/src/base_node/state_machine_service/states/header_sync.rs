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

use crate::{
    base_node::{
        comms_interface::BlockEvent,
        state_machine_service::states::{BlockSyncInfo, Listening, StateEvent, StateInfo, StatusInfo},
        sync::{HeaderSynchronizer, SyncPeers},
        BaseNodeStateMachine,
    },
    chain_storage::BlockchainBackend,
};
use log::*;
use std::time::Instant;
use tari_comms::peer_manager::NodeId;

const LOG_TARGET: &str = "c::bn::header_sync";

#[derive(Clone, Debug, Default)]
pub struct HeaderSync {
    sync_peers: Vec<NodeId>,
}

impl HeaderSync {
    pub fn new(sync_peers: Vec<NodeId>) -> Self {
        Self { sync_peers }
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        let sync_peers = if self.sync_peers.is_empty() {
            &shared.config.block_sync_config.sync_peers
        } else {
            &self.sync_peers
        };

        let mut synchronizer = HeaderSynchronizer::new(
            shared.config.block_sync_config.clone(),
            shared.db.clone(),
            shared.consensus_rules.clone(),
            shared.connectivity.clone(),
            sync_peers,
            shared.randomx_factory.clone(),
        );

        let status_event_sender = shared.status_event_sender.clone();
        let bootstrapped = shared.bootstrapped_sync;
        synchronizer.on_progress(move |current_height, remote_tip_height, sync_peers| {
            let _ = status_event_sender.broadcast(StatusInfo {
                bootstrapped,
                state_info: StateInfo::HeaderSync(BlockSyncInfo {
                    tip_height: remote_tip_height,
                    local_height: current_height,
                    sync_peers: sync_peers.to_vec(),
                }),
            });
        });

        let local_nci = shared.local_node_interface.clone();
        synchronizer.on_rewind(move |blocks| {
            local_nci.publish_block_event(BlockEvent::BlockSyncRewind(blocks));
        });

        let timer = Instant::now();
        match synchronizer.synchronize().await {
            Ok(sync_peer) => {
                info!(target: LOG_TARGET, "Headers synchronized in {:.0?}", timer.elapsed());
                StateEvent::HeadersSynchronized(sync_peer)
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Header sync failed: {}", err);
                StateEvent::HeaderSyncFailed
            },
        }
    }
}

impl From<Listening> for HeaderSync {
    fn from(_: Listening) -> Self {
        Default::default()
    }
}
impl From<SyncPeers> for HeaderSync {
    fn from(peers: SyncPeers) -> Self {
        Self::new(peers.into_iter().map(|p| p.node_id).collect())
    }
}
