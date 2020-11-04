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

use blake2::Digest;
use std::sync::Arc;
use tari_comms::peer_manager::NodeId;
use tari_core::{
    base_node::chain_metadata_service::{ChainMetadataEvent, ChainMetadataHandle, PeerChainMetadata},
    chain_storage::ChainMetadata,
};
use tari_crypto::{common::Blake256, tari_utilities::ByteArray};
use tokio::sync::broadcast;

/// Create a mock Chain Metadata stream.
///
/// This struct simulates the chain metadata input stream the base node uses to keep tabs on the blockchain progress
/// in the rest of the network.
pub struct MockChainMetadata {
    publisher: broadcast::Sender<Arc<ChainMetadataEvent>>,
}

impl MockChainMetadata {
    pub fn new() -> Self {
        let (publisher, _) = broadcast::channel(10);
        Self { publisher }
    }

    pub fn chain_metadata_handle(&self) -> ChainMetadataHandle {
        ChainMetadataHandle::new(self.publisher.clone())
    }

    pub fn subscription(&self) -> broadcast::Receiver<Arc<ChainMetadataEvent>> {
        self.publisher.subscribe()
    }

    pub fn publish_event(&mut self, event: ChainMetadataEvent) -> Result<usize, Arc<ChainMetadataEvent>> {
        self.publisher.send(Arc::new(event)).map_err(|err| err.0)
    }

    pub async fn publish_chain_metadata(
        &mut self,
        id: &NodeId,
        metadata: &ChainMetadata,
    ) -> Result<usize, Arc<ChainMetadataEvent>>
    {
        let data = PeerChainMetadata::new(id.clone(), metadata.clone());
        self.publish_event(ChainMetadataEvent::PeerChainMetadataReceived(vec![data]))
    }
}

pub fn random_peer_metadata(height: u64, difficulty: u128) -> PeerChainMetadata {
    let key: Vec<u8> = (0..13).map(|_| rand::random::<u8>()).collect();
    let id = NodeId::from_key(&key).unwrap();
    let block_hash = Blake256::digest(id.as_bytes()).to_vec();
    let metadata = ChainMetadata::new(height, block_hash, 2800, 0, difficulty);
    PeerChainMetadata::new(id, metadata)
}
