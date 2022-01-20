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

use std::cmp::Ordering;

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
            let last_header = match shared.db.fetch_last_header().await {
                Ok(h) => h,
                Err(err) => return err.into(),
            };

            let horizon_sync_height = local_metadata.horizon_block(last_header.height);
            // Filter sync peers that claim to be able to provide blocks up until our pruned height
            let sync_peers = self
                .sync_peers
                .drain(..)
                .filter(|sync_peer| {
                    let remote_metadata = sync_peer.claimed_chain_metadata();
                    remote_metadata.height_of_longest_chain() >= horizon_sync_height
                })
                .collect::<Vec<_>>();

            if sync_peers.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Unable to find any appropriate sync peers for horizon sync"
                );
                return Continue;
            }

            debug!(
                target: LOG_TARGET,
                "Proceeding to horizon sync with {} sync peer(s) with a best latency of {:.2?}",
                sync_peers.len(),
                sync_peers.first().map(|p| p.latency()).unwrap_or_default()
            );
            ProceedToHorizonSync(sync_peers)
        } else {
            // Filter sync peers that are able to provide full blocks from our current tip
            let sync_peers = self
                .sync_peers
                .drain(..)
                .filter(|sync_peer| {
                    sync_peer.claimed_chain_metadata().pruning_horizon() <= local_metadata.height_of_longest_chain()
                })
                .collect::<Vec<_>>();

            if sync_peers.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Unable to find any appropriate sync peers for block sync"
                );
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
}

impl From<HeaderSyncState> for DecideNextSync {
    fn from(sync: HeaderSyncState) -> Self {
        sync.into_sync_peers().into()
    }
}

impl From<Vec<SyncPeer>> for DecideNextSync {
    fn from(mut sync_peers: Vec<SyncPeer>) -> Self {
        sync_peers.sort_by(|a, b| match (a.latency(), b.latency()) {
            (None, None) => Ordering::Equal,
            // No latency goes to the end
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(la), Some(lb)) => la.cmp(&lb),
        });
        Self { sync_peers }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use rand::{rngs::OsRng, seq::SliceRandom};
    use tari_common_types::chain_metadata::ChainMetadata;

    use super::*;

    mod sort_by_latency {
        use super::*;
        use crate::base_node::chain_metadata_service::PeerChainMetadata;

        #[test]
        fn it_sorts_by_latency() {
            let peers = (0..10)
                .map(|i| {
                    PeerChainMetadata::new(
                        Default::default(),
                        ChainMetadata::empty(),
                        Some(Duration::from_millis(i)),
                    )
                    .into()
                })
                .chain(Some(
                    PeerChainMetadata::new(Default::default(), ChainMetadata::empty(), None).into(),
                ))
                .collect::<Vec<SyncPeer>>();
            let mut shuffled = peers.clone();
            shuffled.shuffle(&mut OsRng);
            let decide = DecideNextSync::from(shuffled);
            assert_eq!(decide.sync_peers, peers);
        }
    }
}
