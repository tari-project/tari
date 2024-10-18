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

use log::*;

use super::{StateEvent, StateInfo};
use crate::{
    base_node::{
        state_machine_service::states::StatusInfo,
        sync::{HorizonStateSynchronization, SyncPeer},
        BaseNodeStateMachine,
    },
    chain_storage::BlockchainBackend,
    transactions::CryptoFactories,
};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::horizon_state_sync";

#[derive(Clone, Debug)]
pub struct HorizonStateSync {
    sync_peers: Vec<SyncPeer>,
}

impl HorizonStateSync {
    pub fn into_sync_peers(self) -> Vec<SyncPeer> {
        self.sync_peers
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent {
        let local_metadata = match shared.db.get_chain_metadata().await {
            Ok(metadata) => metadata,
            Err(err) => return err.into(),
        };

        let sync_peers = &mut self.sync_peers;
        // Order sync peers according to accumulated difficulty
        sync_peers.sort_by(|a, b| {
            b.claimed_chain_metadata()
                .accumulated_difficulty()
                .cmp(&a.claimed_chain_metadata().accumulated_difficulty())
        });

        // Target horizon sync height based on the last header we have synced
        let last_header = match shared.db.fetch_last_header().await {
            Ok(h) => h,
            Err(err) => return err.into(),
        };
        let target_horizon_sync_height = local_metadata.pruned_height_at_given_chain_tip(last_header.height);

        // Determine if we need to sync horizon state
        if local_metadata.pruned_height() >= target_horizon_sync_height {
            info!(target: LOG_TARGET, "Horizon state is already synchronized.");
            return StateEvent::HorizonStateSynchronized;
        }
        if local_metadata.best_block_height() >= target_horizon_sync_height {
            info!(
                target: LOG_TARGET,
                "Our tip height is higher than our target pruned height. Horizon state is already synchronized."
            );
            return StateEvent::HorizonStateSynchronized;
        }

        let db = shared.db.clone();
        let config = shared.config.blockchain_sync_config.clone();
        let connectivity = shared.network.clone();
        let rules = shared.consensus_rules.clone();
        let prover = CryptoFactories::default().range_proof;
        let validator = shared.sync_validators.final_horizon_state.clone();
        let mut horizon_sync = HorizonStateSynchronization::new(
            config,
            db,
            connectivity,
            rules,
            sync_peers,
            target_horizon_sync_height,
            prover,
            validator,
        );

        let status_event_sender = shared.status_event_sender.clone();
        let bootstrapped = shared.is_bootstrapped();
        let randomx_vm_cnt = shared.get_randomx_vm_cnt();
        let randomx_vm_flags = shared.get_randomx_vm_flags();
        horizon_sync.on_starting(move |sync_peer| {
            let _result = status_event_sender.send(StatusInfo {
                bootstrapped,
                state_info: StateInfo::Connecting(sync_peer.clone()),
                randomx_vm_cnt,
                randomx_vm_flags,
            });
        });

        let status_event_sender = shared.status_event_sender.clone();
        horizon_sync.on_progress(move |info| {
            let _result = status_event_sender.send(StatusInfo {
                bootstrapped,
                state_info: StateInfo::HorizonSync(info),
                randomx_vm_cnt,
                randomx_vm_flags,
            });
        });

        match horizon_sync.synchronize().await {
            Ok(()) => {
                info!(target: LOG_TARGET, "Horizon state has synchronized.");
                StateEvent::HorizonStateSynchronized
            },
            Err(err) => {
                let _ignore = shared.status_event_sender.send(StatusInfo {
                    bootstrapped,
                    state_info: StateInfo::SyncFailed("HorizonSyncFailed".to_string()),
                    randomx_vm_cnt,
                    randomx_vm_flags,
                });
                warn!(target: LOG_TARGET, "Synchronizing horizon state has failed. {}", err);
                StateEvent::HorizonStateSyncFailure
            },
        }
    }
}

impl From<Vec<SyncPeer>> for HorizonStateSync {
    fn from(sync_peers: Vec<SyncPeer>) -> Self {
        Self { sync_peers }
    }
}
