//  Copyright 2022, The Tari Project
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

use std::fmt::{Display, Formatter};

use tari_comms::peer_manager::NodeId;

use crate::base_node::sync::SyncPeer;

/// Info about the state of horizon sync
#[derive(Clone, Debug, PartialEq)]
pub struct HorizonSyncInfo {
    pub sync_peers: Vec<NodeId>,
    pub status: HorizonSyncStatus,
}

impl HorizonSyncInfo {
    pub fn new(sync_peers: Vec<NodeId>, status: HorizonSyncStatus) -> HorizonSyncInfo {
        HorizonSyncInfo { sync_peers, status }
    }

    pub fn to_progress_string(&self) -> String {
        use HorizonSyncStatus::*;
        match self.status {
            Starting => "Starting horizon sync".to_string(),
            Kernels {
                current,
                total,
                ref sync_peer,
            } => format!(
                "Syncing kernels: {}/{} ({:.0}%) from {}{} Latency: {:.2?}",
                current,
                total,
                current as f64 / total as f64 * 100.0,
                sync_peer.node_id(),
                sync_peer
                    .items_per_second()
                    .map(|kps| format!(" ({:.2?} kernels/s)", kps))
                    .unwrap_or_default(),
                sync_peer.latency().unwrap_or_default()
            ),
            Outputs {
                current,
                total,
                ref sync_peer,
            } => format!(
                "Syncing outputs: {}/{} ({:.0}%) from {}{} Latency: {:.2?}",
                current,
                total,
                current as f64 / total as f64 * 100.0,
                sync_peer.node_id(),
                sync_peer
                    .items_per_second()
                    .map(|kps| format!(" ({:.2?} outputs/s)", kps))
                    .unwrap_or_default(),
                sync_peer.latency().unwrap_or_default()
            ),
            Finalizing => "Finalizing horizon sync".to_string(),
        }
    }
}

impl Display for HorizonSyncInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        writeln!(f, "Syncing horizon state from the following peers:")?;
        for peer in &self.sync_peers {
            writeln!(f, "{}", peer)?;
        }

        match self.status.clone() {
            HorizonSyncStatus::Starting => write!(f, "Starting horizon state synchronization"),
            HorizonSyncStatus::Kernels {
                current,
                total,
                sync_peer,
            } => write!(
                f,
                "Horizon syncing kernels: {}/{} from {} (latency: {:.2?})",
                current,
                total,
                sync_peer.node_id(),
                sync_peer.latency().unwrap_or_default()
            ),
            HorizonSyncStatus::Outputs {
                current,
                total,
                sync_peer,
            } => {
                write!(
                    f,
                    "Horizon syncing outputs: {}/{} from {} (latency: {:.2?})",
                    current,
                    total,
                    sync_peer.node_id(),
                    sync_peer.latency().unwrap_or_default()
                )
            },
            HorizonSyncStatus::Finalizing => write!(f, "Finalizing horizon state synchronization"),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum HorizonSyncStatus {
    Starting,
    Kernels {
        current: u64,
        total: u64,
        sync_peer: SyncPeer,
    },
    Outputs {
        current: u64,
        total: u64,
        sync_peer: SyncPeer,
    },
    Finalizing,
}
