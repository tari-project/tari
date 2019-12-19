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
    base_node::{
        comms_interface::{BlockEvent, CommsInterfaceError},
        service::BaseNodeServiceConfig,
    },
    blocks::{genesis_block::get_genesis_block, BlockHeader},
    chain_storage::{ChainStorageError, DbTransaction, MmrTree},
    consensus::ConsensusConstants,
    consts::BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION,
    mempool::MempoolServiceConfig,
    proof_of_work::{Difficulty, PowAlgorithm},
    test_utils::{
        builders::{add_block_and_update_header, chain_block, create_genesis_block, create_test_kernel, create_utxo},
        node::{
            create_network_with_2_base_nodes_with_config,
            create_network_with_3_base_nodes,
            create_network_with_3_base_nodes_with_config,
            random_node_identity,
            BaseNodeBuilder,
        },
    },
    tx,
};
use futures::{future, future::Either, join, stream::FusedStream, FutureExt, Stream, StreamExt};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tari_mmr::MerkleChangeTrackerConfig;
use tari_test_utils::random::string;
use tari_transactions::{
    tari_amount::{uT, MicroTari},
    types::CryptoFactories,
};
use tari_utilities::hash::Hashable;
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn request_response_get_metadata() {
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    add_block_and_update_header(&bob_node.blockchain_db, create_genesis_block(&factories).0);

    runtime.block_on(async {
        let received_metadata = alice_node.outbound_nci.get_metadata().await.unwrap();
        assert_eq!(received_metadata.len(), 2);
        assert!(
            (received_metadata[0].height_of_longest_chain == None) ||
                (received_metadata[1].height_of_longest_chain == None)
        );
        assert!(
            (received_metadata[0].height_of_longest_chain == Some(0)) ||
                (received_metadata[1].height_of_longest_chain == Some(0))
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn request_and_response_fetch_headers() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let mut headerb1 = BlockHeader::new(0);
    headerb1.height = 1;
    let mut headerb2 = BlockHeader::new(0);
    headerb2.height = 2;
    let mut txn = DbTransaction::new();
    txn.insert_header(headerb1.clone(), true);
    txn.insert_header(headerb2.clone(), true);
    assert!(bob_node.blockchain_db.commit(txn).is_ok());

    let mut headerc1 = BlockHeader::new(0);
    headerc1.height = 1;
    let mut headerc2 = BlockHeader::new(0);
    headerc2.height = 2;
    let mut txn = DbTransaction::new();
    txn.insert_header(headerc1.clone(), true);
    txn.insert_header(headerc2.clone(), true);
    assert!(carol_node.blockchain_db.commit(txn).is_ok());

    // The request is sent to a random remote base node so the returned headers can be from bob or carol
    runtime.block_on(async {
        let received_headers = alice_node.outbound_nci.fetch_headers(vec![1]).await.unwrap();
        assert_eq!(received_headers.len(), 1);
        assert!(received_headers.contains(&headerb1) || received_headers.contains(&headerc1));

        let received_headers = alice_node.outbound_nci.fetch_headers(vec![1, 2]).await.unwrap();
        assert_eq!(received_headers.len(), 2);
        assert!(
            (received_headers.contains(&headerb1) && (received_headers.contains(&headerb2))) ||
                (received_headers.contains(&headerc1) && (received_headers.contains(&headerc2)))
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn request_and_response_fetch_kernels() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let kernel1 = create_test_kernel(5.into(), 0);
    let kernel2 = create_test_kernel(10.into(), 1);
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone(), true);
    txn.insert_kernel(kernel2.clone(), true);
    assert!(bob_node.blockchain_db.commit(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone(), true);
    txn.insert_kernel(kernel2.clone(), true);
    assert!(carol_node.blockchain_db.commit(txn).is_ok());

    runtime.block_on(async {
        let received_kernels = alice_node
            .outbound_nci
            .fetch_kernels(vec![hash1.clone()])
            .await
            .unwrap();
        assert_eq!(received_kernels.len(), 1);
        assert_eq!(received_kernels[0], kernel1);

        let received_kernels = alice_node.outbound_nci.fetch_kernels(vec![hash1, hash2]).await.unwrap();
        assert_eq!(received_kernels.len(), 2);
        assert!(received_kernels.contains(&kernel1));
        assert!(received_kernels.contains(&kernel2));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn request_and_response_fetch_utxos() {
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    assert!(bob_node.blockchain_db.commit(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    assert!(carol_node.blockchain_db.commit(txn).is_ok());

    runtime.block_on(async {
        let received_utxos = alice_node.outbound_nci.fetch_utxos(vec![hash1.clone()]).await.unwrap();
        assert_eq!(received_utxos.len(), 1);
        assert_eq!(received_utxos[0], utxo1);

        let received_utxos = alice_node.outbound_nci.fetch_utxos(vec![hash1, hash2]).await.unwrap();
        assert_eq!(received_utxos.len(), 2);
        assert!(received_utxos.contains(&utxo1));
        assert!(received_utxos.contains(&utxo2));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn request_and_response_fetch_blocks() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let block0 = add_block_and_update_header(&bob_node.blockchain_db, get_genesis_block());
    let mut block1 = chain_block(&block0, vec![]);
    block1 = add_block_and_update_header(&bob_node.blockchain_db, block1);
    let mut block2 = chain_block(&block1, vec![]);
    block2 = add_block_and_update_header(&bob_node.blockchain_db, block2);

    carol_node.blockchain_db.add_new_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_new_block(block1.clone()).unwrap();
    carol_node.blockchain_db.add_new_block(block2.clone()).unwrap();

    runtime.block_on(async {
        let received_blocks = alice_node.outbound_nci.fetch_blocks(vec![0]).await.unwrap();
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(*received_blocks[0].block(), block0);

        let received_blocks = alice_node.outbound_nci.fetch_blocks(vec![0, 1]).await.unwrap();
        assert_eq!(received_blocks.len(), 2);
        assert_ne!(*received_blocks[0].block(), *received_blocks[1].block());
        assert!((*received_blocks[0].block() == block0) || (*received_blocks[1].block() == block0));
        assert!((*received_blocks[0].block() == block1) || (*received_blocks[1].block() == block1));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn request_and_response_fetch_mmr_state() {
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 1,
        max_history_len: 3,
    };
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) = create_network_with_3_base_nodes_with_config(
        &runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        MempoolServiceConfig::default(),
        temp_dir.path().to_str().unwrap(),
    );

    let (tx1, inputs1, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
    let (tx2, inputs2, _) = tx!(10_000*uT, fee: 20*uT, inputs: 1, outputs: 1);
    let (_, inputs3, _) = tx!(10_000*uT, fee: 25*uT, inputs: 1, outputs: 1);

    let block0 = add_block_and_update_header(&bob_node.blockchain_db, get_genesis_block());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(inputs1[0].as_transaction_output(&factories).unwrap(), true);
    txn.insert_utxo(inputs2[0].as_transaction_output(&factories).unwrap(), true);
    txn.insert_utxo(inputs3[0].as_transaction_output(&factories).unwrap(), true);
    assert!(bob_node.blockchain_db.commit(txn).is_ok());
    let mut block1 = chain_block(&block0, vec![tx1.clone()]);
    block1 = add_block_and_update_header(&bob_node.blockchain_db, block1);
    let mut block2 = chain_block(&block1, vec![]);
    block2 = add_block_and_update_header(&bob_node.blockchain_db, block2);
    let block3 = chain_block(&block2, vec![tx2.clone()]);
    bob_node.blockchain_db.add_new_block(block3.clone()).unwrap();

    let block0 = add_block_and_update_header(&carol_node.blockchain_db, get_genesis_block());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(inputs1[0].as_transaction_output(&factories).unwrap(), true);
    txn.insert_utxo(inputs2[0].as_transaction_output(&factories).unwrap(), true);
    txn.insert_utxo(inputs3[0].as_transaction_output(&factories).unwrap(), true);
    assert!(carol_node.blockchain_db.commit(txn).is_ok());
    let mut block1 = chain_block(&block0, vec![tx1.clone()]);
    block1 = add_block_and_update_header(&carol_node.blockchain_db, block1);
    let mut block2 = chain_block(&block1, vec![]);
    block2 = add_block_and_update_header(&carol_node.blockchain_db, block2);
    let block3 = chain_block(&block2, vec![tx2.clone()]);
    carol_node.blockchain_db.add_new_block(block3.clone()).unwrap();

    runtime.block_on(async {
        // Partial queries
        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::Utxo, 1, 2)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 4);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 2);

        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::Kernel, 1, 2)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 1);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 0); // request out of range

        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::RangeProof, 1, 2)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 4);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 2);

        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::Header, 1, 2)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 3);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 2);

        // Comprehensive queries
        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::Utxo, 0, 100)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 4);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 4);

        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::Kernel, 0, 100)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 1);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 1);

        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::RangeProof, 0, 100)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 4);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 4);

        let received_mmr_state = alice_node
            .outbound_nci
            .fetch_mmr_state(MmrTree::Header, 0, 100)
            .await
            .unwrap();
        assert_eq!(received_mmr_state.total_leaf_count, 3);
        assert_eq!(received_mmr_state.leaf_nodes.leaf_hashes.len(), 3);
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

pub async fn event_stream_next<TStream>(mut stream: TStream, timeout: Duration) -> Option<TStream::Item>
where TStream: Stream + FusedStream + Unpin {
    let either = future::select(
        stream.select_next_some(),
        tokio::timer::delay(Instant::now() + timeout).fuse(),
    )
    .await;

    match either {
        Either::Left((v, _)) => Some(v),
        Either::Right(_) => None,
    }
}

#[test]
fn propagate_and_forward_valid_block() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    // Alice will propagate block to bob, bob will receive it, verify it and then propagate it to carol and dan. Dan and
    // Carol will also try to propagate the block to each other, as they dont know that bob sent it to the other node.
    // These duplicate blocks will be discarded and wont be propagated again.
    //              /-> carol <-\
    //             /             |
    // alice -> bob             |
    //             \             |
    //              \->  dan  <-/
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let dan_node_identity = random_node_identity();
    let mut alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .start(&runtime, temp_dir.path().to_str().unwrap());
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![
            alice_node_identity,
            carol_node_identity.clone(),
            dan_node_identity.clone(),
        ])
        .start(&runtime, temp_dir.path().to_str().unwrap());
    let carol_node = BaseNodeBuilder::new()
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone(), dan_node_identity.clone()])
        .start(&runtime, temp_dir.path().to_str().unwrap());
    let dan_node = BaseNodeBuilder::new()
        .with_node_identity(dan_node_identity)
        .with_peers(vec![bob_node_identity, carol_node_identity])
        .start(&runtime, temp_dir.path().to_str().unwrap());

    let block0 = add_block_and_update_header(&alice_node.blockchain_db, get_genesis_block());
    let mut block1 = chain_block(&block0, vec![]);
    block1 = add_block_and_update_header(&alice_node.blockchain_db, block1);
    let block1_hash = block1.hash();

    bob_node.blockchain_db.add_new_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_new_block(block0.clone()).unwrap();
    dan_node.blockchain_db.add_new_block(block0.clone()).unwrap();

    runtime.block_on(async {
        // Alice will start the propagation. Bob, Carol and Dan will propagate based on the logic in their inbound
        // handle_block handlers
        assert!(alice_node
            .outbound_nci
            .propagate_block(block1.clone(), vec![])
            .await
            .is_ok());

        let bob_block_event_stream = bob_node.local_nci.get_block_event_stream_fused();
        let bob_block_event_fut = event_stream_next(bob_block_event_stream, Duration::from_millis(20000));
        let carol_block_event_stream = carol_node.local_nci.get_block_event_stream_fused();
        let carol_block_event_fut = event_stream_next(carol_block_event_stream, Duration::from_millis(20000));
        let dan_block_event_stream = dan_node.local_nci.get_block_event_stream_fused();
        let dan_block_event_fut = event_stream_next(dan_block_event_stream, Duration::from_millis(20000));
        let (bob_block_event, carol_block_event, dan_block_event) =
            join!(bob_block_event_fut, carol_block_event_fut, dan_block_event_fut);

        if let BlockEvent::Verified((received_block, _)) = &*bob_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            assert!(false);
        }
        if let BlockEvent::Verified((received_block, _block_add_result)) = &*carol_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            assert!(false);
        }
        if let BlockEvent::Verified((received_block, _block_add_result)) = &*dan_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            assert!(false);
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
    dan_node.comms.shutdown().unwrap();
}

#[test]
fn propagate_and_forward_invalid_block() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    // Alice will propagate an invalid block to Carol and Bob, they will check the received block and not propagate the
    // block to dan.
    //       /->  bob  -\
    //      /            \
    // alice              -> dan
    //      \            /
    //       \-> carol -/
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let dan_node_identity = random_node_identity();
    let mut alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone(), carol_node_identity.clone()])
        .start(&runtime, temp_dir.path().to_str().unwrap());
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity.clone(), dan_node_identity.clone()])
        .start(&runtime, temp_dir.path().to_str().unwrap());
    let carol_node = BaseNodeBuilder::new()
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![alice_node_identity, dan_node_identity.clone()])
        .start(&runtime, temp_dir.path().to_str().unwrap());
    let dan_node = BaseNodeBuilder::new()
        .with_node_identity(dan_node_identity)
        .with_peers(vec![bob_node_identity, carol_node_identity])
        .start(&runtime, temp_dir.path().to_str().unwrap());

    let block0 = add_block_and_update_header(&alice_node.blockchain_db, get_genesis_block());
    let block1 = chain_block(&block0, vec![]);
    let block1_hash = block1.hash();

    bob_node.blockchain_db.add_new_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_new_block(block0.clone()).unwrap();

    runtime.block_on(async {
        assert!(alice_node
            .outbound_nci
            .propagate_block(block1.clone(), vec![])
            .await
            .is_ok());

        let bob_block_event_stream = bob_node.local_nci.get_block_event_stream_fused();
        let bob_block_event_fut = event_stream_next(bob_block_event_stream, Duration::from_millis(20000));
        let carol_block_event_stream = carol_node.local_nci.get_block_event_stream_fused();
        let carol_block_event_fut = event_stream_next(carol_block_event_stream, Duration::from_millis(20000));
        let dan_block_event_stream = dan_node.local_nci.get_block_event_stream_fused();
        let dan_block_event_fut = event_stream_next(dan_block_event_stream, Duration::from_millis(5000));
        let (bob_block_event, carol_block_event, dan_block_event) =
            join!(bob_block_event_fut, carol_block_event_fut, dan_block_event_fut);

        if let BlockEvent::Invalid((received_block, err)) = &*bob_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
            assert_eq!(*err, ChainStorageError::MismatchedMmrRoot(MmrTree::Kernel));
        } else {
            assert!(false);
        }
        if let BlockEvent::Invalid((received_block, err)) = &*carol_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
            assert_eq!(*err, ChainStorageError::MismatchedMmrRoot(MmrTree::Kernel));
        } else {
            assert!(false);
        }
        assert!(dan_block_event.is_none());
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
    dan_node.comms.shutdown().unwrap();
}

#[test]
fn service_request_timeout() {
    let runtime = Runtime::new().unwrap();
    let base_node_service_config = BaseNodeServiceConfig {
        request_timeout: Duration::from_millis(1),
        desired_response_fraction: BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION,
    };
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 30,
    };
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &runtime,
        base_node_service_config,
        mct_config,
        MempoolServiceConfig::default(),
        temp_dir.path().to_str().unwrap(),
    );

    runtime.block_on(async {
        assert_eq!(
            alice_node.outbound_nci.get_metadata().await,
            Err(CommsInterfaceError::RequestTimedOut)
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn local_get_metadata() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mut node = BaseNodeBuilder::new().start(&runtime, temp_dir.path().to_str().unwrap());

    let block0 = add_block_and_update_header(&node.blockchain_db, get_genesis_block());
    let mut block1 = chain_block(&block0, vec![]);
    block1 = add_block_and_update_header(&node.blockchain_db, block1);
    let mut block2 = chain_block(&block1, vec![]);
    block2 = add_block_and_update_header(&node.blockchain_db, block2);

    runtime.block_on(async {
        let metadata = node.local_nci.get_metadata().await.unwrap();
        assert_eq!(metadata.height_of_longest_chain, Some(2));
        assert_eq!(metadata.best_block, Some(block2.hash()));
    });

    node.comms.shutdown().unwrap();
}

#[test]
fn local_get_new_block_template_and_get_new_block() {
    let factories = CryptoFactories::default();
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mut node = BaseNodeBuilder::new().start(&runtime, temp_dir.path().to_str().unwrap());

    add_block_and_update_header(&node.blockchain_db, get_genesis_block());
    let (tx1, inputs1, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
    let (tx2, inputs2, _) = tx!(10_000*uT, fee: 20*uT, inputs: 1, outputs: 1);
    let (tx3, inputs3, _) = tx!(10_000*uT, fee: 30*uT, inputs: 1, outputs: 1);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(inputs1[0].as_transaction_output(&factories).unwrap(), true);
    txn.insert_utxo(inputs2[0].as_transaction_output(&factories).unwrap(), true);
    txn.insert_utxo(inputs3[0].as_transaction_output(&factories).unwrap(), true);
    assert!(node.blockchain_db.commit(txn).is_ok());
    assert!(node.mempool.insert(Arc::new(tx1)).is_ok());
    assert!(node.mempool.insert(Arc::new(tx2)).is_ok());
    assert!(node.mempool.insert(Arc::new(tx3)).is_ok());

    runtime.block_on(async {
        let block_template = node.local_nci.get_new_block_template().await.unwrap();
        assert_eq!(block_template.header.height, 1);
        assert_eq!(block_template.body.kernels().len(), 3);

        let mut block = node.local_nci.get_new_block(block_template.clone()).await.unwrap();
        block.header.pow.accumulated_blake_difficulty = Difficulty::from(100);
        assert_eq!(block.header.height, 1);
        assert_eq!(block.body, block_template.body);

        assert!(node.blockchain_db.add_block(block.clone()).is_ok());
    });

    node.comms.shutdown().unwrap();
}

#[test]
fn local_get_target_difficulty() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mut node = BaseNodeBuilder::new().start(&runtime, temp_dir.path().to_str().unwrap());

    let block0 = add_block_and_update_header(&node.blockchain_db, get_genesis_block());
    assert_eq!(node.blockchain_db.get_height(), Ok(Some(0)));

    runtime.block_on(async {
        let monero_target_difficulty1 = node
            .local_nci
            .get_target_difficulty(PowAlgorithm::Monero)
            .await
            .unwrap();
        let blake_target_difficulty1 = node.local_nci.get_target_difficulty(PowAlgorithm::Blake).await.unwrap();
        assert_ne!(monero_target_difficulty1, Difficulty::from(0));
        assert_ne!(blake_target_difficulty1, Difficulty::from(0));

        let mut block1 = chain_block(&block0, Vec::new());
        block1.header.timestamp = block0
            .header
            .timestamp
            .increase(ConsensusConstants::current().get_target_block_interval());
        block1.header.pow.pow_algo = PowAlgorithm::Blake;
        add_block_and_update_header(&node.blockchain_db, block1);
        assert_eq!(node.blockchain_db.get_height(), Ok(Some(1)));

        let monero_target_difficulty2 = node
            .local_nci
            .get_target_difficulty(PowAlgorithm::Monero)
            .await
            .unwrap();
        let blake_target_difficulty2 = node.local_nci.get_target_difficulty(PowAlgorithm::Blake).await.unwrap();
        assert!(monero_target_difficulty1 <= monero_target_difficulty2);
        assert!(blake_target_difficulty1 <= blake_target_difficulty2);
    });

    node.comms.shutdown().unwrap();
}

#[test]
fn local_submit_block() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mut node = BaseNodeBuilder::new().start(&runtime, temp_dir.path().to_str().unwrap());

    let block0 = add_block_and_update_header(&node.blockchain_db, get_genesis_block());
    let mut block1 = chain_block(&block0, vec![]);
    block1.header.height = 1;

    runtime.block_on(async {
        assert!(node.local_nci.submit_block(block1.clone()).await.is_ok());

        let block_event_stream = node.local_nci.get_block_event_stream_fused();
        let bob_block_event = event_stream_next(block_event_stream, Duration::from_millis(20000)).await;

        if let BlockEvent::Invalid((received_block, err)) = &*bob_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1.hash());
            assert_eq!(*err, ChainStorageError::MismatchedMmrRoot(MmrTree::Kernel));
        } else {
            assert!(false);
        }
    });

    node.comms.shutdown().unwrap();
}
