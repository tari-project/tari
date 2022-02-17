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

use std::{cmp::Ordering, time::Instant};

use log::*;
use tari_common_types::chain_metadata::ChainMetadata;

use crate::{
    base_node::{
        comms_interface::BlockEvent,
        state_machine_service::states::{BlockSyncInfo, StateEvent, StateInfo, StatusInfo},
        sync::{BlockHeaderSyncError, HeaderSynchronizer, SyncPeer},
        BaseNodeStateMachine,
    },
    chain_storage::BlockchainBackend,
};

const LOG_TARGET: &str = "c::bn::header_sync";

#[derive(Clone, Debug)]
pub struct HeaderSyncState {
    sync_peers: Vec<SyncPeer>,
    is_synced: bool,
    local_metadata: ChainMetadata,
}

impl HeaderSyncState {
    pub fn new(mut sync_peers: Vec<SyncPeer>, local_metadata: ChainMetadata) -> Self {
        // Sort by latency lowest to highest
        sync_peers.sort_by(|a, b| match (a.latency(), b.latency()) {
            (None, None) => Ordering::Equal,
            // No latency goes to the end
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(la), Some(lb)) => la.cmp(&lb),
        });
        Self {
            sync_peers,
            is_synced: false,
            local_metadata,
        }
    }

    pub fn is_synced(&self) -> bool {
        self.is_synced
    }

    pub fn into_sync_peers(self) -> Vec<SyncPeer> {
        self.sync_peers
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        let mut synchronizer = HeaderSynchronizer::new(
            shared.config.blockchain_sync_config.clone(),
            shared.db.clone(),
            shared.consensus_rules.clone(),
            shared.connectivity.clone(),
            &mut self.sync_peers,
            shared.randomx_factory.clone(),
            &self.local_metadata,
        );

        let status_event_sender = shared.status_event_sender.clone();
        let bootstrapped = shared.is_bootstrapped();
        let randomx_vm_cnt = shared.get_randomx_vm_cnt();
        let randomx_vm_flags = shared.get_randomx_vm_flags();
        synchronizer.on_starting(move || {
            let _ = status_event_sender.send(StatusInfo {
                bootstrapped,
                state_info: StateInfo::HeaderSync(None),
                randomx_vm_cnt,
                randomx_vm_flags,
            });
        });

        let status_event_sender = shared.status_event_sender.clone();
        synchronizer.on_progress(move |current_height, remote_tip_height, sync_peer| {
            let details = BlockSyncInfo {
                tip_height: remote_tip_height,
                local_height: current_height,
                sync_peer: sync_peer.clone(),
            };
            let _ = status_event_sender.send(StatusInfo {
                bootstrapped,
                state_info: StateInfo::HeaderSync(Some(details)),
                randomx_vm_cnt,
                randomx_vm_flags,
            });
        });

        let local_nci = shared.local_node_interface.clone();
        synchronizer.on_rewind(move |blocks| {
            local_nci.publish_block_event(BlockEvent::BlockSyncRewind(blocks));
        });

        let timer = Instant::now();
        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        match synchronizer.synchronize().await {
            Ok(sync_peer) => {
                log_mdc::extend(mdc);
                info!(
                    target: LOG_TARGET,
                    "Headers synchronized from peer {} in {:.0?}",
                    sync_peer,
                    timer.elapsed()
                );
                // Move the sync peer used in header sync to the front of the queue
                if let Some(pos) = self.sync_peers.iter().position(|p| *p == sync_peer) {
                    if pos > 0 {
                        let sync_peer = self.sync_peers.remove(pos);
                        self.sync_peers.insert(0, sync_peer);
                    }
                }
                self.is_synced = true;
                StateEvent::HeadersSynchronized(sync_peer)
            },
            Err(err @ BlockHeaderSyncError::SyncFailedAllPeers) => {
                log_mdc::extend(mdc);
                warn!(target: LOG_TARGET, "{}. Continuing...", err);
                StateEvent::Continue
            },
            Err(err @ BlockHeaderSyncError::NetworkSilence) => {
                log_mdc::extend(mdc);
                warn!(target: LOG_TARGET, "{}", err);
                self.is_synced = true;
                StateEvent::NetworkSilence
            },
            Err(err) => {
                log_mdc::extend(mdc);
                debug!(target: LOG_TARGET, "Header sync failed: {}", err);
                StateEvent::HeaderSyncFailed
            },
        }
    }
}
