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
//

use crate::{
    base_node::{
        comms_interface::CommsInterfaceError,
        states::{
            Starting,
            StateEvent,
            StateEvent::{FatalError, MetadataSynced},
            SyncStatus,
        },
        BackOff,
        BaseNodeStateMachine,
    },
    chain_storage::{BlockchainBackend, ChainMetadata},
};

use log::*;
use std::time::Duration;

const LOG_TARGET: &str = "base_node::initial_sync";
// The number of times we'll request the chain metadata before giving up
const MAX_SYNC_ATTEMPTS: usize = 8;

pub struct InitialSync {
    // keeps track of how many times we've tried to sync with the network
    backoff: BackOff,
}

impl InitialSync {
    pub fn new() -> Self {
        let backoff = BackOff::new(MAX_SYNC_ATTEMPTS, Duration::from_secs(30), 1.0);
        InitialSync { backoff }
    }

    pub async fn next_event<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Starting blockchain metadata sync");
        self.sync_metadata(shared).await
    }

    /// Fetch the blockchain metadata from our internal database and compare it to data received from peers to decide
    /// on the next phase of the blockchain synchronisation.
    async fn sync_metadata<B: BlockchainBackend>(&mut self, shared: &mut BaseNodeStateMachine<B>) -> StateEvent {
        info!(target: LOG_TARGET, "Loading local blockchain metadata.");
        let ours = match shared.db.get_metadata() {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                return FatalError(msg);
            },
        };
        info!(
            target: LOG_TARGET,
            "Current local blockchain database information:\n {}", ours
        );
        // Fetch peer metadata
        let mut theirs = vec![];
        while !self.backoff.is_finished() {
            match shared.comms.get_metadata().await {
                Err(e) => {
                    log_error(e, self.backoff.attempts(), self.backoff.max_attempts());
                    self.backoff.wait().await;
                },
                Ok(data) => {
                    theirs = data;
                    self.backoff.stop();
                },
            }
        }
        if self.backoff.is_stopped() {
            self.evaluate_data(ours, theirs)
        } else {
            StateEvent::FatalError("Too many chain metadata attempts failed".into())
        }
    }

    fn evaluate_data(&self, ours: ChainMetadata, theirs: Vec<ChainMetadata>) -> StateEvent {
        // If there are no other nodes on the network, then we're at the chain tip by definition, so we go into
        // listen mode
        if theirs.is_empty() {
            return StateEvent::BlocksSynchronized;
        }
        let network = self.summarize_network_data(theirs);
        MetadataSynced(InitialSync::determine_sync_mode(ours, network))
    }

    fn summarize_network_data(&self, data: Vec<ChainMetadata>) -> ChainMetadata {
        // TODO: Use heuristics to weed out outliers / dishonest nodes.
        // Right now, we a simple strategy of returning the max height
        data.into_iter().fold(ChainMetadata::default(), |best, current| {
            if current.height_of_longest_chain.unwrap_or(0) >= best.height_of_longest_chain.unwrap_or(0) {
                current
            } else {
                best
            }
        })
    }

    /// Given a local and the network chain state respectively, figure out what synchronisation state we should be in.
    fn determine_sync_mode(local: ChainMetadata, network: ChainMetadata) -> SyncStatus {
        use crate::base_node::states::SyncStatus::*;
        match network.height_of_longest_chain {
            None => UpToDate,
            Some(network_tip) => {
                let horizon_block = local.horizon_block(network_tip);
                let local_tip = local.height_of_longest_chain.unwrap_or(0);
                if local_tip < horizon_block {
                    return BehindHorizon;
                }
                if local_tip < network_tip {
                    Lagging
                } else {
                    UpToDate
                }
            },
        }
    }
}

/// State management for Starting -> InitialSync. This state change occurs every time a node is restarted.
impl From<Starting> for InitialSync {
    fn from(_old_state: Starting) -> Self {
        InitialSync::new()
    }
}

fn log_error(e: CommsInterfaceError, att: usize, max_att: usize) {
    let msg = format!("Attempt {} of {}.", att, max_att);
    match e {
        // If the request timed out, we may be the only node on the network, thus we're up to date by definition
        CommsInterfaceError::RequestTimedOut => {
            debug!(
                target: LOG_TARGET,
                "Network request for chain metadata timed out. {}", msg
            );
        },
        CommsInterfaceError::TransportChannelError(e) => {
            error!(
                target: LOG_TARGET,
                "The base node input channel has closed unexpectedly. The best way to resolve this issue is to \
                 restart the node. {}. {}",
                e.to_string(),
                msg
            );
        },
        CommsInterfaceError::ChainStorageError(e) => {
            error!(
                target: LOG_TARGET,
                "There was a problem accessing the blockchain database. {}. {}.",
                e.to_string(),
                msg
            );
        },
        CommsInterfaceError::UnexpectedApiResponse => {
            warn!(target: LOG_TARGET, "MetadataSync got an unexpected response. {}", msg);
        },
        CommsInterfaceError::NoBootstrapNodesConfigured => {
            warn!(
                target: LOG_TARGET,
                "Cannot connect to the network; No seed nodes are configured. {}", msg
            );
        },
        CommsInterfaceError::OutboundMessageService(e) => {
            error!(
                target: LOG_TARGET,
                "There was a problem with the outbound message service. {}. {}.",
                e.to_string(),
                msg
            );
        },
    }
}
