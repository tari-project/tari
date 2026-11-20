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
    connection_manager::ConnectionManagerError,
    connectivity::{ConnectivityError, ConnectivityRequester},
    peer_manager::{NodeId, PeerFeatures},
    protocol::rpc::{HandshakeRejectReason, RpcError, RpcHandshakeError},
    test_utils::{mocks::create_connectivity_mock, node_identity::build_node_identity},
};
use tari_node_components::blocks::{ChainBlock, ChainHeader};

use super::synchronizer::{dial_failure_is_peer_caused, rpc_setup_failure_is_peer_caused};
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
        blockchain::{TempDatabase, create_chained_blocks, create_main_chain, create_new_blockchain},
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

/// A peer that answers `sync_blocks` with NotFound
fn not_found_peer(db: &BlockchainDatabase<TempDatabase>) -> MockSyncPeer {
    let mut peer = MockSyncPeer::new(db, None);
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
        let service = not_found_peer(&db);
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

#[tokio::test]
async fn a_failure_on_our_side_is_not_recorded_against_the_peer() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // The block is stored locally while the peer serves it, so storing it again fails on our side
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block.block().clone()];
    serving.blocks_to_add = vec![block.clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, serving, claim_for(block)).await;
    let mut attempts_without_block = HashMap::new();

    let err = sync_blocks_with_history(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer],
        &mut attempts_without_block,
    )
    .await
    .unwrap_err();

    assert!(matches!(err, BlockSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(attempts_without_block.is_empty(), "{attempts_without_block:?}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
}

#[tokio::test]
async fn not_found_from_a_peer_whose_claim_is_below_our_header_tip_is_not_recorded() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"]), "A");
    let block = chain.get("A").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    // The peer claims only as much work as our best block, which is less than our header tip
    let local = db.get_chain_metadata().unwrap();
    assert!(local.accumulated_difficulty() < block.accumulated_data().total_accumulated_difficulty);
    let claim = ChainMetadata::new(1, [9u8; 32].into(), 0, 0, local.accumulated_difficulty(), 0).unwrap();
    let (peer, _server) = connect_mock_peer(&connectivity_mock, not_found_peer(&db), claim).await;
    let mut attempts_without_block = HashMap::new();

    let err = sync_blocks_with_history(
        &db,
        connectivity.clone(),
        BlockchainSyncConfig::default(),
        vec![peer],
        &mut attempts_without_block,
    )
    .await
    .unwrap_err();

    assert!(matches!(err, BlockSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(attempts_without_block.is_empty(), "{attempts_without_block:?}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
}

/// Builds blocks A and B as another node's main chain, and adds only their headers to a new database, so that our
/// header tip is two blocks ahead of our blocks. B is built once A is that node's tip: the test helpers only get a
/// block's MMR roots right when its parent is the tip.
fn two_headers_ahead_of_blocks() -> (BlockchainDatabase<TempDatabase>, Arc<ChainBlock>, Arc<ChainBlock>) {
    let other_node = create_new_blockchain();
    let (_, chain) = create_main_chain(&other_node, block_specs!(["A->GB"]));
    let block_a = chain.get("A").unwrap().clone();
    // "GB" names the root of the chain being built, here A
    let (_, chain) = create_chained_blocks(&other_node, block_specs!(["B->GB"]), block_a.clone());
    let block_b = chain.get("B").unwrap().clone();
    let db = create_new_blockchain();
    let headers = [&block_a, &block_b]
        .into_iter()
        .map(|block| ChainHeader::try_construct(block.header().clone(), block.accumulated_data().clone()).unwrap())
        .collect();
    db.insert_valid_headers(headers).unwrap();
    (db, block_a, block_b)
}

#[tokio::test]
async fn an_honest_peer_with_a_stronger_claim_than_our_header_tip_that_supplies_all_requested_blocks_is_not_banned() {
    let (db, chain) = headers_ahead_of_blocks(block_specs!(["A->GB"], ["B->A"]), "A");
    let block_a = chain.get("A").unwrap();
    let block_b = chain.get("B").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block_a.block().clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, serving, claim_for(block_b)).await;

    sync_blocks(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap();

    assert_eq!(db.get_height().unwrap(), 1);
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
}

#[tokio::test]
async fn a_peer_that_stops_before_our_header_tip_does_not_complete_block_sync_early() {
    let (db, block_a, _) = two_headers_ahead_of_blocks();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut partial = MockSyncPeer::new(&db, None);
    partial.blocks_to_serve = vec![block_a.block().clone()];
    let (partial_peer, _partial_server) = connect_mock_peer(&connectivity_mock, partial, claim_for(&block_a)).await;

    let err = sync_blocks(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![
        partial_peer.clone(),
    ])
    .await
    .unwrap_err();

    assert!(matches!(err, BlockSyncError::NoMoreSyncPeers(_)), "{err}");
    assert_eq!(db.get_height().unwrap(), 1);
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, _, _)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, partial_peer.node_id());
}

#[tokio::test]
async fn a_peer_with_a_stronger_claim_that_stops_below_our_header_tip_is_banned_and_the_next_peer_completes() {
    let (db, block_a, block_b) = two_headers_ahead_of_blocks();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut partial = MockSyncPeer::new(&db, None);
    partial.blocks_to_serve = vec![block_a.block().clone()];
    let b_difficulty = block_b.accumulated_data().total_accumulated_difficulty;
    let beyond_b = ChainMetadata::new(3, [9u8; 32].into(), 0, 0, b_difficulty + 1, 0).unwrap();
    let (partial_peer, _partial_server) = connect_mock_peer(&connectivity_mock, partial, beyond_b).await;
    // The mock serves its blocks whatever the requested start, so this peer only has what is left after A
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block_b.block().clone()];
    let (serving_peer, _serving_server) = connect_mock_peer(&connectivity_mock, serving, claim_for(&block_b)).await;
    let config = BlockchainSyncConfig::default();
    let short_ban_period = config.short_ban_period;

    sync_blocks(&db, connectivity.clone(), config, vec![
        partial_peer.clone(),
        serving_peer,
    ])
    .await
    .unwrap();

    assert_eq!(db.get_height().unwrap(), 2);
    let banned = banned_peers(connectivity, &connectivity_mock).await;
    let [(banned_peer, ban_duration, ban_reason)] = banned.as_slice() else {
        panic!("expected one ban, got {banned:?}");
    };
    assert_eq!(banned_peer, partial_peer.node_id());
    assert_eq!(*ban_duration, short_ban_period);
    assert!(
        ban_reason.contains("did not supply all the requested blocks"),
        "{ban_reason}"
    );
}

#[test]
fn only_failures_the_peer_caused_are_recorded() {
    let busy = [
        RpcError::HandshakeError(RpcHandshakeError::Rejected(
            HandshakeRejectReason::NoServerSessionsAvailable("full"),
        )),
        RpcError::HandshakeError(RpcHandshakeError::Rejected(
            HandshakeRejectReason::NoClientSessionsAvailable("full"),
        )),
    ];
    for err in busy {
        assert!(!rpc_setup_failure_is_peer_caused(&err), "{err}");
    }
    assert!(rpc_setup_failure_is_peer_caused(&RpcError::ReplyTimeout));

    let our_ban =
        BlockSyncError::ConnectivityError(ConnectivityError::ConnectionFailed(ConnectionManagerError::PeerBanned));
    assert!(!dial_failure_is_peer_caused(&our_ban));
    let unreachable = BlockSyncError::ConnectivityError(ConnectivityError::ConnectionFailed(
        ConnectionManagerError::DialConnectFailedAllAddresses,
    ));
    assert!(dial_failure_is_peer_caused(&unreachable));
}
