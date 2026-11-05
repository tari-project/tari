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
    sync::Arc,
    time::{Duration, Instant},
};

use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, PeerFeatures},
    test_utils::{mocks::create_connectivity_mock, node_identity::build_node_identity},
};
use tari_node_components::blocks::{ChainBlock, ChainHeader};

use crate::{
    base_node::sync::{
        BlockSyncError,
        BlockSynchronizer,
        BlockchainSyncConfig,
        SyncPeer,
        mock_sync_peer::{MockSyncPeer, banned_peers, connect_mock_peer, connect_mock_peer_as},
    },
    chain_storage::BlockchainDatabase,
    test_helpers::{
        BlockSpecs,
        blockchain::{TempDatabase, create_chained_blocks, create_new_blockchain},
    },
    validation::mocks::MockValidator,
};

/// Builds `specs` on the genesis block without adding them, and adds the header of `header_ahead` so that our header
/// chain is one block ahead of our blocks.
fn headers_ahead_of_blocks(
    specs: BlockSpecs,
    header_ahead: &str,
) -> (BlockchainDatabase<TempDatabase>, HashMap<String, Arc<ChainBlock>>) {
    let db = create_new_blockchain();
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap());
    let (_, chain) = create_chained_blocks(&db, specs, genesis);
    let block = chain.get(header_ahead).unwrap();
    let chain_header = ChainHeader::try_construct(block.header().clone(), block.accumulated_data().clone()).unwrap();
    db.insert_valid_headers(vec![chain_header]).unwrap();
    (db, chain)
}

fn claim_for(block: &ChainBlock) -> ChainMetadata {
    ChainMetadata::new(
        block.height(),
        *block.hash(),
        0,
        0,
        block.accumulated_data().total_accumulated_difficulty,
        0,
    )
    .unwrap()
}

fn short_deadline_config() -> BlockchainSyncConfig {
    BlockchainSyncConfig {
        rpc_deadline: Duration::from_secs(1),
        ..Default::default()
    }
}

async fn sync_blocks(
    db: &BlockchainDatabase<TempDatabase>,
    connectivity: ConnectivityRequester,
    config: BlockchainSyncConfig,
    sync_peers: Vec<SyncPeer>,
) -> Result<(), BlockSyncError> {
    sync_blocks_with_history(db, connectivity, config, sync_peers, &mut HashMap::new()).await
}

async fn sync_blocks_with_history(
    db: &BlockchainDatabase<TempDatabase>,
    connectivity: ConnectivityRequester,
    config: BlockchainSyncConfig,
    mut sync_peers: Vec<SyncPeer>,
    attempts_without_block: &mut HashMap<NodeId, Vec<Instant>>,
) -> Result<(), BlockSyncError> {
    let mut synchronizer = BlockSynchronizer::new(
        config,
        db.clone().into(),
        connectivity,
        &mut sync_peers,
        Arc::new(MockValidator::new(true)),
        attempts_without_block,
    );
    synchronizer.synchronize().await
}

/// A claim for `block`'s work that names a different best block, so a NotFound from the peer is not banned
fn claim_with_another_tip(block: &ChainBlock) -> ChainMetadata {
    ChainMetadata::new(
        block.height(),
        [9u8; 32].into(),
        0,
        0,
        block.accumulated_data().total_accumulated_difficulty,
        0,
    )
    .unwrap()
}

/// A peer that answers `sync_blocks` with NotFound after `delay`
fn not_found_peer(db: &BlockchainDatabase<TempDatabase>, delay: Duration) -> MockSyncPeer {
    let mut peer = MockSyncPeer::new(db, None);
    peer.sync_blocks_stall = delay;
    peer.sync_blocks_not_found = true;
    peer
}

#[tokio::test]
async fn a_peer_that_sends_no_block_in_time_is_banned_and_the_next_peer_is_tried() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut stalling = MockSyncPeer::new(&db, None);
    stalling.sync_blocks_stall = Duration::from_secs(30);
    let (stalling_peer, _stalling_server) = connect_mock_peer(&connectivity_mock, stalling, claim_for(block)).await;
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block.block().clone()];
    let (serving_peer, _serving_server) = connect_mock_peer(&connectivity_mock, serving, claim_for(block)).await;
    let config = short_deadline_config();
    let short_ban_period = config.short_ban_period;

    let timer = Instant::now();
    sync_blocks(&db, connectivity.clone(), config, vec![
        stalling_peer.clone(),
        serving_peer,
    ])
    .await
    .unwrap();
    let elapsed = timer.elapsed();

    // The budget until the first valid block is 2 x rpc_deadline
    assert!(elapsed >= Duration::from_secs(2), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(4), "{elapsed:?}");
    assert_eq!(db.get_height().unwrap(), 1);
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, stalling_peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(ban_reason.contains("did not deliver a valid block"), "{ban_reason}");
}

#[tokio::test]
async fn a_round_without_a_block_ends_after_twice_the_peer_budget() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut peers = Vec::new();
    let mut servers = Vec::new();
    for _ in 0..3 {
        let mut stalling = MockSyncPeer::new(&db, None);
        stalling.sync_blocks_stall = Duration::from_secs(30);
        let (peer, server) = connect_mock_peer(&connectivity_mock, stalling, claim_for(block)).await;
        peers.push(peer);
        servers.push(server);
    }
    let config = short_deadline_config();
    let short_ban_period = config.short_ban_period;

    let timer = Instant::now();
    let err = sync_blocks(&db, connectivity.clone(), config, peers.clone())
        .await
        .unwrap_err();
    let elapsed = timer.elapsed();

    // Two peers use up their 2 x rpc_deadline each, then the round stops before trying the third
    assert!(matches!(err, BlockSyncError::RoundWithoutBlock { .. }), "{err}");
    assert!(elapsed >= Duration::from_secs(4), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(6), "{elapsed:?}");
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let banned_ids = banned
        .iter()
        .map(|(id, duration, _)| (id.clone(), *duration))
        .collect::<Vec<_>>();
    let [first, second, _] = peers.as_slice() else {
        panic!("expected three peers");
    };
    assert_eq!(banned_ids, vec![
        (first.node_id().clone(), short_ban_period),
        (second.node_id().clone(), short_ban_period),
    ]);
}

#[tokio::test]
async fn attempts_that_end_without_a_block_or_a_ban_do_not_use_up_the_round() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut peers = Vec::new();
    let mut servers = Vec::new();
    // Each answers within the server's 1s deadline, but together they take longer than the round's budget of
    // 4 x rpc_deadline
    for _ in 0..6 {
        let service = not_found_peer(&db, Duration::from_millis(800));
        let (peer, server) = connect_mock_peer(&connectivity_mock, service, claim_with_another_tip(block)).await;
        peers.push(peer);
        servers.push(server);
    }
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block.block().clone()];
    let (serving_peer, _serving_server) = connect_mock_peer(&connectivity_mock, serving, claim_for(block)).await;
    peers.push(serving_peer);
    let mut attempts_without_block = HashMap::new();

    sync_blocks_with_history(
        &db,
        connectivity.clone(),
        short_deadline_config(),
        peers.clone(),
        &mut attempts_without_block,
    )
    .await
    .unwrap();

    assert_eq!(db.get_height().unwrap(), 1);
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(attempts_without_block.len(), 6);
}

#[tokio::test]
async fn a_peer_whose_attempts_keep_ending_without_a_block_is_banned() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let peer_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;
    let mut attempts_without_block = HashMap::new();

    for round in 0..3 {
        let service = not_found_peer(&db, Duration::ZERO);
        let (peer, _server) = connect_mock_peer_as(
            peer_identity.clone(),
            &connectivity_mock,
            service,
            claim_with_another_tip(block),
        )
        .await;

        let err = sync_blocks_with_history(
            &db,
            connectivity.clone(),
            config.clone(),
            vec![peer],
            &mut attempts_without_block,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, BlockSyncError::NoMoreSyncPeers(_)), "{err}");
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
        assert!(ban_reason.contains("without a block"), "{ban_reason}");
    }

    // Delivering a block resets the peer's count
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block.block().clone()];
    let (peer, _server) =
        connect_mock_peer_as(peer_identity.clone(), &connectivity_mock, serving, claim_for(block)).await;
    sync_blocks_with_history(&db, connectivity, config, vec![peer], &mut attempts_without_block)
        .await
        .unwrap();
    assert!(!attempts_without_block.contains_key(peer_identity.node_id()));
}

#[tokio::test]
async fn a_peer_whose_recent_attempts_ended_without_a_block_is_tried_after_the_others() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut stalling = MockSyncPeer::new(&db, None);
    stalling.sync_blocks_stall = Duration::from_secs(30);
    let (stalling_peer, _stalling_server) = connect_mock_peer(&connectivity_mock, stalling, claim_for(block)).await;
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block.block().clone()];
    let (serving_peer, _serving_server) = connect_mock_peer(&connectivity_mock, serving, claim_for(block)).await;
    let mut attempts_without_block = HashMap::new();
    attempts_without_block.insert(stalling_peer.node_id().clone(), vec![Instant::now()]);

    let timer = Instant::now();
    sync_blocks_with_history(
        &db,
        connectivity.clone(),
        short_deadline_config(),
        vec![stalling_peer, serving_peer],
        &mut attempts_without_block,
    )
    .await
    .unwrap();

    // The stalling peer is listed first but tried last, so it is never reached
    assert!(timer.elapsed() < Duration::from_secs(1), "{:?}", timer.elapsed());
    assert_eq!(db.get_height().unwrap(), 1);
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
}
