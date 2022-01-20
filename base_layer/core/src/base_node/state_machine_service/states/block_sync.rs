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

use std::{mem, time::Instant};

use log::*;
use randomx_rs::RandomXFlag;

use crate::{
    base_node::{
        comms_interface::BlockEvent,
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
    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        let mut synchronizer = BlockSynchronizer::new(
            shared.config.blockchain_sync_config.clone(),
            shared.db.clone(),
            shared.connectivity.clone(),
            mem::take(&mut self.sync_peers),
            shared.sync_validators.block_body.clone(),
        );

        let status_event_sender = shared.status_event_sender.clone();
        let bootstrapped = shared.is_bootstrapped();
        let _ = status_event_sender.send(StatusInfo {
            bootstrapped,
            state_info: StateInfo::BlockSyncStarting,
            randomx_vm_cnt: 0,
            randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
        });
        let local_nci = shared.local_node_interface.clone();
        let randomx_vm_cnt = shared.get_randomx_vm_cnt();
        let randomx_vm_flags = shared.get_randomx_vm_flags();
        synchronizer.on_progress(move |block, remote_tip_height, sync_peer| {
            let local_height = block.height();
            local_nci.publish_block_event(BlockEvent::ValidBlockAdded(
                block.block().clone().into(),
                BlockAddResult::Ok(block),
            ));

            let _ = status_event_sender.send(StatusInfo {
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
        synchronizer.on_complete(move |block| {
            local_nci.publish_block_event(BlockEvent::BlockSyncComplete(block));
        });

        let timer = Instant::now();
        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        match synchronizer.synchronize().await {
            Ok(()) => {
                log_mdc::extend(mdc);
                info!(target: LOG_TARGET, "Blocks synchronized in {:.0?}", timer.elapsed());
                self.is_synced = true;
                StateEvent::BlocksSynchronized
            },
            Err(err) => {
                log_mdc::extend(mdc);
                warn!(target: LOG_TARGET, "Block sync failed: {}", err);
                StateEvent::BlockSyncFailed
            },
        }
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
