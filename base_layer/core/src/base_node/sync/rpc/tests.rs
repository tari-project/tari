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

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::StreamExt;
use prost::Message;
use tari_common::configuration::Network;
use tari_comms::protocol::rpc::{RpcStatusCode, mock::RpcRequestMock};
use tari_node_components::blocks::ChainBlock;
use tari_service_framework::reply_channel;
use tari_test_utils::{streams::convert_mpsc_to_stream, unpack_enum};
use tari_transaction_components::{
    tari_amount::T,
    test_helpers::schema_to_transaction,
    transaction_components::RangeProofType,
    txn_schema,
};
use tari_utilities::ByteArray;
use tokio::sync::broadcast;

use super::{BaseNodeSyncRpcService, service::is_on_requested_chain};
use crate::{
    base_node::{
        BaseNodeSyncService,
        LocalNodeCommsInterface,
        comms_interface::BlockEvent,
        sync::BlockchainSyncConfig,
    },
    chain_storage::BlockchainDatabase,
    consensus::BaseNodeConsensusManager,
    proto,
    proto::base_node::{SyncBlocksRequest, SyncKernelsRequest, SyncUtxosRequest, sync_utxos_response::Txo},
    test_helpers::{
        BlockSpec,
        BlockSpecs,
        blockchain::{
            TempDatabase,
            TestBlockchain,
            create_chained_blocks,
            create_main_chain,
            create_main_chain_with_range_proof_type,
            create_new_blockchain,
        },
        create_peer_manager,
    },
};

fn setup() -> (
    BaseNodeSyncRpcService<TempDatabase>,
    BlockchainDatabase<TempDatabase>,
    RpcRequestMock,
) {
    setup_with_block_batch_size(BlockchainSyncConfig::default().rpc_block_batch_size)
}

fn setup_with_block_batch_size(
    block_batch_size: usize,
) -> (
    BaseNodeSyncRpcService<TempDatabase>,
    BlockchainDatabase<TempDatabase>,
    RpcRequestMock,
) {
    setup_with(create_new_blockchain(), block_batch_size)
}

fn setup_with_db(
    db: BlockchainDatabase<TempDatabase>,
) -> (
    BaseNodeSyncRpcService<TempDatabase>,
    BlockchainDatabase<TempDatabase>,
    RpcRequestMock,
) {
    setup_with(db, BlockchainSyncConfig::default().rpc_block_batch_size)
}

fn setup_with(
    db: BlockchainDatabase<TempDatabase>,
    block_batch_size: usize,
) -> (
    BaseNodeSyncRpcService<TempDatabase>,
    BlockchainDatabase<TempDatabase>,
    RpcRequestMock,
) {
    let peer_manager = create_peer_manager();
    let request_mock = RpcRequestMock::new(peer_manager);

    let (req_tx, _) = reply_channel::unbounded();
    let (block_tx, _) = reply_channel::unbounded();
    let (block_event_tx, _) = broadcast::channel(1);
    let service = BaseNodeSyncRpcService::new(
        db.clone().into(),
        LocalNodeCommsInterface::new(req_tx, block_tx, block_event_tx),
        block_batch_size,
    );
    (service, db, request_mock)
}

/// Creates a main chain of 105 blocks (B1 to B105), more than one chunk of headers, so that streaming it makes the
/// header prefetch refetch.
fn create_chain_longer_than_a_header_chunk(
    db: &BlockchainDatabase<TempDatabase>,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let num_blocks = 105;
    let mut specs = Vec::with_capacity(num_blocks);
    for i in 1..=num_blocks {
        let name = if i == 1 {
            "B1->GB".to_string()
        } else {
            format!("B{}->B{}", i, i.saturating_sub(1))
        };
        specs.push(
            BlockSpec::builder()
                .with_name(Box::leak(name.into_boxed_str()))
                .finish(),
        );
    }
    let (names, chain) =
        create_main_chain_with_range_proof_type(db, BlockSpecs::from(specs), Some(RangeProofType::RevealedValue));
    assert_eq!(chain.get(names.last().unwrap()).unwrap().height(), num_blocks as u64);
    (names, chain)
}

mod sync_blocks {
    use super::*;

    #[tokio::test]
    async fn it_returns_not_found_if_unknown_hash() {
        let (service, _, rpc_request_mock) = setup();
        let msg = SyncBlocksRequest {
            start_hash: vec![0; 32],
            end_hash: vec![0; 32],
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let err = service.sync_blocks(req).await.unwrap_err();
        unpack_enum!(RpcStatusCode::NotFound = err.as_status_code());
    }

    #[tokio::test]
    async fn it_sends_bad_request_on_bad_response() {
        let (service, db, rpc_request_mock) = setup();

        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"]));

        let block = chain.get("A").unwrap();
        let msg = SyncBlocksRequest {
            start_hash: block.hash().to_vec(),
            end_hash: block.hash().to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        assert!(service.sync_blocks(req).await.is_err());
    }

    #[tokio::test]
    async fn it_streams_blocks_until_end() {
        let (service, db, rpc_request_mock) = setup();

        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));

        let first_block = chain.get("A").unwrap();
        let last_block = chain.get("E").unwrap();

        let msg = SyncBlocksRequest {
            start_hash: first_block.hash().to_vec(),
            end_hash: last_block.hash().to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap().into_inner();
        let blocks = convert_mpsc_to_stream(&mut streaming)
            .map(|block| block.unwrap())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(blocks.len(), 4);
        blocks.iter().zip(["B", "C", "D", "E"]).for_each(|(block, name)| {
            assert_eq!(*chain.get(name).unwrap().hash(), block.hash);
        });
    }

    #[tokio::test]
    async fn it_streams_all_blocks_for_any_batch_size() {
        // A batch size of 0 is clamped to 1, and batch sizes that do not divide the range evenly or exceed it must
        // still stream every block exactly once
        for block_batch_size in [0, 1, 3, 10] {
            let (service, db, rpc_request_mock) = setup_with_block_batch_size(block_batch_size);

            let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));

            let msg = SyncBlocksRequest {
                start_hash: chain.get("A").unwrap().hash().to_vec(),
                end_hash: chain.get("E").unwrap().hash().to_vec(),
            };
            let req = rpc_request_mock.request_with_context(Default::default(), msg);
            let mut streaming = service.sync_blocks(req).await.unwrap().into_inner();
            let blocks = convert_mpsc_to_stream(&mut streaming)
                .map(|block| block.unwrap())
                .collect::<Vec<_>>()
                .await;

            assert_eq!(blocks.len(), 4, "block_batch_size = {block_batch_size}");
            blocks.iter().zip(["B", "C", "D", "E"]).for_each(|(block, name)| {
                assert_eq!(*chain.get(name).unwrap().hash(), block.hash);
            });
        }
    }

    /// The bytes a session counts (the blocks queued and the last block taken) stay within its byte budget plus one
    /// block, however many blocks the batch size allows. The RPC server may also hold one older block while it writes
    /// it.
    #[tokio::test]
    async fn it_buffers_at_most_the_byte_budget_plus_one_block() {
        const BLOCK_BATCH_SIZE: usize = 10;
        let (service, db, rpc_request_mock) = setup_with_block_batch_size(BLOCK_BATCH_SIZE);
        let (names, chain) = create_chain_longer_than_a_header_chunk(&db);
        // Stream B2 to B31
        let num_blocks = 30usize;
        let sizes = (2..=num_blocks as u64 + 1)
            .map(|height| {
                let block = db.fetch_block(height, true).unwrap().into_block();
                proto::base_node::BlockBodyResponse::try_from(block)
                    .unwrap()
                    .encoded_len()
            })
            .collect::<Vec<_>>();
        let max_block_size = sizes.iter().copied().max().unwrap();
        // Room for about three blocks, far fewer than the batch size
        let max_buffered_bytes = max_block_size.saturating_mul(3);
        let service = service.with_max_buffered_block_bytes(max_buffered_bytes);

        let msg = SyncBlocksRequest {
            start_hash: chain.get(names.first().unwrap().as_str()).unwrap().hash().to_vec(),
            end_hash: chain
                .get(names.get(num_blocks).unwrap().as_str())
                .unwrap()
                .hash()
                .to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap().into_inner();

        let mut num_taken = 0usize;
        loop {
            // Let the session fetch as far ahead as it may
            tokio::time::sleep(Duration::from_millis(100)).await;
            let num_queued = streaming.len();
            assert!(num_queued < BLOCK_BATCH_SIZE, "{num_queued} blocks queued");
            // The session counts the blocks queued and the last block taken, which the RPC server may still hold
            let first_held = num_taken.saturating_sub(1);
            let held = sizes
                .get(first_held..num_taken.saturating_add(num_queued))
                .unwrap()
                .iter()
                .sum::<usize>();
            assert!(
                held <= max_buffered_bytes.saturating_add(max_block_size),
                "{held} bytes held, more than {max_buffered_bytes} + {max_block_size}"
            );
            match streaming.recv().await {
                Some(block) => {
                    let block = block.unwrap();
                    assert_eq!(Some(&block.encoded_len()), sizes.get(num_taken));
                    num_taken = num_taken.saturating_add(1);
                },
                None => break,
            }
        }
        assert_eq!(num_taken, num_blocks);
    }

    /// Sets up a session over A to E whose byte budget makes it wait after each block, and waits until it has queued B
    async fn start_session_waiting_after_b() -> (
        BlockchainDatabase<TempDatabase>,
        HashMap<String, Arc<ChainBlock>>,
        LocalNodeCommsInterface,
        tokio::sync::mpsc::Receiver<Result<proto::base_node::BlockBodyResponse, tari_comms::protocol::rpc::RpcStatus>>,
    ) {
        let request_mock = RpcRequestMock::new(create_peer_manager());
        let db = create_new_blockchain();
        let (req_tx, _) = reply_channel::unbounded();
        let (block_tx, _) = reply_channel::unbounded();
        let (block_event_tx, _) = broadcast::channel(1);
        let local_interface = LocalNodeCommsInterface::new(req_tx, block_tx, block_event_tx);
        // Any block fills the budget, so the session waits after each block until the previous one is taken
        let service = BaseNodeSyncRpcService::new(
            db.clone().into(),
            local_interface.clone(),
            BlockchainSyncConfig::default().rpc_block_batch_size,
        )
        .with_max_buffered_block_bytes(1);

        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));

        let msg = SyncBlocksRequest {
            start_hash: chain.get("A").unwrap().hash().to_vec(),
            end_hash: chain.get("E").unwrap().hash().to_vec(),
        };
        let req = request_mock.request_with_context(Default::default(), msg);
        let streaming = service.sync_blocks(req).await.unwrap().into_inner();

        tokio::time::timeout(Duration::from_secs(10), async {
            while streaming.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(streaming.len(), 1);
        (db, chain, local_interface, streaming)
    }

    /// Reads the rest of the session and checks that it is B followed by a Conflict
    async fn assert_b_then_conflict(
        chain: &HashMap<String, Arc<ChainBlock>>,
        streaming: &mut tokio::sync::mpsc::Receiver<
            Result<proto::base_node::BlockBodyResponse, tari_comms::protocol::rpc::RpcStatus>,
        >,
    ) -> String {
        let responses = convert_mpsc_to_stream(streaming).collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 2, "{responses:?}");
        let mut responses = responses.into_iter();
        let block = responses.next().unwrap().unwrap();
        assert_eq!(*chain.get("B").unwrap().hash(), block.hash);
        let err = responses.next().unwrap().unwrap_err();
        unpack_enum!(RpcStatusCode::Conflict = err.as_status_code());
        err.details().to_string()
    }

    /// A reorg whose event never reaches the session is still caught: the next block does not follow the last one sent
    #[tokio::test]
    async fn it_sends_conflict_when_the_chain_changes_without_an_event() {
        let (db, chain, _local_interface, mut streaming) = start_session_waiting_after_b().await;

        // A longer fork from A replaces B to E, and no event is published
        let (fork_names, fork) = create_chained_blocks(
            &db,
            block_specs!(["B2->GB"], ["C2->B2"], ["D2->C2"], ["E2->D2"], ["F2->E2"]),
            chain.get("A").unwrap().clone(),
        );
        for name in &fork_names {
            db.add_block(fork.get(name).unwrap().to_arc_block()).unwrap();
        }
        assert_eq!(db.fetch_block(2, true).unwrap().hash(), fork.get("B2").unwrap().hash());

        let details = assert_b_then_conflict(&chain, &mut streaming).await;
        assert_eq!(
            details,
            "Reorg detected: the requested end block is no longer on the chain"
        );
    }

    /// A reorg above the last block sent, whose event never reaches the session, is caught too: the next block still
    /// follows the last one sent, but the chain no longer ends at the requested end block
    #[tokio::test]
    async fn it_sends_conflict_when_the_chain_changes_above_the_last_block_sent_without_an_event() {
        let (db, chain, _local_interface, mut streaming) = start_session_waiting_after_b().await;

        // A longer fork from B replaces C to E, and no event is published
        let (fork_names, fork) = create_chained_blocks(
            &db,
            block_specs!(["C2->GB"], ["D2->C2"], ["E2->D2"], ["F2->E2"]),
            chain.get("B").unwrap().clone(),
        );
        for name in &fork_names {
            db.add_block(fork.get(name).unwrap().to_arc_block()).unwrap();
        }
        let next = db.fetch_block(3, true).unwrap();
        assert_eq!(next.hash(), fork.get("C2").unwrap().hash());
        assert_eq!(next.header().prev_hash, *chain.get("B").unwrap().hash());

        let details = assert_b_then_conflict(&chain, &mut streaming).await;
        assert_eq!(
            details,
            "Reorg detected: the requested end block is no longer on the chain"
        );
    }

    /// A block that is gone because the chain was rewound is a Conflict, not an internal error
    #[tokio::test]
    async fn it_sends_conflict_when_a_block_is_gone() {
        let (db, chain, _local_interface, mut streaming) = start_session_waiting_after_b().await;

        db.rewind_to_height(2).unwrap();

        let details = assert_b_then_conflict(&chain, &mut streaming).await;
        assert_eq!(details, "Block #3 is no longer on the chain");
    }

    /// Block events play no part: a reorg event that leaves the requested chain intact (e.g. a rewind whose blocks were
    /// re-applied) does not end the session
    #[tokio::test]
    async fn it_keeps_serving_when_a_reorg_event_leaves_the_requested_chain_intact() {
        let (_db, chain, local_interface, mut streaming) = start_session_waiting_after_b().await;

        let removed = vec![chain.get("C").unwrap().clone(), chain.get("D").unwrap().clone()];
        local_interface.publish_block_event(BlockEvent::BlockSyncRewind(removed));

        let responses = convert_mpsc_to_stream(&mut streaming).collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 4, "{responses:?}");
        for (response, name) in responses.into_iter().zip(["B", "C", "D", "E"]) {
            assert_eq!(*chain.get(name).unwrap().hash(), response.unwrap().hash);
        }
    }

    /// The check after each fetch accepts the fetched block only if the same chain state has it at its height and has
    /// the requested end block at the end height
    #[test]
    fn a_fetched_block_is_served_only_if_it_is_on_the_requested_chain() {
        let db = create_new_blockchain();
        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"]));
        let (_, fork) = create_chained_blocks(
            &db,
            block_specs!(["B2->GB"], ["C2->B2"]),
            chain.get("A").unwrap().clone(),
        );
        let b = chain.get("B").unwrap();
        let c = chain.get("C").unwrap();
        let b2 = fork.get("B2").unwrap();
        let c2 = fork.get("C2").unwrap();

        assert!(is_on_requested_chain(
            b.hash(),
            Some(b.header()),
            Some(c.header()),
            c.hash()
        ));
        // The block at the fetched height was replaced after the fetch
        assert!(!is_on_requested_chain(
            b.hash(),
            Some(b2.header()),
            Some(c.header()),
            c.hash()
        ));
        // The chain no longer ends at the requested end block
        assert!(!is_on_requested_chain(
            b.hash(),
            Some(b.header()),
            Some(c2.header()),
            c.hash()
        ));
        // Either block is gone
        assert!(!is_on_requested_chain(b.hash(), None, Some(c.header()), c.hash()));
        assert!(!is_on_requested_chain(b.hash(), Some(b.header()), None, c.hash()));
    }
}

mod sync_utxos {
    use super::*;

    #[tokio::test]
    async fn it_returns_not_found_if_unknown_hash() {
        let (service, db, rpc_request_mock) = setup();
        let gen_block_hash = db.fetch_header(0).unwrap().unwrap().hash();
        let msg = SyncUtxosRequest {
            start_header_hash: gen_block_hash.to_vec(),
            end_header_hash: vec![0; 32],
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let err = service.sync_utxos(req).await.unwrap_err();
        unpack_enum!(RpcStatusCode::NotFound = err.as_status_code());
    }

    #[tokio::test]
    async fn it_returns_not_found_if_start_not_found() {
        let (service, db, rpc_request_mock) = setup();
        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"]));
        let gb = chain.get("GB").unwrap();
        let msg = SyncUtxosRequest {
            start_header_hash: vec![0; 32],
            end_header_hash: gb.hash().to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let err = service.sync_utxos(req).await.unwrap_err();
        unpack_enum!(RpcStatusCode::NotFound = err.as_status_code());
    }

    #[tokio::test]
    async fn it_only_streams_spent_commitments_for_outputs_mined_before_the_start() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let mut blockchain = TestBlockchain::create(rules);
        // A and B are mined before the requested range
        let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
        let (_, coinbase_b) = blockchain.add_next_tip(block_spec!("B")).unwrap();

        // C spends coinbase A, which was mined before the range
        let schema = txn_schema!(from: vec![coinbase_a], to: vec![1 * T]);
        let (txs, outputs_c) = schema_to_transaction(&[schema], &blockchain.km);
        let txs = txs.iter().map(|t| (**t).clone()).collect();
        let (block_c, _) = blockchain.add_next_tip(block_spec!("C", transactions: txs)).unwrap();

        // D spends an output of C, so it is both mined and spent inside the range
        let schema = txn_schema!(from: vec![outputs_c.first().unwrap().clone()], to: vec![1 * T / 2]);
        let (txs, _) = schema_to_transaction(&[schema], &blockchain.km);
        let txs = txs.iter().map(|t| (**t).clone()).collect();
        let (block_d, _) = blockchain.add_next_tip(block_spec!("D", transactions: txs)).unwrap();

        // E spends coinbase B, which was mined before the range
        let schema = txn_schema!(from: vec![coinbase_b], to: vec![1 * T]);
        let (txs, _) = schema_to_transaction(&[schema], &blockchain.km);
        let txs = txs.iter().map(|t| (**t).clone()).collect();
        let (block_e, _) = blockchain.add_next_tip(block_spec!("E", transactions: txs)).unwrap();

        let input_commitment = |block: &ChainBlock| {
            let inputs = block.block().body.inputs();
            assert_eq!(inputs.len(), 1);
            inputs.first().unwrap().commitment().unwrap().as_bytes().to_vec()
        };
        let spent_before_start_c = input_commitment(&block_c);
        let spent_inside_range = input_commitment(&block_d);
        let spent_before_start_e = input_commitment(&block_e);

        let (service, _, rpc_request_mock) = setup_with_db(blockchain.db().clone());
        let msg = SyncUtxosRequest {
            start_header_hash: block_c.hash().to_vec(),
            end_header_hash: block_e.hash().to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_utxos(req).await.unwrap().into_inner();
        let responses = convert_mpsc_to_stream(&mut streaming)
            .map(|resp| resp.unwrap())
            .collect::<Vec<_>>()
            .await;

        let spent_commitments = responses
            .iter()
            .filter_map(|resp| match &resp.txo {
                Some(Txo::Commitment(commitment)) => Some((commitment.clone(), resp.mined_header.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(spent_commitments, vec![
            (spent_before_start_c, block_c.hash().to_vec()),
            (spent_before_start_e, block_e.hash().to_vec()),
        ]);
        // The output both mined and spent inside the range is never streamed, neither as a commitment nor as an
        // unspent output
        assert!(responses.iter().all(|resp| match &resp.txo {
            Some(Txo::Commitment(commitment)) => *commitment != spent_inside_range,
            Some(Txo::Output(output)) => output.commitment.as_ref().unwrap().data != spent_inside_range,
            None => true,
        }));

        let terminator = responses.last().unwrap();
        assert!(terminator.txo.is_none());
        assert_eq!(terminator.mined_header, block_e.hash().to_vec());
    }

    #[tokio::test]
    async fn it_streams_all_outputs_in_order_across_header_chunks() {
        let (service, db, rpc_request_mock) = setup();
        let (names, chain) = create_chain_longer_than_a_header_chunk(&db);
        let first_block = chain.get(names.first().unwrap()).unwrap();
        let last_block = chain.get(names.last().unwrap()).unwrap();

        let mut expected = Vec::new();
        for name in &names {
            let block = chain.get(name).unwrap();
            assert!(block.block().body.inputs().is_empty());
            for output in block.block().body.outputs() {
                expected.push((output.commitment.as_bytes().to_vec(), block.hash().to_vec()));
            }
        }

        let msg = SyncUtxosRequest {
            start_header_hash: first_block.hash().to_vec(),
            end_header_hash: last_block.hash().to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_utxos(req).await.unwrap().into_inner();
        let mut responses = convert_mpsc_to_stream(&mut streaming)
            .map(|resp| resp.unwrap())
            .collect::<Vec<_>>()
            .await;

        let terminator = responses.pop().unwrap();
        assert!(terminator.txo.is_none());
        assert_eq!(terminator.mined_header, last_block.hash().to_vec());

        let streamed = responses
            .into_iter()
            .map(|resp| match resp.txo {
                Some(Txo::Output(output)) => (output.commitment.unwrap().data, resp.mined_header),
                other => panic!("Expected an unspent output, got {:?}", other),
            })
            .collect::<Vec<_>>();
        assert_eq!(streamed, expected);
    }
}

mod sync_kernels {
    use super::*;

    #[tokio::test]
    async fn it_streams_all_kernels_in_order_across_header_chunks() {
        let (service, db, rpc_request_mock) = setup();

        let (names, chain) = create_chain_longer_than_a_header_chunk(&db);
        let last_block = chain.get(names.last().unwrap()).unwrap();

        let mut expected = Vec::new();
        for name in &names {
            let block = chain.get(name).unwrap();
            assert!(!block.block().body.kernels().is_empty());
            expected.extend(
                block
                    .block()
                    .body
                    .kernels()
                    .iter()
                    .cloned()
                    .map(proto::types::TransactionKernel::from),
            );
        }

        // The first kernel after the genesis block is in block 1
        let genesis = db.fetch_header(0).unwrap().unwrap();
        let msg = SyncKernelsRequest {
            start: genesis.kernel_mmr_size,
            end_header_hash: last_block.hash().to_vec(),
        };
        let req = rpc_request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_kernels(req).await.unwrap().into_inner();
        let kernels = convert_mpsc_to_stream(&mut streaming)
            .map(|kernel| kernel.unwrap())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(kernels.len(), expected.len());
        assert_eq!(kernels, expected);
    }
}
