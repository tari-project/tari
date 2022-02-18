// Copyright 2020. The Tari Project
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

use std::fmt::{Display, Error, Formatter};

use randomx_rs::RandomXFlag;
use tari_common_types::chain_metadata::ChainMetadata;

use crate::base_node::{
    state_machine_service::states::{
        BlockSync,
        DecideNextSync,
        HeaderSyncState,
        HorizonStateSync,
        Listening,
        ListeningInfo,
        Shutdown,
        Starting,
        Waiting,
    },
    sync::{HorizonSyncInfo, SyncPeer},
};

#[derive(Debug)]
pub enum BaseNodeState {
    Starting(Starting),
    HeaderSync(HeaderSyncState),
    DecideNextSync(DecideNextSync),
    HorizonStateSync(HorizonStateSync),
    BlockSync(BlockSync),
    // The best network chain metadata
    Listening(Listening),
    // We're in a paused state, and will return to Listening after a timeout
    Waiting(Waiting),
    Shutdown(Shutdown),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateEvent {
    Initialized,
    HeadersSynchronized(SyncPeer),
    HeaderSyncFailed,
    ProceedToHorizonSync(Vec<SyncPeer>),
    ProceedToBlockSync(Vec<SyncPeer>),
    HorizonStateSynchronized,
    HorizonStateSyncFailure,
    BlocksSynchronized,
    BlockSyncFailed,
    FallenBehind(SyncStatus),
    NetworkSilence,
    FatalError(String),
    Continue,
    UserQuit,
}

impl<E: std::error::Error> From<E> for StateEvent {
    fn from(err: E) -> Self {
        Self::FatalError(err.to_string())
    }
}

/// Some state transition functions must return `SyncStatus`. The sync status indicates how far behind the network's
/// blockchain the local node is. It can either be very far behind (`LaggingBehindHorizon`), in which case we will just
/// synchronise against the pruning horizon; we're somewhat behind (`Lagging`) and need to download the missing
/// blocks to catch up, or we are `UpToDate`.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    // We are behind the chain tip.
    Lagging {
        local: ChainMetadata,
        network: ChainMetadata,
        sync_peers: Vec<SyncPeer>,
    },
    UpToDate,
}

impl SyncStatus {
    pub fn is_lagging(&self) -> bool {
        !self.is_up_to_date()
    }

    pub fn is_up_to_date(&self) -> bool {
        matches!(self, SyncStatus::UpToDate)
    }
}

impl Display for SyncStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use SyncStatus::*;
        match self {
            Lagging {
                network, sync_peers, ..
            } => write!(
                f,
                "Lagging behind {} peers (#{}, Difficulty: {})",
                sync_peers.len(),
                network.height_of_longest_chain(),
                network.accumulated_difficulty(),
            ),
            UpToDate => f.write_str("UpToDate"),
        }
    }
}

impl Display for StateEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use StateEvent::*;
        match self {
            Initialized => write!(f, "Initialized"),
            BlocksSynchronized => write!(f, "Synchronised Blocks"),
            HeadersSynchronized(peer) => write!(f, "Headers Synchronized from peer `{}`", peer),
            HeaderSyncFailed => write!(f, "Header Synchronization Failed"),
            ProceedToHorizonSync(_) => write!(f, "Proceed to horizon sync"),
            ProceedToBlockSync(_) => write!(f, "Proceed to block sync"),
            HorizonStateSynchronized => write!(f, "Horizon State Synchronized"),
            HorizonStateSyncFailure => write!(f, "Horizon State Synchronization Failed"),
            BlockSyncFailed => write!(f, "Block Synchronization Failed"),
            FallenBehind(s) => write!(f, "Fallen behind main chain - {}", s),
            NetworkSilence => write!(f, "Network Silence"),
            Continue => write!(f, "Continuing"),
            FatalError(e) => write!(f, "Fatal Error - {}", e),
            UserQuit => write!(f, "User Termination"),
        }
    }
}

impl Display for BaseNodeState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use BaseNodeState::*;
        let s = match self {
            Starting(_) => "Initializing",
            HeaderSync(_) => "Synchronizing block headers",
            DecideNextSync(_) => "Deciding next sync",
            HorizonStateSync(_) => "Synchronizing horizon state",
            BlockSync(_) => "Synchronizing blocks",
            Listening(_) => "Listening",
            Shutdown(_) => "Shutting down",
            Waiting(_) => "Waiting",
        };
        f.write_str(s)
    }
}

/// This enum will display all info inside of the state engine
#[derive(Debug, Clone, PartialEq)]
pub enum StateInfo {
    StartUp,
    HeaderSync(Option<BlockSyncInfo>),
    HorizonSync(HorizonSyncInfo),
    BlockSyncStarting,
    BlockSync(BlockSyncInfo),
    Listening(ListeningInfo),
}

impl StateInfo {
    pub fn short_desc(&self) -> String {
        use StateInfo::*;
        match self {
            StartUp => "Starting up".to_string(),
            HeaderSync(None) => "Starting header sync".to_string(),
            HeaderSync(Some(info)) => format!("Syncing headers: {}", info.sync_progress_string()),
            HorizonSync(info) => info.to_progress_string(),

            BlockSync(info) => format!("Syncing blocks: {}", info.sync_progress_string()),
            Listening(_) => "Listening".to_string(),
            BlockSyncStarting => "Starting block sync".to_string(),
        }
    }

    pub fn get_block_sync_info(&self) -> Option<BlockSyncInfo> {
        match self {
            Self::BlockSync(info) => Some(info.clone()),
            _ => None,
        }
    }

    pub fn is_synced(&self) -> bool {
        use StateInfo::*;
        match self {
            StartUp | HeaderSync(_) | HorizonSync(_) | BlockSync(_) | BlockSyncStarting => false,
            Listening(info) => info.is_synced(),
        }
    }
}

impl Display for StateInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use StateInfo::*;
        match self {
            StartUp => write!(f, "Node starting up"),
            HeaderSync(Some(info)) => write!(f, "Synchronizing block headers: {}", info),
            HeaderSync(None) => write!(f, "Synchronizing block headers: Starting"),
            HorizonSync(info) => write!(f, "Synchronizing horizon state: {}", info),
            BlockSync(info) => write!(f, "Synchronizing blocks: {}", info),
            Listening(info) => write!(f, "Listening: {}", info),
            BlockSyncStarting => write!(f, "Synchronizing blocks: Starting"),
        }
    }
}

/// This struct contains global state machine state and the info specific to the current State
#[derive(Debug, Clone, PartialEq)]
pub struct StatusInfo {
    pub bootstrapped: bool,
    pub state_info: StateInfo,
    pub randomx_vm_cnt: usize,
    pub randomx_vm_flags: RandomXFlag,
}

impl StatusInfo {
    pub fn new() -> Self {
        Self {
            bootstrapped: false,
            state_info: StateInfo::StartUp,
            randomx_vm_cnt: 0,
            randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
        }
    }
}

impl Default for StatusInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for StatusInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "Bootstrapped: {}, {}", self.bootstrapped, self.state_info)
    }
}

/// This struct contains info that is use full for external viewing of state info
#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncInfo {
    pub tip_height: u64,
    pub local_height: u64,
    pub sync_peer: SyncPeer,
}

impl BlockSyncInfo {
    /// Creates a new BlockSyncInfo
    pub fn new(tip_height: u64, local_height: u64, sync_peer: SyncPeer) -> Self {
        Self {
            tip_height,
            local_height,
            sync_peer,
        }
    }

    pub fn sync_progress_string(&self) -> String {
        format!(
            "({}) {}/{} ({:.0}%){} Latency: {:.2?}",
            self.sync_peer.node_id().short_str(),
            self.local_height,
            self.tip_height,
            (self.local_height as f64 / self.tip_height as f64 * 100.0),
            self.sync_peer
                .items_per_second()
                .map(|bps| format!(" {:.2?} blks/s", bps))
                .unwrap_or_default(),
            self.sync_peer.latency().unwrap_or_default()
        )
    }
}

impl Display for BlockSyncInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        writeln!(f, "Syncing {}", self.sync_progress_string())
    }
}
