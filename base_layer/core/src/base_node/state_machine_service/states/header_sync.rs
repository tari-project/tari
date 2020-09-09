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

use crate::{
    base_node::state_machine_service::{
        states::{
            block_sync::BlockSyncError,
            helpers,
            sync_peers::SyncPeer,
            BlockSyncInfo,
            HorizonStateSync,
            StateEvent,
            StatusInfo,
            SyncPeers,
        },
        BaseNodeStateMachine,
    },
    blocks::BlockHeader,
    chain_storage::{async_db, BlockchainBackend, BlockchainDatabase, ChainMetadata, ChainStorageError},
    iterators::VecChunkIter,
    validation::ValidationError,
};
use log::*;
use std::cmp;
use tari_crypto::tari_utilities::Hashable;
use thiserror::Error;
use tokio::{task, task::spawn_blocking};

const LOG_TARGET: &str = "c::bn::state_machine_service::states::header_sync";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeaderSync {
    pub network_metadata: ChainMetadata,
    pub sync_peers: SyncPeers,
}

impl HeaderSync {
    pub fn new(network_metadata: ChainMetadata, sync_peers: SyncPeers) -> Self {
        Self {
            network_metadata,
            sync_peers,
        }
    }

    pub async fn next_event<B: BlockchainBackend + 'static>(
        &mut self,
        shared: &mut BaseNodeStateMachine<B>,
    ) -> StateEvent
    {
        match async_db::get_chain_metadata(shared.db.clone()).await {
            Ok(local_metadata) => {
                shared
                    .set_status_info(StatusInfo::HeaderSync(BlockSyncInfo::new(
                        self.network_metadata.height_of_longest_chain(),
                        local_metadata.height_of_longest_chain(),
                        self.sync_peers.clone(),
                    )))
                    .await;

                let sync_height = self.get_sync_height(&shared, &local_metadata);
                info!(
                    target: LOG_TARGET,
                    "Synchronizing to height {}. Network tip height is {}.",
                    sync_height,
                    self.network_metadata.height_of_longest_chain()
                );
                let local_tip_height = local_metadata.height_of_longest_chain();
                if local_tip_height >= sync_height {
                    debug!(target: LOG_TARGET, "Header state already synchronized.");
                    return StateEvent::HeadersSynchronized(local_metadata, sync_height);
                }
                debug!(target: LOG_TARGET, "Horizon sync starting to height {}", sync_height);

                let mut sync = HeaderSynchronisation {
                    shared,
                    sync_peers: &mut self.sync_peers,
                    sync_height,
                };
                match sync.synchronize().await {
                    Ok(()) => {
                        info!(target: LOG_TARGET, "Block headers synchronised.");
                        StateEvent::HeadersSynchronized(local_metadata, sync_height)
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Block header synchronization has failed. {:?}", err);
                        StateEvent::HeaderSyncFailure
                    },
                }
            },
            Err(err) => StateEvent::FatalError(format!("Unable to retrieve local chain metadata. {:?}", err)),
        }
    }

    fn get_sync_height<B: BlockchainBackend>(
        &self,
        shared: &BaseNodeStateMachine<B>,
        local_metadata: &ChainMetadata,
    ) -> u64
    {
        if local_metadata.is_archival_node() {
            return self.network_metadata.height_of_longest_chain();
        }
        let network_tip_height = self.network_metadata.height_of_longest_chain();

        let horizon_sync_height_offset = shared.config.horizon_sync_config.horizon_sync_height_offset;

        calc_sync_height(network_tip_height, local_metadata, horizon_sync_height_offset)
    }
}

impl From<HorizonStateSync> for HeaderSync {
    fn from(s: HorizonStateSync) -> Self {
        Self {
            network_metadata: s.network_metadata,
            sync_peers: s.sync_peers,
        }
    }
}

struct HeaderSynchronisation<'a, 'b, B> {
    shared: &'a mut BaseNodeStateMachine<B>,
    sync_peers: &'b mut SyncPeers,
    sync_height: u64,
}

impl<B: BlockchainBackend + 'static> HeaderSynchronisation<'_, '_, B> {
    pub async fn synchronize(&mut self) -> Result<(), HeaderSyncError> {
        let tip_header = async_db::fetch_tip_header(self.db()).await?;
        debug!(
            target: LOG_TARGET,
            "Syncing from height {} to sync height {}.", tip_header.height, self.sync_height
        );

        trace!(target: LOG_TARGET, "Synchronizing headers");
        self.synchronize_headers(&tip_header).await?;

        Ok(())
    }

    async fn synchronize_headers(&mut self, tip_header: &BlockHeader) -> Result<(), HeaderSyncError> {
        let tip_height = tip_header.height;
        let config = self.shared.config.block_sync_config;

        let chunks = VecChunkIter::new(tip_height + 1, self.sync_height + 1, config.header_request_size);
        for block_nums in chunks {
            let num_sync_peers = self.sync_peers.len();
            for attempt in 1..=num_sync_peers {
                let (headers, sync_peer) = helpers::request_headers(
                    LOG_TARGET,
                    self.shared,
                    self.sync_peers,
                    &block_nums,
                    config.max_header_request_retry_attempts,
                )
                .await?;

                match self.validate_and_insert_headers(&block_nums, headers).await {
                    Ok(_) => {
                        self.shared
                            .set_status_info(StatusInfo::HeaderSync(BlockSyncInfo::new(
                                self.sync_height,
                                *block_nums.last().unwrap(),
                                Clone::clone(&*self.sync_peers),
                            )))
                            .await;
                        debug!(
                            target: LOG_TARGET,
                            "Successfully added headers {} to {} to the database",
                            block_nums.first().unwrap(),
                            block_nums.last().unwrap()
                        );
                        break;
                    },
                    Err(err @ HeaderSyncError::EmptyResponse) |
                    Err(err @ HeaderSyncError::IncorrectResponse) |
                    Err(err @ HeaderSyncError::InvalidHeader(_)) => {
                        warn!(target: LOG_TARGET, "Peer `{}`: {}", sync_peer, err);
                        debug!(
                            target: LOG_TARGET,
                            "Banning peer {} from local node, because they supplied an invalid response", sync_peer
                        );
                        self.ban_sync_peer(sync_peer).await?;
                    },
                    // Fatal
                    Err(e) => return Err(e),
                }

                if attempt == num_sync_peers {
                    debug!(target: LOG_TARGET, "Reached maximum ({}) attempts", attempt);
                    return Err(HeaderSyncError::MaxSyncAttemptsReached);
                }
                debug!(
                    target: LOG_TARGET,
                    "Retrying header sync. Attempt {} of {}", attempt, num_sync_peers
                );
            }
        }

        Ok(())
    }

    // Check the received set of headers.
    async fn validate_and_insert_headers(
        &self,
        block_nums: &[u64],
        headers: Vec<BlockHeader>,
    ) -> Result<(), HeaderSyncError>
    {
        if headers.is_empty() {
            return Err(HeaderSyncError::EmptyResponse);
        }
        // Check that the received headers are the requested headers
        if (0..block_nums.len()).any(|i| headers[i].height != block_nums[i]) {
            return Err(HeaderSyncError::IncorrectResponse);
        }
        // Check that header set forms a sequence
        for index in 1..headers.len() {
            let prev_header = &headers[index - 1];
            let curr_header = &headers[index];
            if prev_header.height + 1 != curr_header.height {
                return Err(HeaderSyncError::InvalidHeader(format!(
                    "Headers heights are not in sequence. (Previous height: {}, Current height: {})",
                    prev_header.height, curr_header.height
                )));
            }
            if curr_header.prev_hash != prev_header.hash() {
                return Err(HeaderSyncError::InvalidHeader(
                    "Headers do not form a chain.".to_string(),
                ));
            }
        }
        // Check that the first header is linked to the chain tip header
        assert_eq!(
            headers.is_empty(),
            false,
            "validate_headers: headers.is_empty() assertion failed"
        );
        let first_header = &headers[0];
        let db = &self.shared.db;
        let tip_header = async_db::fetch_tip_header(db.clone()).await?;
        if tip_header.height + 1 != first_header.height {
            return Err(HeaderSyncError::InvalidHeader(format!(
                "Headers do not link to the current chain tip header (Tip height = {}, Received header height = {})",
                tip_header.height, first_header.height
            )));
        }
        if first_header.prev_hash != tip_header.hash() {
            return Err(HeaderSyncError::InvalidHeader(
                "Headers do not form a chain from the current tip.".to_string(),
            ));
        }

        // Validate and insert each header
        let validator = self.shared.sync_validators.header.clone();
        let db = self.db();
        spawn_blocking(move || -> Result<(), HeaderSyncError> {
            for header in headers {
                validator
                    .validate(&header)
                    .map_err(HeaderSyncError::HeaderValidationFailed)?;
                db.insert_valid_headers(vec![header])?;
            }
            Ok(())
        })
        .await??;

        Ok(())
    }

    async fn ban_sync_peer(&mut self, sync_peer: SyncPeer) -> Result<(), HeaderSyncError> {
        helpers::ban_sync_peer(
            LOG_TARGET,
            &mut self.shared.connectivity,
            self.sync_peers,
            sync_peer,
            self.shared.config.sync_peer_config.peer_ban_duration,
        )
        .await?;
        Ok(())
    }

    #[inline]
    fn db(&self) -> BlockchainDatabase<B> {
        self.shared.db.clone()
    }
}

#[derive(Debug, Error)]
pub enum HeaderSyncError {
    #[error("Peer sent an empty response")]
    EmptyResponse,
    #[error("Peer sent an invalid response")]
    IncorrectResponse,
    #[error("Received invalid headers from peer: {0}")]
    InvalidHeader(String),
    #[error("Exceeded maximum sync attempts")]
    MaxSyncAttemptsReached,
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Block sync error: {0}")]
    BlockSyncError(#[from] BlockSyncError),
    #[error("Header validation failed: {0}")]
    HeaderValidationFailed(ValidationError),
    #[error("Join error: {0}")]
    JoinError(#[from] task::JoinError),
}

fn calc_sync_height(network_tip: u64, local_metadata: &ChainMetadata, horizon_offset: u64) -> u64 {
    let pruning_horizon = local_metadata.pruning_horizon;
    let target_height = network_tip.saturating_sub(pruning_horizon + horizon_offset);
    // Can never sync to lower than our current network tip
    cmp::max(target_height, local_metadata.height_of_longest_chain())
}

#[cfg(test)]
mod test {
    use crate::{
        base_node::state_machine_service::states::header_sync::calc_sync_height,
        chain_storage::ChainMetadata,
    };

    #[test]
    fn calc_sync_height_zero_pruning_horizon() {
        let metadata = ChainMetadata {
            pruning_horizon: 0,
            ..Default::default()
        };
        assert_eq!(calc_sync_height(100, &metadata, 0), 100);
        assert_eq!(calc_sync_height(100, &metadata, 5), 95);
        assert_eq!(calc_sync_height(100, &metadata, 500), 0);
    }

    #[test]
    fn calc_sync_height_non_zero_pruning_horizon() {
        let metadata = ChainMetadata {
            pruning_horizon: 100,
            ..Default::default()
        };
        assert_eq!(calc_sync_height(0, &metadata, 0), 0);
        assert_eq!(calc_sync_height(100, &metadata, 0), 0);
        assert_eq!(calc_sync_height(101, &metadata, 0), 1);
        assert_eq!(calc_sync_height(1000, &metadata, 0), 900);
    }

    #[test]
    fn calc_sync_height_behind_chain_tip() {
        let metadata = ChainMetadata {
            pruning_horizon: 100,
            height_of_longest_chain: Some(50),
            ..Default::default()
        };
        assert_eq!(calc_sync_height(0, &metadata, 0), 50);
        assert_eq!(calc_sync_height(100, &metadata, 0), 50);
        assert_eq!(calc_sync_height(101, &metadata, 0), 50);
        assert_eq!(calc_sync_height(1000, &metadata, 0), 900);
    }

    #[test]
    fn calc_sync_height_infront_chain_tip() {
        let metadata = ChainMetadata {
            pruning_horizon: 50,
            height_of_longest_chain: Some(100),
            ..Default::default()
        };
        assert_eq!(calc_sync_height(0, &metadata, 0), 100);
        assert_eq!(calc_sync_height(200, &metadata, 0), 150);
        assert_eq!(calc_sync_height(200, &metadata, 1), 149);
        assert_eq!(calc_sync_height(101, &metadata, 0), 100);
    }
}
