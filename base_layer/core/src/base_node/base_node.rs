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
    base_node::node_state::BaseNodeState,
    chain_storage::{BlockchainBackend, BlockchainDatabase},
};
use log::*;

const TARGET: &str = "core::base_node";

/// A Tari full node, aka Base Node.
///
/// The Base Node is essentially a finite state machine that synchronises its blockchain state with its peers and
/// then listens for new blocks to add to the blockchain
///
/// ## Initial Synchronisation
/// First there's the initial sync from the horizon point. This has multiple steps
/// 1. All block headers to the horizon block are downloaded and validated.
/// 2. All kernels to the horizon are downloaded and validated.
/// 3. Download the UTXO set at the pruning horizon.
///
/// ## Block synchronisation
///
/// Then there's the sequential block sync. Essentially a series of "NewBlock" commands starting from the horizon
/// block + 1 to the current chain tip. This process is identical to the normal block validation process. You'll
/// receive an entire block from a peer and then try to add to the `BlockchainDB` instance.
///
/// See the [SynchronizationSate] documentation for more details.
pub struct BaseNode<B>
where B: BlockchainBackend
{
    state: BaseNodeState,
    db: BlockchainDatabase<B>,
}

impl<B> BaseNode<B>
where B: BlockchainBackend
{
    /// Instantiate a new Base Node.
    ///
    /// ```
    /// # use tari_core::chain_storage::{BlockchainDatabase, BlockchainBackend, MemoryDatabase};
    /// # use tari_core::types::HashDigest;
    /// # use tari_core::BaseNode;
    /// // Create and configure database backend.
    /// let db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    /// // Cloning a BlockchainDatabase is a light copy, so cloning it is the preferred way to pass in the instance:
    /// let base_node = BaseNode::new(db.clone());
    /// base_node.start();
    /// ```
    pub fn new(db: BlockchainDatabase<B>) -> Self {
        BaseNode {
            state: BaseNodeState::Startup,
            db,
        }
    }

    /// Start the base node runtime.
    pub fn start(&self) -> bool {
        match self.state {
            BaseNodeState::Startup => {
                info!(target: TARGET, "Starting up base node");
                true
            },
            _ => {
                warn!(target: TARGET, "Node is already running. It can't be started again.");
                false
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        chain_storage::{BlockchainDatabase, MemoryDatabase},
        types::HashDigest,
        BaseNode,
    };

    #[test]
    fn create_node() {
        let db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
        let base_node = BaseNode::new(db);
        assert!(base_node.start());
    }
}
