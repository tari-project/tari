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

use crate::{
    base_node::{
        node_state::{StateEvent, StateEvent::FatalError},
        starting_state::Starting,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase},
};
use log::*;

const LOG_TARGET: &str = "base_node::initial_sync";

pub struct InitialSync<B>
where B: BlockchainBackend
{
    pub(crate) db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> InitialSync<B> {
    pub fn next_event(&mut self) -> StateEvent {
        info!(target: LOG_TARGET, "Starting blockchain metadata sync");
        self.sync_metadata()
    }

    /// Fetch the blockchain metadata from our internal database and compare it to data received from peers to decide
    /// on the next phase of the blockchain synchronisation.
    fn sync_metadata(&self) -> StateEvent {
        info!(target: LOG_TARGET, "Loading local blockchain metadata.");
        let metadata = match self.db.get_metadata() {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("Could not get local blockchain metadata. {}", e.to_string());
                return FatalError(msg);
            },
        };
        info!(
            target: LOG_TARGET,
            "Current local blockchain database information:\n {}", metadata
        );
        // TODO async fetch peer metadata

        FatalError("Unimplemented".into())
    }
}

impl<B: BlockchainBackend> From<Starting<B>> for InitialSync<B> {
    fn from(old_state: Starting<B>) -> Self {
        InitialSync { db: old_state.db }
    }
}
