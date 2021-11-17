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

use crate::{
    base_node::{
        state_machine_service::{
            states::{HeaderSync, StateEvent},
            BaseNodeStateMachine,
        },
        sync::SyncPeer,
    },
    chain_storage::BlockchainBackend,
};
use log::*;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::sync_decide";

#[derive(Clone, Debug, PartialEq)]
pub struct DecideNextSync {
    sync_peers: Vec<SyncPeer>,
}

impl DecideNextSync {
    pub async fn next_event<B: BlockchainBackend + 'static>(&mut self, shared: &BaseNodeStateMachine<B>) -> StateEvent {
        use StateEvent::*;
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

        if shared.config.pruning_horizon > 0 {
            // Filter sync peers that claim to be able to provide full blocks up until our pruned height
            let sync_peers_iter = self.sync_peers.iter().filter(|sync_peer| {
                let chain_metadata = sync_peer.claimed_chain_metadata();
                let our_pruned_height_from_peer =
                    local_metadata.horizon_block(chain_metadata.height_of_longest_chain());
                let their_pruned_height = chain_metadata.pruned_height();
                our_pruned_height_from_peer >= their_pruned_height
            });

            match find_best_latency(sync_peers_iter) {
                Some(sync_peer) => ProceedToHorizonSync(sync_peer),
                None => Continue,
            }
        } else {
            // Filter sync peers that are able to provide full blocks from our current tip
            let sync_peers_iter = self.sync_peers.iter().filter(|sync_peer| {
                sync_peer.claimed_chain_metadata().pruning_horizon() <= local_metadata.height_of_longest_chain()
            });

            match find_best_latency(sync_peers_iter) {
                Some(sync_peer) => ProceedToBlockSync(sync_peer),
                None => Continue,
            }
        }
    }
}

/// Find the peer with the best latency
fn find_best_latency<'a, I: IntoIterator<Item = &'a SyncPeer>>(iter: I) -> Option<SyncPeer> {
    iter.into_iter()
        .fold(Option::<&'a SyncPeer>::None, |current, sync_peer| match current {
            Some(p) => match (p.latency(), sync_peer.latency()) {
                (Some(_), None) => Some(p),
                (None, Some(_)) => Some(sync_peer),
                (Some(current), Some(latency)) if current > latency => Some(sync_peer),
                (Some(_), Some(_)) | (None, None) => current,
            },
            None => Some(sync_peer),
        })
        .cloned()
}

impl From<HeaderSync> for DecideNextSync {
    fn from(sync: HeaderSync) -> Self {
        Self {
            sync_peers: sync.into_sync_peers(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::{rngs::OsRng, seq::SliceRandom};
    use tari_common_types::chain_metadata::ChainMetadata;

    mod find_best_latency {
        use super::*;

        #[test]
        fn it_selects_the_best_latency() {
            let peers = (0..10)
                .map(|i| SyncPeer::new(Default::default(), ChainMetadata::empty(), Some(i)))
                .chain(Some(SyncPeer::new(Default::default(), ChainMetadata::empty(), None)))
                .collect::<Vec<_>>();
            let mut shuffled = peers.clone();
            shuffled.shuffle(&mut OsRng);
            let selected = find_best_latency(shuffled.iter());
            assert_eq!(selected, peers.first().cloned());
        }
    }
}
