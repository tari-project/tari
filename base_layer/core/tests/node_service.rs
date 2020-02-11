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

#[allow(dead_code)]
mod helpers;

use futures::{future, future::Either, join, stream::FusedStream, FutureExt, Stream, StreamExt};
use helpers::{
    block_builders::{
        append_block,
        chain_block,
        create_genesis_block,
        create_genesis_block_with_utxos,
        generate_block,
    },
    nodes::{
        create_network_with_2_base_nodes_with_config,
        create_network_with_3_base_nodes,
        random_node_identity,
        BaseNodeBuilder,
    },
};
use std::time::Duration;
use tari_core::{
    base_node::{
        comms_interface::{BlockEvent, CommsInterfaceError},
        consts::BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION,
        service::BaseNodeServiceConfig,
    },
    blocks::BlockHeader,
    chain_storage::{BlockAddResult, DbTransaction},
    consensus::ConsensusConstants,
    mempool::MempoolServiceConfig,
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        helpers::{create_test_kernel, create_utxo, schema_to_transaction},
        tari_amount::{uT, MicroTari, T},
        types::CryptoFactories,
    },
    txn_schema,
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::services::liveness::LivenessConfig;
use tari_test_utils::random::string;
use tari_utilities::hash::Hashable;
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn request_response_get_metadata() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());
    let (block0, _) = create_genesis_block(&bob_node.blockchain_db, &factories);
    bob_node.blockchain_db.add_block(block0).unwrap();
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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());

    let mut headerb1 = BlockHeader::new(0);
    headerb1.height = 1;
    let mut headerb2 = BlockHeader::new(0);
    headerb2.height = 2;
    let mut txn = DbTransaction::new();
    txn.insert_header(headerb1.clone());
    txn.insert_header(headerb2.clone());
    assert!(bob_node.blockchain_db.commit(txn).is_ok());

    let mut headerc1 = BlockHeader::new(0);
    headerc1.height = 1;
    let mut headerc2 = BlockHeader::new(0);
    headerc2.height = 2;
    let mut txn = DbTransaction::new();
    txn.insert_header(headerc1.clone());
    txn.insert_header(headerc2.clone());
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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());

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
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());

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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, mut bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());
    let factories = CryptoFactories::default();
    let db = &mut bob_node.blockchain_db;
    let (block0, _) = create_genesis_block(db, &factories);
    db.add_block(block0.clone()).expect("Could not add Genesis block");
    let mut blocks = vec![block0];
    generate_block(db, &mut blocks, vec![]).unwrap();
    generate_block(db, &mut blocks, vec![]).unwrap();
    generate_block(db, &mut blocks, vec![]).unwrap();

    carol_node.blockchain_db.add_block(blocks[0].clone()).unwrap();
    carol_node.blockchain_db.add_block(blocks[1].clone()).unwrap();
    carol_node.blockchain_db.add_block(blocks[2].clone()).unwrap();

    runtime.block_on(async {
        let received_blocks = alice_node.outbound_nci.fetch_blocks(vec![0]).await.unwrap();
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(*received_blocks[0].block(), blocks[0]);

        let received_blocks = alice_node.outbound_nci.fetch_blocks(vec![0, 1]).await.unwrap();
        assert_eq!(received_blocks.len(), 2);
        assert_ne!(*received_blocks[0].block(), *received_blocks[1].block());
        assert!((*received_blocks[0].block() == blocks[0]) || (*received_blocks[1].block() == blocks[0]));
        assert!((*received_blocks[0].block() == blocks[1]) || (*received_blocks[1].block() == blocks[1]));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

pub async fn event_stream_next<TStream>(mut stream: TStream, timeout: Duration) -> Option<TStream::Item>
where TStream: Stream + FusedStream + Unpin {
    let either = future::select(stream.select_next_some(), tokio::time::delay_for(timeout).fuse()).await;

    match either {
        Either::Left((v, _)) => Some(v),
        Either::Right(_) => None,
    }
}

#[test]
fn propagate_and_forward_valid_block() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let factories = CryptoFactories::default();
    // Alice will propagate block to bob, bob will receive it, verify it and then propagate it to carol and dan. Dan and
    // Carol will also try to propagate the block to each other, as they dont know that bob sent it to the other node.
    // These duplicate blocks will be discarded and wont be propagated again.
    //              /-> carol <-\
    //             /             |
    // alice -> bob              |
    //             \             |
    //              \->  dan  <-/
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let dan_node_identity = random_node_identity();
    let mut alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![
            alice_node_identity,
            carol_node_identity.clone(),
            dan_node_identity.clone(),
        ])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    let carol_node = BaseNodeBuilder::new()
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone(), dan_node_identity.clone()])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    let dan_node = BaseNodeBuilder::new()
        .with_node_identity(dan_node_identity)
        .with_peers(vec![bob_node_identity, carol_node_identity])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());

    let db = &alice_node.blockchain_db;
    let (block0, _) = create_genesis_block(db, &factories);
    db.add_block(block0.clone()).unwrap();
    let block1 = append_block(db, &block0, vec![]).unwrap();
    let block1_hash = block1.hash();

    bob_node.blockchain_db.add_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_block(block0.clone()).unwrap();
    dan_node.blockchain_db.add_block(block0.clone()).unwrap();

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
            panic!("Bob's node did not receive and validate the expected block");
        }
        if let BlockEvent::Verified((received_block, _block_add_result)) = &*carol_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            panic!("Carol's node did not receive and validate the expected block");
        }
        if let BlockEvent::Verified((received_block, _block_add_result)) = &*dan_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            panic!("Dan's node did not receive and validate the expected block");
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
    dan_node.comms.shutdown().unwrap();
}

#[test]
fn propagate_and_forward_invalid_block() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let factories = CryptoFactories::default();
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
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity.clone(), dan_node_identity.clone()])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    let carol_node = BaseNodeBuilder::new()
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![alice_node_identity, dan_node_identity.clone()])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    let dan_node = BaseNodeBuilder::new()
        .with_node_identity(dan_node_identity)
        .with_peers(vec![bob_node_identity, carol_node_identity])
        .start(&mut runtime, temp_dir.path().to_str().unwrap());

    let db = &alice_node.blockchain_db;
    let (block0, _) = create_genesis_block(db, &factories);
    db.add_block(block0.clone()).unwrap();
    let mut block1 = append_block(db, &block0, vec![]).unwrap();

    bob_node.blockchain_db.add_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_block(block0.clone()).unwrap();

    // Make block 1 invalid
    block1.header.height = 0;
    let block1_hash = block1.hash();
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

        if let BlockEvent::Invalid((received_block, _err)) = &*bob_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            panic!("Bob's node should have detected an invalid block");
        }
        if let BlockEvent::Invalid((received_block, _err)) = &*carol_block_event.unwrap() {
            assert_eq!(received_block.hash(), block1_hash);
        } else {
            panic!("Carol's node should have detected an invalid block");
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
    let mut runtime = Runtime::new().unwrap();
    let base_node_service_config = BaseNodeServiceConfig {
        request_timeout: Duration::from_millis(1),
        desired_response_fraction: BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION,
    };
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        base_node_service_config,
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let factories = CryptoFactories::default();
    let mut node = BaseNodeBuilder::new().start(&mut runtime, temp_dir.path().to_str().unwrap());
    let db = &node.blockchain_db;
    let (block0, _) = create_genesis_block(db, &factories);
    db.add_block(block0.clone()).unwrap();
    let block1 = append_block(db, &block0, vec![]).unwrap();
    let block2 = append_block(db, &block1, vec![]).unwrap();

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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mut node = BaseNodeBuilder::new().start(&mut runtime, temp_dir.path().to_str().unwrap());

    let db = &node.blockchain_db;
    let (block0, outputs) = create_genesis_block_with_utxos(db, &factories, &[T, T]);
    // Override coinbase maturity. This only works here because we're using mock validators
    db.add_block(block0.clone()).unwrap();
    let schema = [
        txn_schema!(from: vec![outputs[1].clone()], to: vec![10_000 * uT, 20_000 * uT]),
        txn_schema!(from: vec![outputs[2].clone()], to: vec![30_000 * uT, 40_000 * uT]),
    ];
    let (txs, _) = schema_to_transaction(&schema);
    assert!(node.mempool.insert(txs[0].clone()).is_ok());
    assert!(node.mempool.insert(txs[1].clone()).is_ok());

    runtime.block_on(async {
        let block_template = node.local_nci.get_new_block_template().await.unwrap();
        assert_eq!(block_template.header.height, 1);
        assert_eq!(block_template.body.kernels().len(), 2);

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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let factories = CryptoFactories::default();
    let mut node = BaseNodeBuilder::new().start(&mut runtime, temp_dir.path().to_str().unwrap());

    let db = &node.blockchain_db;
    let (block0, _) = create_genesis_block(db, &factories);
    db.add_block(block0.clone()).unwrap();
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

        let block1 = chain_block(&block0, Vec::new());
        let mut block1 = node.blockchain_db.calculate_mmr_roots(block1).unwrap();
        block1.header.timestamp = block0
            .header
            .timestamp
            .increase(ConsensusConstants::current().get_target_block_interval());
        block1.header.pow.pow_algo = PowAlgorithm::Blake;
        node.blockchain_db.add_block(block1).unwrap();
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
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let factories = CryptoFactories::default();
    let mut node = BaseNodeBuilder::new().start(&mut runtime, temp_dir.path().to_str().unwrap());

    let db = &node.blockchain_db;
    let (block0, _) = create_genesis_block(db, &factories);
    db.add_block(block0.clone()).unwrap();
    let block1 = db.calculate_mmr_roots(chain_block(&block0, vec![])).unwrap();
    runtime.block_on(async {
        assert!(node.local_nci.submit_block(block1.clone()).await.is_ok());

        let event_stream = node.local_nci.get_block_event_stream_fused();
        let event = event_stream_next(event_stream, Duration::from_millis(20000)).await;

        if let BlockEvent::Verified((received_block, result)) = &*event.unwrap() {
            assert_eq!(received_block.hash(), block1.hash());
            assert_eq!(*result, BlockAddResult::Ok);
        } else {
            panic!("Block validation failed");
        }
    });

    node.comms.shutdown().unwrap();
}
