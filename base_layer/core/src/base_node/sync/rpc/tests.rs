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

use std::{collections::HashMap, sync::Arc};

use futures::StreamExt;
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

use super::BaseNodeSyncRpcService;
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

    #[tokio::test]
    async fn it_sends_conflict_on_reorg() {
        let request_mock = RpcRequestMock::new(create_peer_manager());
        let db = create_new_blockchain();
        let (req_tx, _) = reply_channel::unbounded();
        let (block_tx, _) = reply_channel::unbounded();
        let (block_event_tx, _) = broadcast::channel(1);
        let local_interface = LocalNodeCommsInterface::new(req_tx, block_tx, block_event_tx);
        let service = BaseNodeSyncRpcService::new(
            db.clone().into(),
            local_interface.clone(),
            BlockchainSyncConfig::default().rpc_block_batch_size,
        );

        let (_, chain) = create_main_chain(&db, block_specs!(["A->GB"], ["B->A"], ["C->B"], ["D->C"], ["E->D"]));

        let msg = SyncBlocksRequest {
            start_hash: chain.get("A").unwrap().hash().to_vec(),
            end_hash: chain.get("E").unwrap().hash().to_vec(),
        };
        let req = request_mock.request_with_context(Default::default(), msg);
        let mut streaming = service.sync_blocks(req).await.unwrap().into_inner();

        // The sync worker is spawned but has not yet run on this single threaded runtime, so it sees this rewind
        // before sending any blocks
        let removed = vec![chain.get("C").unwrap().clone()];
        assert_eq!(
            local_interface.publish_block_event(BlockEvent::BlockSyncRewind(removed)),
            1
        );

        let responses = convert_mpsc_to_stream(&mut streaming).collect::<Vec<_>>().await;
        assert_eq!(responses.len(), 1);
        let err = responses.into_iter().next().unwrap().unwrap_err();
        unpack_enum!(RpcStatusCode::Conflict = err.as_status_code());
        assert_eq!(err.details(), "Reorg at height 3 detected");
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
