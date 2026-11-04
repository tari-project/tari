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
use tari_comms::{connectivity::ConnectivityRequester, test_utils::mocks::create_connectivity_mock};
use tari_node_components::blocks::{ChainBlock, ChainHeader};
use tari_transaction_components::tari_proof_of_work::Difficulty;

use crate::{
    base_node::sync::{
        BlockSyncError,
        BlockSynchronizer,
        BlockchainSyncConfig,
        SyncPeer,
        mock_sync_peer::{MockSyncPeer, banned_peers, connect_mock_peer},
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
    mut sync_peers: Vec<SyncPeer>,
) -> Result<(), BlockSyncError> {
    let mut synchronizer = BlockSynchronizer::new(
        config,
        db.clone().into(),
        connectivity,
        &mut sync_peers,
        Arc::new(MockValidator::new(true)),
    );
    synchronizer.synchronize().await
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
    let saw_add_block_disabled = serving.saw_add_block_disabled.clone();
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
    // Propagated blocks are accepted until the first synced block is stored, and refused after it
    assert!(!saw_add_block_disabled.load(Ordering::SeqCst));
    assert!(db.is_add_block_disabled());
}

#[tokio::test]
async fn a_propagated_block_that_replaces_the_synced_headers_is_not_banned() {
    let (db, chain) = headers_ahead_of_blocks(
        block_specs!(["A->GB"], ["B->GB", difficulty: Difficulty::from_u64(10).unwrap()]),
        "A",
    );
    let block = chain.get("A").unwrap();
    let propagated = chain.get("B").unwrap();
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_mock = connectivity_mock.spawn();
    let mut serving = MockSyncPeer::new(&db, None);
    serving.blocks_to_serve = vec![block.block().clone()];
    serving.blocks_to_add = vec![propagated.clone()];
    let (peer, _server) = connect_mock_peer(&connectivity_mock, serving, claim_for(block)).await;

    let err = sync_blocks(&db, connectivity.clone(), BlockchainSyncConfig::default(), vec![peer])
        .await
        .unwrap_err();

    assert!(matches!(err, BlockSyncError::NoMoreSyncPeers(_)), "{err}");
    assert!(banned_peers(connectivity, &connectivity_mock).await.is_empty());
    assert_eq!(db.get_chain_metadata().unwrap().best_block_hash(), propagated.hash());
    assert!(!db.is_add_block_disabled());
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
    assert!(!db.is_add_block_disabled());
}
