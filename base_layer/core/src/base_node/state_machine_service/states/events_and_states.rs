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

use crate::base_node::{
    state_machine_service::states::{
        BlockSync,
        HeaderSync,
        HorizonStateSync,
        Listening,
        ListeningInfo,
        Shutdown,
        Starting,
        Waiting,
    },
    sync::SyncPeers,
};
use std::fmt::{Display, Error, Formatter};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{peer_manager::NodeId, PeerConnection};

#[derive(Debug)]
pub enum BaseNodeState {
    Starting(Starting),
    HeaderSync(HeaderSync),
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
    InitialSync,
    HeadersSynchronized(PeerConnection),
    HeaderSyncFailed,
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
    Lagging(ChainMetadata, SyncPeers),
    // We are behind the pruning horizon.
    LaggingBehindHorizon(ChainMetadata, SyncPeers),
    UpToDate,
}

impl SyncStatus {
    pub fn is_lagging(&self) -> bool {
        match self {
            SyncStatus::Lagging(_, _) | SyncStatus::LaggingBehindHorizon(_, _) => true,
            SyncStatus::UpToDate => false,
        }
    }
}

impl Display for SyncStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use SyncStatus::*;
        match self {
            Lagging(m, v) => write!(
                f,
                "Lagging behind {} peers (#{}, Difficulty: {})",
                v.len(),
                m.height_of_longest_chain(),
                m.accumulated_difficulty(),
            ),
            LaggingBehindHorizon(m, v) => write!(
                f,
                "Lagging behind pruning horizon ({} peer(s), Network height: #{}, Difficulty: {})",
                v.len(),
                m.height_of_longest_chain(),
                m.accumulated_difficulty(),
            ),
            UpToDate => f.write_str("UpToDate"),
        }
    }
}

impl Display for StateEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use StateEvent::*;
        match self {
            Initialized => f.write_str("Initialized"),
            InitialSync => f.write_str("InitialSync"),
            BlocksSynchronized => f.write_str("Synchronised Blocks"),
            HeadersSynchronized(conn) => write!(f, "Headers Synchronized from peer `{}`", conn.peer_node_id()),
            HeaderSyncFailed => f.write_str("Header Synchronization Failed"),
            HorizonStateSynchronized => f.write_str("Horizon State Synchronized"),
            HorizonStateSyncFailure => f.write_str("Horizon State Synchronization Failed"),
            BlockSyncFailed => f.write_str("Block Synchronization Failed"),
            FallenBehind(s) => write!(f, "Fallen behind main chain - {}", s),
            NetworkSilence => f.write_str("Network Silence"),
            Continue => f.write_str("Continuing"),
            FatalError(e) => write!(f, "Fatal Error - {}", e),
            UserQuit => f.write_str("User Termination"),
        }
    }
}

impl Display for BaseNodeState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use BaseNodeState::*;
        let s = match self {
            Starting(_) => "Initializing",
            HeaderSync(_) => "Synchronizing block headers",
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
    HeaderSync(BlockSyncInfo),
    HorizonSync(BlockSyncInfo),
    BlockSync(BlockSyncInfo),
    Listening(ListeningInfo),
}

impl StateInfo {
    pub fn short_desc(&self) -> String {
        match self {
            Self::StartUp => "Starting up".to_string(),
            Self::HeaderSync(info) => format!(
                "Syncing headers:{}/{} ({:.0}%)",
                info.local_height,
                info.tip_height,
                info.local_height as f64 / info.tip_height as f64 * 100.0
            ),
            Self::HorizonSync(_) => "Syncing to horizon".to_string(),
            Self::BlockSync(info) => format!(
                "Syncing blocks:{}/{} ({:.0}%)",
                info.local_height,
                info.tip_height,
                info.local_height as f64 / info.tip_height as f64 * 100.0
            ),
            Self::Listening(_) => "Listening".to_string(),
        }
    }
}

impl Display for StateInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Self::StartUp => write!(f, "Node starting up"),
            Self::HeaderSync(info) => write!(f, "Synchronizing block headers: {}", info),
            Self::HorizonSync(info) => write!(f, "Synchronizing horizon state: {}", info),
            Self::BlockSync(info) => write!(f, "Synchronizing blocks: {}", info),
            Self::Listening(info) => write!(f, "Listening: {}", info),
        }
    }
}

/// This struct contains global state machine state and the info specific to the current State
#[derive(Debug, Clone, PartialEq)]
pub struct StatusInfo {
    pub bootstrapped: bool,
    pub state_info: StateInfo,
}

impl StatusInfo {
    pub fn new() -> Self {
        Self {
            bootstrapped: false,
            state_info: StateInfo::StartUp,
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

#[derive(Clone, Debug, PartialEq)]
/// This struct contains info that is use full for external viewing of state info
pub struct BlockSyncInfo {
    pub tip_height: u64,
    pub local_height: u64,
    pub sync_peers: Vec<NodeId>,
}

impl BlockSyncInfo {
    /// Creates a new blockSyncInfo
    pub fn new(tip_height: u64, local_height: u64, sync_peers: Vec<NodeId>) -> BlockSyncInfo {
        BlockSyncInfo {
            tip_height,
            local_height,
            sync_peers,
        }
    }
}

impl Display for BlockSyncInfo {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("Syncing from the following peers: \n")?;
        for peer in &self.sync_peers {
            fmt.write_str(&format!("{}\n", peer))?;
        }
        fmt.write_str(&format!("Syncing {}/{}\n", self.local_height, self.tip_height))
    }
}
