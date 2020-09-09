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

use crate::{
    base_node::state_machine_service::states::{
        BlockSyncInfo,
        BlockSyncStrategy,
        HeaderSync,
        HorizonStateSync,
        Listening,
        ListeningInfo,
        Shutdown,
        Starting,
        SyncPeers,
        Waiting,
    },
    chain_storage::ChainMetadata,
    proof_of_work::Difficulty,
};
use std::fmt::{Display, Error, Formatter};

#[derive(Clone, Debug, PartialEq)]
pub enum BaseNodeState {
    Starting(Starting),
    HeaderSync(HeaderSync),
    HorizonStateSync(HorizonStateSync),
    BlockSync(BlockSyncStrategy, ChainMetadata, SyncPeers),
    // The best network chain metadata
    Listening(Listening),
    // We're in a paused state, and will return to Listening after a timeout
    Waiting(Waiting),
    Shutdown(Shutdown),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateEvent {
    Initialized,
    MetadataSynced(SyncStatus),
    HeadersSynchronized(ChainMetadata, u64),
    HeaderSyncFailure,
    HorizonStateSynchronized,
    HorizonStateSyncFailure,
    BlocksSynchronized,
    BlockSyncFailure,
    FallenBehind(SyncStatus),
    NetworkSilence,
    FatalError(String),
    Continue,
    UserQuit,
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
                m.accumulated_difficulty.unwrap_or_else(Difficulty::min),
            ),
            LaggingBehindHorizon(m, v) => write!(
                f,
                "Lagging behind pruning horizon ({} peer(s), Network height: #{}, Difficulty: {})",
                v.len(),
                m.height_of_longest_chain(),
                m.accumulated_difficulty.unwrap_or_else(Difficulty::min),
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
            MetadataSynced(s) => write!(f, "Synchronized metadata - {}", s),
            BlocksSynchronized => f.write_str("Synchronised Blocks"),
            HeadersSynchronized(_, h) => write!(f, "Headers Synchronized to height {}", h),
            HeaderSyncFailure => f.write_str("Header Synchronization Failure"),
            HorizonStateSynchronized => f.write_str("Horizon State Synchronized"),
            HorizonStateSyncFailure => f.write_str("Horizon State Synchronization Failure"),
            BlockSyncFailure => f.write_str("Block Synchronization Failure"),
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
            BlockSync(_, _, _) => "Synchronizing blocks",
            Listening(_) => "Listening",
            Shutdown(_) => "Shutting down",
            Waiting(_) => "Waiting",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq)]
/// This enum will display all info inside of the state engine
pub enum StatusInfo {
    StartUp,
    HeaderSync(BlockSyncInfo),
    HorizonSync(BlockSyncInfo),
    BlockSync(BlockSyncInfo),
    Listening(ListeningInfo),
}

impl Display for StatusInfo {
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
