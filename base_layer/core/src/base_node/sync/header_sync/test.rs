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
    time::{Duration, Instant},
};

use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, PeerFeatures},
    protocol::rpc::{NamedProtocolService, Request, Response, RpcStatus, Streaming, mock::MockRpcServer},
    test_utils::{
        mocks::{ConnectivityManagerMockState, create_connectivity_mock},
        node_identity::build_node_identity,
    },
};
use tari_node_components::blocks::{BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader};
use tari_transaction_components::tari_proof_of_work::Difficulty;

use crate::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        sync::{
            AttemptSyncResult,
            BlockHeaderSyncError,
            BlockchainSyncConfig,
            HeaderSyncStatus,
            HeaderSynchronizer,
            SyncPeer,
            rpc::{BaseNodeSyncRpcServer, BaseNodeSyncService},
        },
    },
    chain_storage::BlockchainDatabase,
    proof_of_work::randomx_factory::RandomXFactory,
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
    test_helpers::blockchain::{TempDatabase, create_chained_blocks, create_new_blockchain},
};

/// A sync peer that only answers `find_chain_split`.
struct MockSyncPeer {
    local_db: BlockchainDatabase<TempDatabase>,
    reply_delay: Duration,
    /// Added to the local chain while the peer is answering, as if it arrived by propagation
    block_to_add: Option<Arc<ChainBlock>>,
    /// `None` answers NotFound
    reply: Option<FindChainSplitResponse>,
    saw_add_block_disabled: Arc<AtomicBool>,
}

impl MockSyncPeer {
    fn new(local_db: &BlockchainDatabase<TempDatabase>, reply: Option<FindChainSplitResponse>) -> Self {
        Self {
            local_db: local_db.clone(),
            reply_delay: Duration::ZERO,
            block_to_add: None,
            reply,
            saw_add_block_disabled: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[tari_comms::async_trait]
impl BaseNodeSyncService for MockSyncPeer {
    async fn sync_blocks(
        &self,
        _: Request<SyncBlocksRequest>,
    ) -> Result<Streaming<proto::base_node::BlockBodyResponse>, RpcStatus> {
        Err(RpcStatus::not_implemented("mock"))
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
        if let Some(block) = &self.block_to_add {
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
async fn connect_mock_peer(
    connectivity_mock: &ConnectivityManagerMockState,
    service: MockSyncPeer,
    claim: ChainMetadata,
) -> (SyncPeer, MockRpcServer<BaseNodeSyncRpcServer<MockSyncPeer>>) {
    let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
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

async fn sync_headers(
    db: &BlockchainDatabase<TempDatabase>,
    connectivity: ConnectivityRequester,
    config: BlockchainSyncConfig,
    mut sync_peers: Vec<SyncPeer>,
) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
    let local_metadata = db.get_chain_metadata().unwrap();
    let mut synchronizer = HeaderSynchronizer::new(
        config,
        db.clone().into(),
        db.rules().clone(),
        connectivity,
        &mut sync_peers,
        RandomXFactory::default(),
        &local_metadata,
    );
    synchronizer.synchronize().await
}

/// The bans sent so far. Bans are fire-and-forget, so a request with a reply is sent first to flush them.
async fn banned_peers(
    mut connectivity: ConnectivityRequester,
    connectivity_mock: &ConnectivityManagerMockState,
) -> Vec<(NodeId, Duration, String)> {
    connectivity.get_connectivity_status().await.unwrap();
    connectivity_mock.take_banned_peers().await
}

/// A valid block on top of the genesis block that is not added to `db`.
fn block_on_genesis(db: &BlockchainDatabase<TempDatabase>) -> Arc<ChainBlock> {
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, mut chain) = create_chained_blocks(db, block_specs!(["A->GB"]), genesis);
    chain.remove("A").unwrap()
}

fn claim_for(block: &ChainBlock) -> ChainMetadata {
    ChainMetadata::new(
        block.height(),
        *block.hash(),
        0,
        0,
        block.accumulated_data().total_accumulated_difficulty,
        block.header().timestamp.as_u64(),
    )
    .unwrap()
}

#[tokio::test]
async fn switching_to_a_stronger_batch_disables_add_block() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let block = block_on_genesis(&db);
    let reply = FindChainSplitResponse {
        headers: vec![block.header().clone().into()],
        fork_hash_index: 0,
    };
    let service = MockSyncPeer::new(&db, Some(reply));
    let saw_add_block_disabled = service.saw_add_block_disabled.clone();
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&block)).await;

    let (_, result) = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap();

    assert!(matches!(result.header_sync_status, HeaderSyncStatus::Lagging(_)));
    // Not while finding the chain split, only once the peer's headers are accepted
    assert!(!saw_add_block_disabled.load(Ordering::SeqCst));
    assert!(db.is_add_block_disabled());
    assert_eq!(db.fetch_last_header().unwrap().hash(), *block.hash());
}

#[tokio::test]
async fn a_stronger_local_block_that_landed_during_sync_is_kept() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, mut chain) = create_chained_blocks(
        &db,
        block_specs!(["peer->GB"], ["local->GB", difficulty: Difficulty::from_u64(10).unwrap()]),
        genesis,
    );
    let peer_block = chain.remove("peer").unwrap();
    let local_block = chain.remove("local").unwrap();
    let reply = FindChainSplitResponse {
        headers: vec![peer_block.header().clone().into()],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(reply));
    service.block_to_add = Some(local_block.clone());
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&peer_block)).await;

    let err = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert!(!db.is_add_block_disabled());
    assert_eq!(db.get_chain_metadata().unwrap().best_block_hash(), local_block.hash());
    assert_eq!(db.fetch_last_header().unwrap().hash(), *local_block.hash());
}

#[tokio::test]
async fn an_attempt_that_runs_out_of_time_is_banned_and_the_next_peer_is_tried() {
    let db = create_new_blockchain();
    // Enough headers that a peer answering NotFound keeps us walking back for several rounds
    let mut tip = db.fetch_tip_header().unwrap();
    let mut headers = Vec::new();
    for _ in 0..2500 {
        let mut header = BlockHeader::from_previous(tip.header());
        header.kernel_mmr_size = header.kernel_mmr_size.saturating_add(1);
        header.output_smt_size = header.output_smt_size.saturating_add(1);
        let acc_data = BlockHeaderAccumulatedData::genesis(header.hash(), header.total_kernel_offset.clone());
        tip = ChainHeader::try_construct(header, acc_data).unwrap();
        headers.push(tip.clone());
    }
    db.insert_valid_headers(headers).unwrap();

    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let claim = ChainMetadata::new(3000, [1u8; 32].into(), 0, 0, 1_000_000_000u64.into(), 0).unwrap();
    let mut slow = MockSyncPeer::new(&db, None);
    slow.reply_delay = Duration::from_millis(600);
    let saw_add_block_disabled = slow.saw_add_block_disabled.clone();
    let (slow_peer, _slow_server) = connect_mock_peer(&connectivity_mock, slow, claim.clone()).await;
    let in_sync = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let (next_peer, _next_server) =
        connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(in_sync)), claim).await;

    let config = BlockchainSyncConfig {
        rpc_deadline: Duration::from_secs(1),
        ..Default::default()
    };
    let short_ban_period = config.short_ban_period;
    let timer = Instant::now();
    let (synced_from, _) = sync_headers(&db, connectivity.clone(), config, vec![
        slow_peer.clone(),
        next_peer.clone(),
    ])
    .await
    .unwrap();
    let elapsed = timer.elapsed();

    // The attempt budget is 2 x rpc_deadline
    assert!(elapsed >= Duration::from_secs(2), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(4), "{elapsed:?}");
    assert_eq!(synced_from.node_id(), next_peer.node_id());
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, slow_peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("did not complete within"), "{ban_reason}");
    // NotFound rounds never prove a stronger chain
    assert!(!saw_add_block_disabled.load(Ordering::SeqCst));
    assert!(!db.is_add_block_disabled());
}

#[tokio::test]
async fn no_headers_for_a_block_that_landed_during_sync_is_not_banned() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let block = block_on_genesis(&db);
    let no_headers = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(no_headers));
    service.block_to_add = Some(block.clone());
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&block)).await;

    let err = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(db.get_height().unwrap(), 1);
    assert!(!db.is_add_block_disabled());
}

#[tokio::test]
async fn a_header_that_landed_during_sync_is_not_banned() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let block = block_on_genesis(&db);
    let reply = FindChainSplitResponse {
        headers: vec![block.header().clone().into()],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(reply));
    service.block_to_add = Some(block.clone());
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&block)).await;

    let err = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(db.get_height().unwrap(), 1);
    assert!(!db.is_add_block_disabled());
}
