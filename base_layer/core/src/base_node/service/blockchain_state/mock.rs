//  Copyright 2020, The Tari Project
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

use crate::{
    base_node::service::blockchain_state::{BlockchainStateRequest, BlockchainStateServiceHandle},
    blocks::{Block, BlockHeader},
    chain_storage::ChainMetadata,
};
use futures::{channel::mpsc, StreamExt};
use std::{
    any::Any,
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};
use tokio::{
    sync::{Mutex, RwLock},
    task,
};

pub fn create_blockchain_state_service_mock() -> (BlockchainStateServiceHandle, BlockchainStateMockState) {
    let (tx, rx) = mpsc::channel(1);
    let mock = BlockchainStateServiceMock::new(rx);
    let state = mock.get_shared_state();
    task::spawn(mock.run());
    (BlockchainStateServiceHandle::new(tx), state)
}

#[derive(Debug, Clone)]
pub struct BlockchainStateMockState {
    get_blocks: Arc<Mutex<Vec<Block>>>,
    get_chain_metadata: Arc<Mutex<ChainMetadata>>,
    get_header_by_hash: Arc<Mutex<Option<BlockHeader>>>,
    calls: Arc<RwLock<HashMap<String, VecDeque<Box<dyn Any + Send + Sync>>>>>,
}

impl BlockchainStateMockState {
    pub async fn set_get_blocks_response(&self, blocks: Vec<Block>) {
        *self.get_blocks.lock().await = blocks;
    }

    pub async fn set_get_chain_metadata_response(&self, metadata: ChainMetadata) {
        *self.get_chain_metadata.lock().await = metadata;
    }

    pub async fn set_get_block_header_by_hash(&self, maybe_header: Option<BlockHeader>) {
        *self.get_header_by_hash.lock().await = maybe_header;
    }

    async fn add_call<S: ToString, T: Any + Send + Sync>(&self, name: S, params: T) {
        let mut lock = self.calls.write().await;
        match lock.entry(name.to_string()) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().push_back(Box::new(params));
            },
            Entry::Vacant(entry) => {
                entry.insert({
                    let mut v = VecDeque::<Box<dyn Any + Send + Sync>>::with_capacity(1);
                    v.push_back(Box::new(params));
                    v
                });
            },
        }
    }

    pub async fn get_calls<T: Any + Clone>(&self, name: &str) -> Vec<T> {
        self.calls
            .read()
            .await
            .get(name)
            .into_iter()
            .flatten()
            .map(|v| v.downcast_ref::<T>().unwrap().clone())
            .collect()
    }

    pub async fn pop_front_call<T: Any + Clone>(&self, name: &str) -> Option<T> {
        self.calls
            .write()
            .await
            .get_mut(name)
            .and_then(|calls| calls.pop_front())
            .map(|v| v.downcast_ref::<T>().unwrap().clone())
    }

    pub async fn get_call_count(&self, name: &str) -> usize {
        self.calls.read().await.get(name).map(|v| v.len()).unwrap_or(0)
    }
}

struct BlockchainStateServiceMock {
    receiver: mpsc::Receiver<BlockchainStateRequest>,
    state: BlockchainStateMockState,
}

impl BlockchainStateServiceMock {
    pub fn new(receiver: mpsc::Receiver<BlockchainStateRequest>) -> Self {
        let state = BlockchainStateMockState {
            get_blocks: Default::default(),
            get_chain_metadata: Arc::new(Mutex::new(ChainMetadata::new(0, Vec::new(), 0, 0, 0))),
            get_header_by_hash: Default::default(),
            calls: Default::default(),
        };

        Self { receiver, state }
    }

    pub fn get_shared_state(&self) -> BlockchainStateMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: BlockchainStateRequest) {
        use BlockchainStateRequest::*;

        match req {
            GetBlocks(p, reply) => {
                self.state.add_call("get_blocks", p).await;
                reply.send(Ok(self.state.get_blocks.lock().await.clone())).unwrap();
            },
            GetHeaders(_, _) => unimplemented!(),
            GetHeaderByHeight(_, _) => unimplemented!(),
            GetChainMetadata(reply) => {
                self.state.add_call("get_chain_metadata", ()).await;
                reply
                    .send(Ok(self.state.get_chain_metadata.lock().await.clone()))
                    .unwrap();
            },
            FindHeadersAfterHash(_, _) => unimplemented!(),
            GetHeaderByHash(hash, reply) => {
                self.state.add_call("get_header_by_hash", hash).await;
                reply
                    .send(Ok(self.state.get_header_by_hash.lock().await.clone()))
                    .unwrap();
            },
        }
    }
}
