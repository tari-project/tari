//  Copyright 2026, The Tari Project
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

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, NodeIdentity, PeerFeatures},
    protocol::rpc::{NamedProtocolService, Request, Response, RpcStatus, Streaming, mock::MockRpcServer},
    test_utils::{mocks::ConnectivityManagerMockState, node_identity::build_node_identity},
};
use tari_node_components::blocks::{Block, ChainBlock};
use tokio::sync::mpsc;

use crate::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        sync::{
            SyncPeer,
            rpc::{BaseNodeSyncRpcServer, BaseNodeSyncService},
        },
    },
    chain_storage::BlockchainDatabase,
    proto,
    proto::base_node::{
        FindChainSplitRequest,
        FindChainSplitResponse,
        SyncBlocksRequest,
        SyncHeadersRequest,
        SyncKernelsRequest,
        SyncUtxosRequest,
        SyncUtxosResponse,
    },
    test_helpers::blockchain::TempDatabase,
};

/// A sync peer that answers `find_chain_split` and `sync_blocks`.
pub(crate) struct MockSyncPeer {
    pub(crate) local_db: BlockchainDatabase<TempDatabase>,
    pub(crate) reply_delay: Duration,
    /// Added to the local chain while the peer is answering, as if they arrived by propagation
    pub(crate) blocks_to_add: Vec<Arc<ChainBlock>>,
    /// `None` answers NotFound
    pub(crate) reply: Option<FindChainSplitResponse>,
    pub(crate) saw_add_block_disabled: Arc<AtomicBool>,
    /// Sent in answer to `sync_blocks`
    pub(crate) blocks_to_serve: Vec<Block>,
    /// How long the `sync_blocks` stream stays open after the served blocks
    pub(crate) sync_blocks_stall: Duration,
}

impl MockSyncPeer {
    pub(crate) fn new(local_db: &BlockchainDatabase<TempDatabase>, reply: Option<FindChainSplitResponse>) -> Self {
        Self {
            local_db: local_db.clone(),
            reply_delay: Duration::ZERO,
            blocks_to_add: vec![],
            reply,
            saw_add_block_disabled: Arc::new(AtomicBool::new(false)),
            blocks_to_serve: vec![],
            sync_blocks_stall: Duration::ZERO,
        }
    }
}

#[tari_comms::async_trait]
impl BaseNodeSyncService for MockSyncPeer {
    async fn sync_blocks(
        &self,
        _: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::base_node::BlockBodyResponse>, RpcStatus> {
        let (tx, rx) = mpsc::channel(self.blocks_to_serve.len().saturating_add(1));
        for block in &self.blocks_to_serve {
            let response = proto::base_node::BlockBodyResponse::try_from(block.clone()).unwrap();
            tx.send(Ok(response)).await.unwrap();
        }
        let stall = self.sync_blocks_stall;
        tokio::spawn(async move {
            tokio::time::sleep(stall).await;
            drop(tx);
        });
        Ok(Streaming::new(rx))
    }

    async fn sync_headers(
        &self,
        _: Request<SyncHeadersRequest>,
    ) -> Result<Streaming<proto::core::BlockHeader>, RpcStatus> {
        Err(RpcStatus::not_implemented("mock"))
    }

    async fn get_header_by_height(&self, _: Request<u64>) -> Result<Response<proto::core::BlockHeader>, RpcStatus> {
        Err(RpcStatus::not_implemented("mock"))
    }

    async fn find_chain_split(
        &self,
        _: Request<FindChainSplitRequest>,
    ) -> Result<Response<FindChainSplitResponse>, RpcStatus> {
        if self.local_db.is_add_block_disabled() {
            self.saw_add_block_disabled.store(true, Ordering::SeqCst);
        }
        tokio::time::sleep(self.reply_delay).await;
        for block in &self.blocks_to_add {
            self.local_db.add_block(block.to_arc_block()).unwrap();
        }
        match &self.reply {
            Some(reply) => Ok(Response::new(reply.clone())),
            None => Err(RpcStatus::not_found("no chain split")),
        }
    }

    async fn get_chain_metadata(&self, _: Request<()>) -> Result<Response<proto::base_node::ChainMetadata>, RpcStatus> {
        Err(RpcStatus::not_implemented("mock"))
    }

    async fn sync_kernels(
        &self,
        _: Request<SyncKernelsRequest>,
    ) -> Result<Streaming<proto::types::TransactionKernel>, RpcStatus> {
        Err(RpcStatus::not_implemented("mock"))
    }

    async fn sync_utxos(&self, _: Request<SyncUtxosRequest>) -> Result<Streaming<SyncUtxosResponse>, RpcStatus> {
        Err(RpcStatus::not_implemented("mock"))
    }
}

/// Serves `service` as a sync peer claiming `claim`. The returned server must be kept alive for the test.
pub(crate) async fn connect_mock_peer(
    connectivity_mock: &ConnectivityManagerMockState,
    service: MockSyncPeer,
    claim: ChainMetadata,
) -> (SyncPeer, MockRpcServer<BaseNodeSyncRpcServer<MockSyncPeer>>) {
    let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    connect_mock_peer_as(peer_identity, connectivity_mock, service, claim).await
}

/// As [`connect_mock_peer`], for a given peer identity
pub(crate) async fn connect_mock_peer_as(
    peer_identity: Arc<NodeIdentity>,
    connectivity_mock: &ConnectivityManagerMockState,
    service: MockSyncPeer,
    claim: ChainMetadata,
) -> (SyncPeer, MockRpcServer<BaseNodeSyncRpcServer<MockSyncPeer>>) {
    let server = BaseNodeSyncRpcServer::new(service);
    let protocol_name = server.as_protocol_name();
    let mut mock_server = MockRpcServer::new(server, peer_identity.clone());
    mock_server.serve();
    let conn = mock_server
        .create_connection(peer_identity.to_peer(), protocol_name.into())
        .await;
    connectivity_mock.add_active_connection(conn).await;
    let sync_peer = SyncPeer::from(PeerChainMetadata::new(peer_identity.node_id().clone(), claim, None));
    (sync_peer, mock_server)
}

/// The bans sent so far. Bans are fire-and-forget, so a request with a reply is sent first to flush them.
pub(crate) async fn banned_peers(
    mut connectivity: ConnectivityRequester,
    connectivity_mock: &ConnectivityManagerMockState,
) -> Vec<(NodeId, Duration, String)> {
    connectivity.get_connectivity_status().await.unwrap();
    connectivity_mock.take_banned_peers().await
}
