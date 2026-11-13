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
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, NodeIdentity, PeerFeatures},
    test_utils::{mocks::create_connectivity_mock, node_identity::build_node_identity},
};
use tari_node_components::blocks::{BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader};
use tari_transaction_components::tari_proof_of_work::Difficulty;

use super::synchronizer::MAX_CHAIN_CHANGES_PER_PEER;
use crate::{
    base_node::sync::{
        AttemptSyncResult,
        BlockHeaderSyncError,
        BlockchainSyncConfig,
        HeaderSyncStatus,
        HeaderSynchronizer,
        SyncPeer,
        mock_sync_peer::{MockSyncPeer, banned_peers, connect_mock_peer, connect_mock_peer_as},
    },
    chain_storage::BlockchainDatabase,
    proof_of_work::randomx_factory::RandomXFactory,
    proto::base_node::FindChainSplitResponse,
    test_helpers::blockchain::{TempDatabase, create_chained_blocks, create_main_chain, create_new_blockchain},
};

async fn sync_headers(
    db: &BlockchainDatabase<TempDatabase>,
    connectivity: ConnectivityRequester,
    config: BlockchainSyncConfig,
    sync_peers: Vec<SyncPeer>,
) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
    sync_headers_with_history(db, connectivity, config, sync_peers, &mut HashMap::new()).await
}

async fn sync_headers_with_history(
    db: &BlockchainDatabase<TempDatabase>,
    connectivity: ConnectivityRequester,
    config: BlockchainSyncConfig,
    sync_peers: Vec<SyncPeer>,
    chain_changes: &mut HashMap<NodeId, Vec<Instant>>,
) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError> {
    sync_headers_with_hook(db, connectivity, config, sync_peers, chain_changes, |_| {}).await
}

async fn sync_headers_with_hook<H>(
    db: &BlockchainDatabase<TempDatabase>,
    connectivity: ConnectivityRequester,
    config: BlockchainSyncConfig,
    mut sync_peers: Vec<SyncPeer>,
    chain_changes: &mut HashMap<NodeId, Vec<Instant>>,
    on_starting: H,
) -> Result<(SyncPeer, AttemptSyncResult), BlockHeaderSyncError>
where
    H: FnOnce(&SyncPeer) + Send + Sync + 'static,
{
    let local_metadata = db.get_chain_metadata().unwrap();
    let mut synchronizer = HeaderSynchronizer::new(
        config,
        db.clone().into(),
        db.rules().clone(),
        connectivity,
        &mut sync_peers,
        RandomXFactory::default(),
        local_metadata,
        chain_changes,
    );
    synchronizer.on_starting(on_starting);
    synchronizer.synchronize().await
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
    service.blocks_to_add = vec![local_block.clone()];
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
        let mut acc_data = BlockHeaderAccumulatedData::genesis(header.hash(), header.total_kernel_offset.clone());
        // Stronger than our blocks, so a claim of this tip is ahead of us
        acc_data.total_accumulated_difficulty = tip.accumulated_data().total_accumulated_difficulty + 1;
        tip = ChainHeader::try_construct(header, acc_data).unwrap();
        headers.push(tip.clone());
    }
    db.insert_valid_headers(headers).unwrap();

    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // Covered by our header chain, so the peer answering with no headers is in sync
    let claim = ChainMetadata::new(
        tip.height(),
        *tip.hash(),
        0,
        0,
        tip.accumulated_data().total_accumulated_difficulty,
        0,
    )
    .unwrap();
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
    service.blocks_to_add = vec![block.clone()];
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
    service.blocks_to_add = vec![block.clone()];
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
async fn a_weaker_local_block_that_landed_during_sync_is_rewound() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, mut chain) = create_chained_blocks(&db, block_specs!(["P1->GB"], ["P2->P1"], ["L->GB"]), genesis.clone());
    let peer_block_1 = chain.remove("P1").unwrap();
    let peer_block_2 = chain.remove("P2").unwrap();
    let local_block = chain.remove("L").unwrap();
    let reply = FindChainSplitResponse {
        headers: vec![
            peer_block_1.header().clone().into(),
            peer_block_2.header().clone().into(),
        ],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(reply));
    service.blocks_to_add = vec![local_block.clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&peer_block_2)).await;

    let (_, result) = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap();

    assert!(matches!(result.header_sync_status, HeaderSyncStatus::Lagging(_)));
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert!(db.is_add_block_disabled());
    assert_eq!(db.fetch_last_header().unwrap().hash(), *peer_block_2.hash());
    assert_eq!(db.get_chain_metadata().unwrap().best_block_hash(), genesis.hash());
    assert!(db.fetch_header_by_block_hash(*local_block.hash()).unwrap().is_none());
}

/// A local chain of genesis and one block, and a claim that is stronger than it naming that block as the tip.
fn chain_with_one_block_and_an_inflated_claim() -> (BlockchainDatabase<TempDatabase>, Arc<ChainBlock>, ChainMetadata) {
    let db = create_new_blockchain();
    let (_, mut chain) = create_main_chain(&db, block_specs!(["A->GB"]));
    let tip = chain.remove("A").unwrap();
    let tip_metadata = claim_for(&tip);
    let claim = ChainMetadata::new(
        tip_metadata.best_block_height().saturating_add(5),
        *tip_metadata.best_block_hash(),
        0,
        0,
        tip_metadata.accumulated_difficulty().saturating_mul(100.into()),
        tip_metadata.timestamp(),
    )
    .unwrap();
    (db, tip, claim)
}

#[tokio::test]
async fn no_headers_for_an_inflated_claim_naming_our_tip_is_banned() {
    let (db, _tip, claim) = chain_with_one_block_and_an_inflated_claim();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let no_headers = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let (peer, _server) = connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(no_headers)), claim).await;
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;

    let err = sync_headers(&db, connectivity.clone(), config, vec![peer.clone()])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("claimed a stronger chain"), "{ban_reason}");
}

#[tokio::test]
async fn our_own_tip_header_sent_back_is_banned() {
    let (db, tip, claim) = chain_with_one_block_and_an_inflated_claim();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // Index 1 of the hashes we send (tip, genesis) is genesis, the parent of our tip
    let our_tip = FindChainSplitResponse {
        headers: vec![tip.header().clone().into()],
        fork_hash_index: 1,
    };
    let (peer, _server) = connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(our_tip)), claim).await;
    let config = BlockchainSyncConfig::default();
    let ban_period = config.ban_period;

    let err = sync_headers(&db, connectivity.clone(), config, vec![peer.clone()])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, peer.node_id());
    assert_eq!(*ban_duration, ban_period);
    assert!(ban_reason.contains("Header already in database"), "{ban_reason}");
}

/// A local chain with one header above the genesis block but no block for it yet.
fn chain_with_a_header_ahead_of_its_blocks() -> (BlockchainDatabase<TempDatabase>, Arc<ChainBlock>) {
    let db = create_new_blockchain();
    let header_only = block_on_genesis(&db);
    let chain_header =
        ChainHeader::try_construct(header_only.header().clone(), header_only.accumulated_data().clone()).unwrap();
    db.insert_valid_headers(vec![chain_header]).unwrap();
    (db, header_only)
}

#[tokio::test]
async fn no_headers_for_a_claim_above_our_header_chain_is_banned() {
    let (db, header_only) = chain_with_a_header_ahead_of_its_blocks();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let header_tip_metadata = claim_for(&header_only);
    let claim = ChainMetadata::new(
        header_tip_metadata.best_block_height().saturating_add(5),
        *header_tip_metadata.best_block_hash(),
        0,
        0,
        header_tip_metadata.accumulated_difficulty().saturating_mul(100.into()),
        0,
    )
    .unwrap();
    let no_headers = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let (peer, _server) = connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(no_headers)), claim).await;
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;

    let err = sync_headers(&db, connectivity.clone(), config, vec![peer.clone()])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("claimed a stronger chain"), "{ban_reason}");
    assert!(!db.is_add_block_disabled());
}

#[tokio::test]
async fn no_headers_for_a_claim_covered_by_our_header_chain_is_in_sync() {
    let (db, header_only) = chain_with_a_header_ahead_of_its_blocks();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let no_headers = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let (peer, _server) = connect_mock_peer(
        &connectivity_mock,
        MockSyncPeer::new(&db, Some(no_headers)),
        claim_for(&header_only),
    )
    .await;

    let (_, result) = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap();

    assert_eq!(result.header_sync_status, HeaderSyncStatus::InSyncOrAhead);
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
}

#[tokio::test]
async fn no_headers_above_an_older_split_when_headers_are_ahead_of_blocks_is_banned_if_inflated() {
    let (db, header_only) = chain_with_a_header_ahead_of_its_blocks();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let header_tip_metadata = claim_for(&header_only);
    let inflated_split_claim = ChainMetadata::new(
        header_tip_metadata.best_block_height(),
        [7u8; 32].into(),
        0,
        0,
        header_tip_metadata.accumulated_difficulty(),
        header_tip_metadata.timestamp(),
    )
    .unwrap();
    // We send (header tip, genesis); the peer's header tip is genesis
    let empty_at_genesis = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 1,
    };
    let (peer, _server) = connect_mock_peer(
        &connectivity_mock,
        MockSyncPeer::new(&db, Some(empty_at_genesis)),
        inflated_split_claim,
    )
    .await;
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;

    let err = sync_headers(&db, connectivity.clone(), config, vec![peer.clone()])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("claimed a stronger chain"), "{ban_reason}");
    assert!(!db.is_add_block_disabled());
}

#[tokio::test]
async fn no_headers_at_an_intermediate_split_weaker_than_our_header_tip_is_not_in_sync() {
    let db = create_new_blockchain();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, chain) = create_chained_blocks(&db, block_specs!(["A->GB"], ["B->A"]), genesis);
    let block_a = chain.get("A").unwrap();
    let block_b = chain.get("B").unwrap();
    let header_a = ChainHeader::try_construct(block_a.header().clone(), block_a.accumulated_data().clone()).unwrap();
    let header_b = ChainHeader::try_construct(block_b.header().clone(), block_b.accumulated_data().clone()).unwrap();
    db.insert_valid_headers(vec![header_a, header_b]).unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // We send (B, A, genesis)
    let split_at_a = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 1,
    };
    let (peer_a, _server_a) = connect_mock_peer(
        &connectivity_mock,
        MockSyncPeer::new(&db, Some(split_at_a)),
        claim_for(block_a),
    )
    .await;
    let split_at_b = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let (peer_b, _server_b) = connect_mock_peer(
        &connectivity_mock,
        MockSyncPeer::new(&db, Some(split_at_b)),
        claim_for(block_b),
    )
    .await;

    let mut chain_changes = HashMap::new();

    let (synced_peer, result) = sync_headers_with_history(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer_a.clone(), peer_b.clone()],
        &mut chain_changes,
    )
    .await
    .unwrap();

    assert_eq!(synced_peer.node_id(), peer_b.node_id());
    assert_eq!(result.header_sync_status, HeaderSyncStatus::InSyncOrAhead);
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    // A weaker peer is not a chain change, so it gets no strike
    assert!(!chain_changes.contains_key(peer_a.node_id()));
}

#[tokio::test]
async fn no_headers_above_an_older_split_for_a_block_that_landed_during_sync_is_not_banned() {
    let (db, header_only) = chain_with_a_header_ahead_of_its_blocks();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let empty_at_genesis = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 1,
    };
    let mut service = MockSyncPeer::new(&db, Some(empty_at_genesis));
    service.blocks_to_add = vec![header_only.clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&header_only)).await;
    let mut chain_changes = HashMap::new();

    let err = sync_headers_with_history(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer.clone()],
        &mut chain_changes,
    )
    .await
    .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(chain_changes.get(peer.node_id()).map(Vec::len), Some(1));
    assert_eq!(db.get_height().unwrap(), 1);
}

#[tokio::test]
async fn no_headers_above_an_older_split_with_a_higher_claimed_height_is_banned() {
    let db = create_new_blockchain();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, chain) = create_chained_blocks(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"]), genesis);
    db.add_block(chain.get("A").unwrap().to_arc_block()).unwrap();
    let headers = ["B", "C"]
        .iter()
        .map(|name| {
            let block = chain.get(*name).unwrap();
            ChainHeader::try_construct(block.header().clone(), block.accumulated_data().clone()).unwrap()
        })
        .collect();
    db.insert_valid_headers(headers).unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // We send (C, B, A, genesis); the peer's header tip is B, but it claims C's height
    let block_b = chain.get("B").unwrap();
    let claim = ChainMetadata::new(
        chain.get("C").unwrap().height(),
        [7u8; 32].into(),
        0,
        0,
        block_b.accumulated_data().total_accumulated_difficulty,
        block_b.header().timestamp.as_u64(),
    )
    .unwrap();
    let empty_at_b = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 1,
    };
    let (peer, _server) = connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(empty_at_b)), claim).await;
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;

    let err = sync_headers(&db, connectivity.clone(), config, vec![peer.clone()])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("claimed a higher chain tip"), "{ban_reason}");
}

#[tokio::test]
async fn no_headers_at_a_split_stronger_than_a_higher_claim_is_not_banned() {
    let db = create_new_blockchain();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, chain) = create_chained_blocks(
        &db,
        block_specs!(["H->GB", difficulty: Difficulty::from_u64(10).unwrap()]),
        genesis,
    );
    let header_only = chain.get("H").unwrap();
    let chain_header =
        ChainHeader::try_construct(header_only.header().clone(), header_only.accumulated_data().clone()).unwrap();
    db.insert_valid_headers(vec![chain_header]).unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // A stale claim for a longer but weaker chain: above our block tip, below the split, and higher than it
    let claim = ChainMetadata::new(
        header_only.height().saturating_add(1),
        [7u8; 32].into(),
        0,
        0,
        header_only
            .accumulated_data()
            .total_accumulated_difficulty
            .saturating_sub(1.into()),
        header_only.header().timestamp.as_u64(),
    )
    .unwrap();
    let no_headers = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let (peer, _server) = connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(no_headers)), claim).await;

    let (_, result) = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap();

    assert_eq!(result.header_sync_status, HeaderSyncStatus::InSyncOrAhead);
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
}

#[tokio::test]
async fn no_headers_at_a_split_our_header_chain_dropped_during_sync_is_not_banned() {
    let db = create_new_blockchain();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, chain) = create_chained_blocks(
        &db,
        block_specs!(["H->GB"], ["M->GB", difficulty: Difficulty::from_u64(10).unwrap()]),
        genesis,
    );
    let header_only = chain.get("H").unwrap();
    let chain_header =
        ChainHeader::try_construct(header_only.header().clone(), header_only.accumulated_data().clone()).unwrap();
    db.insert_valid_headers(vec![chain_header]).unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // We send (H, genesis); while the peer answers, a block on genesis replaces our header H
    let no_headers = FindChainSplitResponse {
        headers: vec![],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(no_headers));
    service.blocks_to_add = vec![chain.get("M").unwrap().clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(header_only)).await;
    let mut chain_changes = HashMap::new();

    let err = sync_headers_with_history(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer.clone()],
        &mut chain_changes,
    )
    .await
    .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(db.fetch_header_by_block_hash(*header_only.hash()).unwrap().is_none());
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(chain_changes.get(peer.node_id()).map(Vec::len), Some(1));
}

#[tokio::test]
async fn a_reorg_onto_the_peers_chain_during_sync_is_not_banned() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, mut chain) = create_chained_blocks(&db, block_specs!(["L->GB"], ["P1->GB"], ["P2->P1"]), genesis);
    let local_block = chain.remove("L").unwrap();
    let peer_block_1 = chain.remove("P1").unwrap();
    let peer_block_2 = chain.remove("P2").unwrap();
    db.add_block(local_block.to_arc_block()).unwrap();
    // We send (L, genesis); the peer's chain splits at genesis
    let reply = FindChainSplitResponse {
        headers: vec![
            peer_block_1.header().clone().into(),
            peer_block_2.header().clone().into(),
        ],
        fork_hash_index: 1,
    };
    let mut service = MockSyncPeer::new(&db, Some(reply));
    service.blocks_to_add = vec![peer_block_1.clone(), peer_block_2.clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&peer_block_2)).await;

    let err = sync_headers(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(db.get_chain_metadata().unwrap().best_block_hash(), peer_block_2.hash());
}

#[tokio::test]
async fn a_peer_whose_attempts_keep_ending_on_a_chain_change_is_banned() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (names, mut chain) = create_chained_blocks(
        &db,
        block_specs!(["A1->GB"], ["A2->A1"], ["A3->A2"], ["A4->A3"]),
        genesis,
    );
    let blocks = names.iter().map(|name| chain.remove(name).unwrap()).collect::<Vec<_>>();
    let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;
    let mut chain_changes = HashMap::new();

    // Each round, the block the peer claims lands by propagation while it answers with no headers
    for (round, block) in blocks.iter().take(3).enumerate() {
        let no_headers = FindChainSplitResponse {
            headers: vec![],
            fork_hash_index: 0,
        };
        let mut service = MockSyncPeer::new(&db, Some(no_headers));
        service.blocks_to_add = vec![block.clone()];
        let (peer, _server) =
            connect_mock_peer_as(peer_identity.clone(), &connectivity_mock, service, claim_for(block)).await;

        let err = sync_headers_with_history(
            &db,
            connectivity.clone(),
            config.clone(),
            vec![peer],
            &mut chain_changes,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
        let banned = banned_peers(connectivity.clone(), &connectivity_mock).await;
        if round < 2 {
            assert!(banned.is_empty(), "round {round}: {banned:?}");
            continue;
        }
        let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
            panic!("expected one ban, got {banned:?}");
        };
        assert_eq!(banned_peer, peer_identity.node_id());
        assert_eq!(*ban_duration, short_ban_period);
        assert!(ban_reason.contains("because our chain changed"), "{ban_reason}");
    }
    assert_eq!(chain_changes.get(peer_identity.node_id()).map(Vec::len), Some(3));

    // A successful attempt with the peer resets its count
    let last = blocks.last().unwrap();
    let reply = FindChainSplitResponse {
        headers: vec![last.header().clone().into()],
        fork_hash_index: 0,
    };
    let (peer, _server) = connect_mock_peer_as(
        peer_identity.clone(),
        &connectivity_mock,
        MockSyncPeer::new(&db, Some(reply)),
        claim_for(last),
    )
    .await;
    sync_headers_with_history(&db, connectivity.clone(), config, vec![peer], &mut chain_changes)
        .await
        .unwrap();
    assert!(!chain_changes.contains_key(peer_identity.node_id()));
}

#[tokio::test]
async fn a_split_that_leaves_our_chain_during_sync_is_not_banned() {
    // Index 1 is checked by the deep reorg anchor, index 0 only by the header validator setup
    for (fork_hash_index, peer_block_name) in [(1, "P1"), (0, "P0")] {
        let db = create_new_blockchain();
        let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
        let (_, mut chain) = create_chained_blocks(
            &db,
            block_specs!(
                ["L1->GB"],
                ["L2->L1"],
                ["P0->L2"],
                // Ahead of L2, so the peer still has a claim to serve at the start of the attempt
                ["P1->L1", difficulty: Difficulty::from_u64(10).unwrap()],
                ["M1->GB"],
                ["M2->M1"],
                ["M3->M2"]
            ),
            genesis,
        );
        db.add_block(chain.get("L1").unwrap().to_arc_block()).unwrap();
        db.add_block(chain.get("L2").unwrap().to_arc_block()).unwrap();
        let peer_block = chain.remove(peer_block_name).unwrap();
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        let connectivity_mock = connectivity_mock.spawn();
        // We send (L2, L1, genesis); while the peer answers, a stronger chain on genesis replaces L1 and L2
        let reply = FindChainSplitResponse {
            headers: vec![peer_block.header().clone().into()],
            fork_hash_index,
        };
        let mut service = MockSyncPeer::new(&db, Some(reply));
        service.blocks_to_add = ["M1", "M2", "M3"]
            .iter()
            .map(|name| chain.get(*name).unwrap().clone())
            .collect();
        let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&peer_block)).await;
        let mut chain_changes = HashMap::new();

        let err = sync_headers_with_history(
            &db,
            connectivity.clone(),
            BlockchainSyncConfig::default(),
            vec![peer.clone()],
            &mut chain_changes,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
        assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
        assert_eq!(chain_changes.get(peer.node_id()).map(Vec::len), Some(1));
    }
}

/// A peer block on genesis, and a stronger local block on genesis that is not added to `db`.
fn peer_and_stronger_local_block(db: &BlockchainDatabase<TempDatabase>) -> (Arc<ChainBlock>, Arc<ChainBlock>) {
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, mut chain) = create_chained_blocks(
        db,
        block_specs!(["peer->GB"], ["local->GB", difficulty: Difficulty::from_u64(10).unwrap()]),
        genesis,
    );
    (chain.remove("peer").unwrap(), chain.remove("local").unwrap())
}

/// Syncs from a peer that serves exactly its claim on a fork, while a stronger local block lands as it answers.
async fn sync_from_a_peer_overtaken_mid_round(
    chain_changes: &mut HashMap<NodeId, Vec<Instant>>,
    peer_identity: Arc<NodeIdentity>,
) -> (
    BlockchainDatabase<TempDatabase>,
    Arc<ChainBlock>,
    BlockHeaderSyncError,
    Vec<(NodeId, Duration, String)>,
) {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let (peer_block, local_block) = peer_and_stronger_local_block(&db);
    let reply = FindChainSplitResponse {
        headers: vec![peer_block.header().clone().into()],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(reply));
    service.blocks_to_add = vec![local_block.clone()];
    let (peer, _server) =
        connect_mock_peer_as(peer_identity, &connectivity_mock, service, claim_for(&peer_block)).await;

    let err = sync_headers_with_history(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer],
        chain_changes,
    )
    .await
    .unwrap_err();
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    (db, local_block, err, banned)
}

#[tokio::test]
async fn a_peer_whose_claim_held_when_our_chain_overtook_it_is_not_banned() {
    let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut chain_changes = HashMap::new();

    let (db, local_block, err, banned) =
        sync_from_a_peer_overtaken_mid_round(&mut chain_changes, peer_identity.clone()).await;

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned.is_empty(), "{banned:?}");
    assert_eq!(chain_changes.get(peer_identity.node_id()).map(Vec::len), Some(1));
    assert_eq!(db.get_chain_metadata().unwrap().best_block_hash(), local_block.hash());
    assert!(!db.is_add_block_disabled());
}

#[tokio::test]
async fn a_peer_overtaken_mid_round_too_often_is_banned() {
    let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut chain_changes = HashMap::new();
    chain_changes.insert(peer_identity.node_id().clone(), vec![
        Instant::now();
        MAX_CHAIN_CHANGES_PER_PEER - 1
    ]);
    let short_ban_period = BlockchainSyncConfig::default().short_ban_period;

    let (_, _, err, banned) = sync_from_a_peer_overtaken_mid_round(&mut chain_changes, peer_identity.clone()).await;

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, peer_identity.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("because our chain changed"), "{ban_reason}");
}

#[tokio::test]
async fn a_short_batch_below_the_claim_is_banned() {
    // Whether or not a stronger local block lands before the attempt reads our tip
    for local_block_lands in [false, true] {
        let db = create_new_blockchain();
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        let connectivity_mock = connectivity_mock.spawn();
        let (peer_block, local_block) = peer_and_stronger_local_block(&db);
        let peer_claim = claim_for(&peer_block);
        // Above the local block too, so the attempt still goes ahead once it lands
        let claim = ChainMetadata::new(
            peer_claim.best_block_height(),
            *peer_claim.best_block_hash(),
            0,
            0,
            local_block
                .accumulated_data()
                .total_accumulated_difficulty
                .saturating_mul(100.into()),
            peer_claim.timestamp(),
        )
        .unwrap();
        // We send (local, genesis) once the local block has landed, else (genesis)
        let reply = FindChainSplitResponse {
            headers: vec![peer_block.header().clone().into()],
            fork_hash_index: u64::from(local_block_lands),
        };
        let (peer, _server) = connect_mock_peer(&connectivity_mock, MockSyncPeer::new(&db, Some(reply)), claim).await;
        let config = BlockchainSyncConfig::default();
        let short_ban_period = config.short_ban_period;
        let hook_db = db.clone();
        let mut chain_changes = HashMap::new();

        let err = sync_headers_with_hook(
            &db,
            connectivity.clone(),
            config,
            vec![peer.clone()],
            &mut chain_changes,
            move |_| {
                if local_block_lands {
                    hook_db.add_block(local_block.to_arc_block()).unwrap();
                }
            },
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
        let banned = banned_peers(connectivity, &connectivity_mock).await;
        let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
            panic!("local block lands: {local_block_lands}: expected one ban, got {banned:?}");
        };
        assert_eq!(banned_peer, peer.node_id());
        assert_eq!(*ban_duration, short_ban_period);
        assert!(ban_reason.contains("claimed a stronger chain"), "{ban_reason}");
        assert!(chain_changes.is_empty());
    }
}

#[tokio::test]
async fn a_peer_our_chain_overtook_before_the_attempt_is_skipped() {
    let db = create_new_blockchain();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, mut chain) = create_chained_blocks(
        &db,
        block_specs!(["peer->GB"], ["local->GB", difficulty: Difficulty::from_u64(10).unwrap()], ["next->local"]),
        genesis,
    );
    let peer_block = chain.remove("peer").unwrap();
    let local_block = chain.remove("local").unwrap();
    let reply = FindChainSplitResponse {
        headers: vec![peer_block.header().clone().into()],
        fork_hash_index: 0,
    };
    let mut service = MockSyncPeer::new(&db, Some(reply));
    // Lands only if the peer is asked for the chain split
    service.blocks_to_add = vec![chain.remove("next").unwrap()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, service, claim_for(&peer_block)).await;
    let hook_db = db.clone();
    let landed = local_block.clone();
    let mut chain_changes = HashMap::new();

    let err = sync_headers_with_hook(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer],
        &mut chain_changes,
        move |_| {
            hook_db.add_block(landed.to_arc_block()).unwrap();
        },
    )
    .await
    .unwrap_err();

    assert!(matches!(err, BlockHeaderSyncError::NoMoreSyncPeers(_)), "{err}");
    assert_eq!(db.get_chain_metadata().unwrap().best_block_hash(), local_block.hash());
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert!(chain_changes.is_empty());
    assert!(
        BlockHeaderSyncError::PeerClaimNoLongerAhead {
            claimed: 1.into(),
            local: 1.into()
        }
        .get_ban_reason()
        .is_none()
    );
}
