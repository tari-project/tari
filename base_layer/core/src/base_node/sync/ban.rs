//  Copyright 2023, The Tari Project
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

use std::time::Duration;

use log::*;
use tari_network::{identity::PeerId, NetworkHandle, NetworkingService};

use crate::base_node::BlockchainSyncConfig;

const LOG_TARGET: &str = "c::bn::sync";

// Sync peers are banned if there exists a ban reason for the error and the peer is not on the allow list for sync.

pub struct PeerBanManager {
    config: BlockchainSyncConfig,
    network: NetworkHandle,
}

impl PeerBanManager {
    pub fn new(config: BlockchainSyncConfig, network: NetworkHandle) -> Self {
        Self { config, network }
    }

    pub async fn ban_peer_if_required(&mut self, peer_id: PeerId, ban_reason: String, ban_duration: Duration) {
        if self.config.forced_sync_peers.contains(&peer_id) {
            debug!(
                target: LOG_TARGET,
                "Not banning peer that is on the allow list for sync. Ban reason = {}", ban_reason
            );
            return;
        }
        debug!(target: LOG_TARGET, "Sync peer {} removed from the sync peer list because {}", peer_id, ban_reason);

        match self
            .network
            .ban_peer(peer_id, ban_reason.clone(), Some(ban_duration))
            .await
        {
            Ok(_) => {
                warn!(target: LOG_TARGET, "Banned sync peer {} for {:?} because {}", peer_id, ban_duration, ban_reason)
            },
            Err(err) => error!(target: LOG_TARGET, "Failed to ban sync peer {}: {}", peer_id, err),
        }
    }
}
