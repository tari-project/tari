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

use std::sync::{atomic::AtomicBool, Arc};
use tari_common::{DatabaseType, NodeBuilderConfig};
use tari_core::{
    base_node::{BaseNodeStateMachine, OutboundNodeCommsInterface},
    chain_storage::{create_lmdb_database, BlockchainDatabase, LMDBDatabase, MemoryDatabase},
    types::HashDigest,
};
use tari_service_framework::reply_channel;

pub enum NodeType {
    LMDB(BaseNodeStateMachine<LMDBDatabase<HashDigest>>),
    Memory(BaseNodeStateMachine<MemoryDatabase<HashDigest>>),
}

impl NodeType {
    pub fn get_flag(&self) -> Arc<AtomicBool> {
        match self {
            NodeType::LMDB(n) => n.get_interrupt_flag(),
            NodeType::Memory(n) => n.get_interrupt_flag(),
        }
    }

    pub async fn run(self) {
        async move {
            match self {
                NodeType::LMDB(n) => n.run().await,
                NodeType::Memory(n) => n.run().await,
            }
        }
            .await;
    }
}

pub fn compose_node(builder: &NodeBuilderConfig) -> Result<NodeType, String> {
    let (sender, _receiver) = reply_channel::unbounded();
    let comms = OutboundNodeCommsInterface::new(sender);
    let node = match &builder.db_type {
        DatabaseType::Memory => {
            let backend = MemoryDatabase::<HashDigest>::default();
            let db = BlockchainDatabase::new(backend).map_err(|e| e.to_string())?;
            NodeType::Memory(BaseNodeStateMachine::new(&db, &comms))
        },
        DatabaseType::LMDB(p) => {
            let backend = create_lmdb_database(&p).map_err(|e| e.to_string())?;
            let db = BlockchainDatabase::new(backend).map_err(|e| e.to_string())?;
            NodeType::LMDB(BaseNodeStateMachine::new(&db, &comms))
        },
    };
    Ok(node)
}
