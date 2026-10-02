//  Copyright 2022. The Tari Project
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

#![allow(clippy::indexing_slicing)]
use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use tari_common::configuration::Network;
use tari_common_types::types::{FixedHash, PrivateKey};
use tari_comms::{peer_manager::NodeId, test_utils::mocks::create_connectivity_mock};
use tari_core::{
    base_node::comms_interface::{
        CommsInterfaceError,
        GetNewBlockTemplateRequest,
        InboundNodeCommsHandlers,
        NodeCommsRequest,
        NodeCommsResponse,
        OutboundNodeCommsInterface,
    },
    chain_storage::{
        BlockAddResult,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        ChainStorageError,
        DbTransaction,
        Validators,
        body_matches_header,
        inputs_and_outputs_match_header,
    },
    consensus::{BaseNodeConsensusManager, BaseNodeConsensusManagerBuilder},
    mempool::{Mempool, MempoolConfig},
    proof_of_work::randomx_factory::RandomXFactory,
    test_helpers::{
        blockchain::{TempDatabase, create_store_with_consensus_and_validators_and_config, create_test_blockchain_db},
        create_consensus_rules,
    },
    validation::{ValidationError, mocks::MockValidator, transaction::TransactionChainLinkedValidator},
};
use tari_node_components::blocks::{Block, BlockHeader, ChainBlock, NewBlock};
use tari_script::{ExecutionStack, script};
use tari_service_framework::reply_channel;
use tari_transaction_components::{
    BanPeriod,
    MicroMinotari,
    aggregated_body::AggregateBody,
    consensus::emission::Emission,
    key_manager::KeyManager,
    tari_amount::T,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
    test_helpers::{create_utxo, schema_to_transaction},
    transaction_components::{
        SpentOutput,
        Transaction,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
        WalletOutput,
        covenants::Covenant,
    },
    txn_schema,
};
use tokio::sync::{broadcast, mpsc};

use crate::helpers::{
    block_builders::{
        append_block,
        chain_block_with_coinbase,
        create_chain_header,
        create_coinbase,
        find_header_with_achieved_difficulty,
    },
    sample_blockchains::{consensus_constants, create_new_blockchain},
};

fn new_mempool() -> Mempool {
    let rules = create_consensus_rules();
    let mempool_validator = MockValidator::new(true);
    Mempool::new(MempoolConfig::default(), rules, Box::new(mempool_validator))
}

#[tokio::test]
async fn inbound_get_metadata() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();

    let network = Network::LocalNet;
    let consensus_manager = BaseNodeConsensusManager::builder(network).build().unwrap();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender.clone());
    let randomx_factory = RandomXFactory::new(2);
    let (connectivity, _) = create_connectivity_mock();
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
        connectivity,
        randomx_factory,
    );
    let block = store.fetch_block(0, true).unwrap().block().clone();

    if let Ok(NodeCommsResponse::ChainMetadata(received_metadata)) =
        inbound_nch.handle_request(NodeCommsRequest::GetChainMetadata).await
    {
        assert_eq!(received_metadata.best_block_height(), 0);
        assert_eq!(received_metadata.best_block_hash(), &block.hash());
        assert_eq!(received_metadata.pruning_horizon(), 0);
    } else {
        panic!();
    }
}

#[tokio::test]
async fn inbound_fetch_kernel_by_excess_sig() {
    let network = Network::LocalNet;
    let (store, blocks, _outputs, consensus_manager, _key_manager) = create_new_blockchain(network);
    let mempool = new_mempool();

    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender.clone());
    let (connectivity, _) = create_connectivity_mock();
    let randomx_factory = RandomXFactory::new(2);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
        connectivity,
        randomx_factory,
    );
    let block = blocks[0].block().clone();
    let sig = block.body.kernels()[0].excess_sig.clone();

    if let Ok(NodeCommsResponse::TransactionKernels(received_kernels)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchKernelByExcessSig(sig))
        .await
    {
        assert_eq!(received_kernels.len(), 1);
        assert_eq!(received_kernels[0], block.body.kernels()[0]);
    } else {
        panic!("kernel not found");
    }
}

#[tokio::test]
async fn inbound_fetch_headers() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();
    let network = Network::LocalNet;
    let consensus_manager = BaseNodeConsensusManager::builder(network).build().unwrap();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let (connectivity, _) = create_connectivity_mock();
    let randomx_factory = RandomXFactory::new(2);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
        connectivity,
        randomx_factory,
    );
    let header = store.fetch_block(0, true).unwrap().header().clone();

    if let Ok(NodeCommsResponse::BlockHeaders(received_headers)) =
        inbound_nch.handle_request(NodeCommsRequest::FetchHeaders(0..=0)).await
    {
        assert_eq!(received_headers.len(), 1);
        assert_eq!(*received_headers[0].header(), header);
    } else {
        panic!();
    }
}

#[tokio::test]
async fn inbound_fetch_utxos() {
    let network = Network::LocalNet;
    let (store, blocks, _outputs, consensus_manager, _key_manager) = create_new_blockchain(network);
    let mempool = new_mempool();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let (connectivity, _) = create_connectivity_mock();
    let randomx_factory = RandomXFactory::new(2);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
        connectivity,
        randomx_factory,
    );

    let block0 = blocks[0].block().clone();
    let utxo_1 = block0.body.outputs()[0].clone();
    let hash_1 = utxo_1.hash();

    let key_manager = KeyManager::new_random().unwrap();
    let (utxo_2, _, _) = create_utxo(
        MicroMinotari(10_000),
        &key_manager,
        &Default::default(),
        &script!(Nop).unwrap(),
        &Covenant::default(),
        MicroMinotari::zero(),
    );
    let hash_2 = utxo_2.hash();

    // Only retrieve a subset of the actual hashes, including a fake hash in the list
    if let Ok(NodeCommsResponse::TransactionOutputs(received_utxos)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingUtxos(vec![hash_1, hash_2]))
        .await
    {
        assert_eq!(received_utxos.len(), 1);
        assert_eq!(received_utxos[0], utxo_1);
    } else {
        panic!();
    }
}

#[tokio::test]
async fn inbound_fetch_blocks() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();
    let (block_event_sender, _) = broadcast::channel(50);
    let network = Network::LocalNet;
    let consensus_manager = BaseNodeConsensusManager::builder(network).build().unwrap();
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let (connectivity, _) = create_connectivity_mock();
    let randomx_factory = RandomXFactory::new(2);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
        connectivity,
        randomx_factory,
    );
    let block = store.fetch_block(0, true).unwrap().block().clone();

    if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingBlocks {
            range: 0..=0,
            compact: true,
        })
        .await
    {
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(*received_blocks[0].block(), block);
    } else {
        panic!();
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn inbound_fetch_blocks_before_horizon_height() {
    let consensus_manager = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let block0 = consensus_manager.get_genesis_block();
    let key_manager = KeyManager::new_random().unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let config = BlockchainDatabaseConfig {
        pruning_horizon: 3,
        pruning_interval: 1,
        ..Default::default()
    };
    let store = create_store_with_consensus_and_validators_and_config(consensus_manager.clone(), validators, config);
    let mempool_validator = TransactionChainLinkedValidator::new(store.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let (connectivity, _) = create_connectivity_mock();
    let randomx_factory = RandomXFactory::new(2);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager.clone(),
        outbound_nci,
        connectivity,
        randomx_factory,
    );

    let (block1, _) = append_block(
        &store,
        &block0,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();
    let (block2, _) = append_block(
        &store,
        &block1,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();
    let (block3, _) = append_block(
        &store,
        &block2,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();
    let (block4, _) = append_block(
        &store,
        &block3,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();
    let (_block5, _) = append_block(
        &store,
        &block4,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();

    if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingBlocks {
            range: 1..=1,
            compact: true,
        })
        .await
    {
        assert_eq!(received_blocks.len(), 1);
    } else {
        panic!();
    }

    if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingBlocks {
            range: 2..=2,
            compact: true,
        })
        .await
    {
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0].block(), block2.block());
    } else {
        panic!();
    }
}

// A `GetNewBlockTemplate` request must not build a template on a stale tip. When the base node tip advances while the
// handler is waiting for the mempool to catch up, the handler should re-fetch the fresher tip and build the template on
// it. Here the mempool starts behind the tip (so the handler waits), then - mid-wait - a new block (height 3) is added
// to the chain and the mempool is advanced to it. The returned template must be built on the *new* tip: height 4 (the
// stale height-2 tip the handler first observed would have produced height 3). It must also be flagged mempool-in-sync.
#[tokio::test]
async fn inbound_get_new_block_template_refetches_advanced_tip() {
    let consensus_manager = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let block0 = consensus_manager.get_genesis_block();
    let key_manager = KeyManager::new_random().unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let store = create_store_with_consensus_and_validators_and_config(
        consensus_manager.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
    );
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(MockValidator::new(true)),
    );

    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let (connectivity, _) = create_connectivity_mock();
    let randomx_factory = RandomXFactory::new(2);

    // Advance the chain tip to height 2.
    let (block1, _) = append_block(
        &store,
        &block0,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();
    let (block2, _) = append_block(
        &store,
        &block1,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();

    // Put the mempool behind the tip (it has only seen the genesis block) so the handler has to wait. A default
    // last-seen hash would be treated as "in sync" and skip the wait entirely.
    mempool
        .process_published_block(Arc::new(block0.block().clone()))
        .await
        .unwrap();

    // `mempool` is cloned into the handler; the original is retained to drive it from the test. Both share the same
    // underlying storage and last-seen broadcast channel. The mempool-sync timeout is raised well above the production
    // default so the test waits for the mempool deterministically instead of racing the short production deadline (the
    // cause of CI flakes - the handler must not give up and build on the stale tip before the test advances the chain).
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool.clone(),
        consensus_manager.clone(),
        outbound_nci,
        connectivity,
        randomx_factory,
    )
    .with_mempool_sync_timeout(Duration::from_secs(60));

    let handle = tokio::spawn(async move {
        inbound_nch
            .handle_request(NodeCommsRequest::GetNewBlockTemplate(GetNewBlockTemplateRequest {
                algo: PowAlgorithm::Sha3x,
                max_weight: 0,
            }))
            .await
    });

    // Advance the chain to height 3 and notify the mempool of the new tip. The chain is advanced *before* the mempool
    // is notified, so the mempool can never be observed ahead of a stale tip. With the generous sync timeout above, the
    // handler reliably waits for this notification (or sees the advanced state on its first read), then builds the
    // template on the height-3 tip regardless of scheduling - no dependence on hitting a narrow timing window.
    tokio::task::yield_now().await;
    let (block3, _) = append_block(
        &store,
        &block2,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .unwrap();
    mempool
        .process_published_block(Arc::new(block3.block().clone()))
        .await
        .unwrap();

    let response = handle.await.unwrap().unwrap();
    let NodeCommsResponse::NewBlockTemplate(template) = response else {
        panic!("expected a NewBlockTemplate response");
    };
    // Built on the fresher tip (height 3 -> template height 4), not the stale height-2 tip (template height 3) the
    // handler first observed.
    assert_eq!(template.header.height, 4);
    assert!(template.is_mempool_in_sync);
}

/// A block on `prev` with the given transactions and a coinbase, ready to be added but not added
fn prepare_block(
    store: &BlockchainDatabase<TempDatabase>,
    prev: &ChainBlock,
    txs: Vec<Transaction>,
    rules: &BaseNodeConsensusManager,
    key_manager: &KeyManager,
) -> Block {
    prepare_block_with_extra_coinbase(store, prev, txs, rules, key_manager, MicroMinotari::zero())
}

/// As [prepare_block], with `extra` more in the coinbase than the block may claim
// Overflow in test code panics, which is the desired failure mode for a test.
#[allow(clippy::arithmetic_side_effects)]
fn prepare_block_with_extra_coinbase(
    store: &BlockchainDatabase<TempDatabase>,
    prev: &ChainBlock,
    txs: Vec<Transaction>,
    rules: &BaseNodeConsensusManager,
    key_manager: &KeyManager,
    extra: MicroMinotari,
) -> Block {
    let height = prev.height() + 1;
    let mut coinbase_value = rules.emission_schedule().block_reward(height) + extra;
    for tx in &txs {
        coinbase_value += tx.body.get_total_fee().unwrap();
    }
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        coinbase_value,
        height + rules.consensus_constants(0).coinbase_min_maturity(),
        None,
        key_manager,
    );
    let template = chain_block_with_coinbase(prev, txs, coinbase_utxo, coinbase_kernel, rules, None);
    let mut block = store.prepare_new_block(template).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::min());
    block
}

/// A transaction spending the genesis output into a few outputs
fn spend_genesis_output(outputs: &[Vec<WalletOutput>], key_manager: &KeyManager) -> Transaction {
    let schema = txn_schema!(from: vec![outputs[0][0].clone()], to: vec![T, T, T, T, T]);
    let (txs, _) = schema_to_transaction(&[schema], key_manager);
    (*txs[0]).clone()
}

fn new_handlers(
    store: &BlockchainDatabase<TempDatabase>,
    mempool: Mempool,
    rules: BaseNodeConsensusManager,
) -> InboundNodeCommsHandlers<TempDatabase> {
    let (request_sender, _) = reply_channel::unbounded();
    new_handlers_with_requests(store, mempool, rules, request_sender)
}

/// Handlers whose requests to peers are all answered with `served`
fn new_handlers_serving(
    store: &BlockchainDatabase<TempDatabase>,
    mempool: Mempool,
    rules: BaseNodeConsensusManager,
    served: Block,
) -> InboundNodeCommsHandlers<TempDatabase> {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    tokio::spawn(async move {
        while let Some(request_context) = request_receiver.next().await {
            let (_request, reply_tx) = request_context.split();
            let _ignore = reply_tx.send(Ok(NodeCommsResponse::Block(Box::new(Some(served.clone())))));
        }
    });
    new_handlers_with_requests(store, mempool, rules, request_sender)
}

fn new_handlers_with_requests(
    store: &BlockchainDatabase<TempDatabase>,
    mempool: Mempool,
    rules: BaseNodeConsensusManager,
    request_sender: reply_channel::SenderService<
        (NodeCommsRequest, Option<NodeId>),
        Result<NodeCommsResponse, CommsInterfaceError>,
    >,
) -> InboundNodeCommsHandlers<TempDatabase> {
    let (block_event_sender, _) = broadcast::channel(50);
    let (block_sender, _) = mpsc::unbounded_channel();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let (connectivity, _) = create_connectivity_mock();
    InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        rules,
        outbound_nci,
        connectivity,
        RandomXFactory::new(2),
    )
}

/// Orphans are stored with hydrated inputs, and an orphan that fits in the messaging frame is served that way, so a
/// peer on another chain can accept it without having the outputs it spends. (An orphan that does not fit is served
/// compact; that case is covered by the unit test of `orphan_block_to_serve`.)
#[tokio::test]
async fn inbound_get_block_from_all_chains_serves_small_orphans_hydrated() {
    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);
    let tx = spend_genesis_output(&outputs, &key_manager);
    let orphan = prepare_block(&store, &blocks[0], vec![tx], &rules, &key_manager);
    assert!(!orphan.body.inputs().is_empty());
    assert!(orphan.body.inputs().iter().all(|input| !input.is_compact()));
    // A stronger block at the same height makes `orphan` an orphan
    append_block(
        &store,
        &blocks[0],
        vec![],
        &rules,
        Difficulty::from_u64(10).unwrap(),
        &key_manager,
    )
    .unwrap();
    let result = store.add_block(Arc::new(orphan.clone())).unwrap();
    assert!(matches!(result, BlockAddResult::OrphanBlock), "{result:?}");
    assert!(
        store
            .fetch_orphan(orphan.hash())
            .unwrap()
            .body
            .inputs()
            .iter()
            .all(|input| !input.is_compact())
    );

    let handlers = new_handlers(&store, new_mempool(), rules);
    let response = handlers
        .handle_request(NodeCommsRequest::GetBlockFromAllChains(orphan.hash()))
        .await
        .unwrap();
    let NodeCommsResponse::Block(served) = response else {
        panic!("unexpected response {response}");
    };
    let served = (*served).expect("the orphan is served");
    assert!(served.body.inputs().iter().all(|input| !input.is_compact()));
    assert_eq!(served, orphan);
}

/// A compact block that is rebuilt from the mempool into a body over the consensus byte limit is rejected right
/// after it is rebuilt, before it is handed on.
#[tokio::test]
async fn a_compact_block_rebuilt_over_the_byte_limit_is_rejected() {
    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);
    let tx = spend_genesis_output(&outputs, &key_manager);
    let block = prepare_block(&store, &blocks[0], vec![tx.clone()], &rules, &key_manager);
    let body_bytes = block.body.compact_serialized_size().unwrap();
    let rules_with_limit = |max_bytes| {
        BaseNodeConsensusManagerBuilder::new(network)
            .add_consensus_constants(
                consensus_constants(network)
                    .with_max_block_body_bytes(max_bytes)
                    .build(),
            )
            .with_block(blocks[0].clone())
            .build()
            .unwrap()
    };

    // One byte over the limit: rejected, and the peer is banned
    let mempool = new_mempool();
    mempool.insert(Arc::new(tx.clone())).await.unwrap();
    let mut handlers = new_handlers(&store, mempool, rules_with_limit(body_bytes - 1));
    let err = handlers
        .handle_new_block_message(NewBlock::from(&block), NodeId::default())
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            CommsInterfaceError::ChainStorageError(ChainStorageError::ValidationError {
                source: ValidationError::BlockBodyTooManyBytes { actual_bytes, max_bytes }
            }) if *actual_bytes == body_bytes && *max_bytes == body_bytes - 1
        ),
        "{err:?}"
    );
    assert_eq!(err.get_ban_reason().map(|r| r.ban_duration), Some(BanPeriod::Long));
    assert_eq!(store.get_height().unwrap(), 0);

    // At the limit: rebuilt and added
    let mempool = new_mempool();
    mempool.insert(Arc::new(tx)).await.unwrap();
    let mut handlers = new_handlers(&store, mempool, rules_with_limit(body_bytes));
    handlers
        .handle_new_block_message(NewBlock::from(&block), NodeId::default())
        .await
        .unwrap();
    assert_eq!(store.get_height().unwrap(), 1);
}

/// A compact input is hydrated from the database, from an output created in the same block, or from a held orphan the
/// block builds on. An input that can be resolved from none of these makes a block on our main chain invalid (long ban)
/// and a block on a held orphan chain suspect (short ban), but a block on a chain we do not hold may spend outputs from
/// that chain that we have never seen, so the peer that sent it is not banned.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn compact_inputs_that_cannot_be_hydrated() {
    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);
    let tx = spend_genesis_output(&outputs, &key_manager);
    let block = prepare_block(&store, &blocks[0], vec![tx], &rules, &key_manager);
    let mut handlers = new_handlers(&store, new_mempool(), rules.clone());

    // An extra input spending an output of the same block is hydrated from the block. The block is then invalid for
    // other reasons (its MMR roots), but not because the input could not be hydrated.
    let (header, inputs, outputs_in_block, kernels) = block.clone().to_compact().dissolve();
    let mut with_in_block_spend = inputs.clone();
    with_in_block_spend.push(TransactionInput::new_current_version(
        SpentOutput::OutputHash(outputs_in_block[0].hash()),
        Default::default(),
        Default::default(),
    ));
    let in_block_spend = Block::new(
        header.clone(),
        AggregateBody::new_unsorted(with_in_block_spend, outputs_in_block.clone(), kernels.clone()),
    );
    let err = handlers.handle_block(in_block_spend, None).await.unwrap_err();
    assert!(
        matches!(
            err,
            CommsInterfaceError::ChainStorageError(ChainStorageError::ValidationError { .. })
        ),
        "{err:?}"
    );

    // An input spending an output we do not have
    let mut with_unknown_spend = inputs;
    with_unknown_spend.push(TransactionInput::new_current_version(
        SpentOutput::OutputHash(FixedHash::from([7u8; 32])),
        Default::default(),
        Default::default(),
    ));
    let unknown_spend = |prev_hash| {
        let mut header = header.clone();
        header.prev_hash = prev_hash;
        Block::new(
            header,
            AggregateBody::new_unsorted(with_unknown_spend.clone(), outputs_in_block.clone(), kernels.clone()),
        )
    };

    // On our tip: invalid, and the peer is banned
    let err = handlers
        .handle_block(unknown_spend(*blocks[0].hash()), None)
        .await
        .unwrap_err();
    assert!(matches!(err, CommsInterfaceError::InvalidFullBlock { .. }), "{err:?}");
    assert_eq!(err.get_ban_reason().map(|r| r.ban_duration), Some(BanPeriod::Long));

    // On a main chain block that is not the tip: still invalid, and the peer is banned
    let (block1, _) = append_block(
        &store,
        &blocks[0],
        vec![],
        &rules,
        Difficulty::from_u64(10).unwrap(),
        &key_manager,
    )
    .unwrap();
    assert_eq!(store.get_height().unwrap(), 1);
    let err = handlers
        .handle_block(unknown_spend(*blocks[0].hash()), None)
        .await
        .unwrap_err();
    assert!(matches!(err, CommsInterfaceError::InvalidFullBlock { .. }), "{err:?}");
    assert_eq!(err.get_ban_reason().map(|r| r.ban_duration), Some(BanPeriod::Long));

    // On an unknown parent: dropped without banning the peer
    let err = handlers
        .handle_block(unknown_spend(FixedHash::from([9u8; 32])), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CommsInterfaceError::UnknownSpentOutputs { .. }),
        "{err:?}"
    );
    assert!(err.get_ban_reason().is_none());

    // On a parent that is in the orphan pool: the held chain was searched and the output is not on it, so a short ban
    let orphan = prepare_block(&store, &block1, vec![], &rules, &key_manager);
    let mut orphan_header = orphan.header.clone();
    orphan_header.prev_hash = FixedHash::from([5u8; 32]);
    let orphan = Block::new(orphan_header, orphan.body);
    let result = store.add_block(Arc::new(orphan.clone())).unwrap();
    assert!(matches!(result, BlockAddResult::OrphanBlock), "{result:?}");
    let err = handlers
        .handle_block(unknown_spend(orphan.hash()), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CommsInterfaceError::UnknownSpentOutputs {
            held_orphan_parent: true,
            ..
        }),
        "{err:?}"
    );
    assert_eq!(err.get_ban_reason().map(|r| r.ban_duration), Some(BanPeriod::Short));

    // A pruned node deletes outputs spent at or below its pruned height
    let (block2, _) = append_block(
        &store,
        &block1,
        vec![],
        &rules,
        Difficulty::from_u64(10).unwrap(),
        &key_manager,
    )
    .unwrap();
    let mut txn = DbTransaction::new();
    txn.set_pruned_height(1);
    store.write(txn).unwrap();

    // Parent at the pruned height: every output it could spend is still held, so invalid, and the peer is banned
    let err = handlers
        .handle_block(unknown_spend(*block1.hash()), None)
        .await
        .unwrap_err();
    assert!(matches!(err, CommsInterfaceError::InvalidFullBlock { .. }), "{err:?}");
    assert_eq!(err.get_ban_reason().map(|r| r.ban_duration), Some(BanPeriod::Long));

    // Parent below the pruned height: the output may have been pruned, so dropped without banning the peer
    let err = handlers
        .handle_block(unknown_spend(*blocks[0].hash()), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CommsInterfaceError::UnknownSpentOutputs { .. }),
        "{err:?}"
    );
    assert!(err.get_ban_reason().is_none());

    // Parent header stored ahead of our best block, as header sync leaves it: its body's outputs are not held yet, so
    // dropped without banning the peer
    let mut header3 = BlockHeader::from_previous(block2.header());
    // As if its body added one kernel, the coinbase; headers are indexed by their kernel MMR size
    header3.kernel_mmr_size = block2.header().kernel_mmr_size + 1;
    let header3 = create_chain_header(header3, block2.accumulated_data(), rules.consensus_constants(0));
    store.insert_valid_headers(vec![header3.clone()]).unwrap();
    assert_eq!(store.get_chain_metadata().unwrap().best_block_height(), 2);
    let err = handlers
        .handle_block(unknown_spend(*header3.hash()), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CommsInterfaceError::UnknownSpentOutputs { .. }),
        "{err:?}"
    );
    assert!(err.get_ban_reason().is_none());
}

/// Inputs that are not in the database are looked up among the block's own outputs. That lookup must not cost a pass
/// over every output for every input.
#[tokio::test]
async fn hydrating_many_in_block_spends_completes() {
    const COUNT: usize = 5_000;
    let network = Network::LocalNet;
    let (store, blocks, _, rules, _) = create_new_blockchain(network);
    let mut handlers = new_handlers(&store, new_mempool(), rules);

    let outputs = (0..COUNT)
        .map(|i| TransactionOutput {
            minimum_value_promise: MicroMinotari::from(u64::try_from(i).unwrap()),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    // Every input spends an output of the block, except the last, which spends an output nobody has
    let mut inputs = outputs
        .iter()
        .map(|output| {
            TransactionInput::new_current_version(
                SpentOutput::OutputHash(output.hash()),
                Default::default(),
                Default::default(),
            )
        })
        .collect::<Vec<_>>();
    inputs.push(TransactionInput::new_current_version(
        SpentOutput::OutputHash(FixedHash::from([7u8; 32])),
        Default::default(),
        Default::default(),
    ));
    let mut header = blocks[0].header().clone();
    header.height = 1;
    header.prev_hash = FixedHash::from([9u8; 32]);
    let block = Block::new(header, AggregateBody::new_unsorted(inputs, outputs, vec![]));

    let err = handlers.handle_block(block, None).await.unwrap_err();
    assert!(
        matches!(err, CommsInterfaceError::UnknownSpentOutputs { .. }),
        "{err:?}"
    );
}

/// A peer can relay an honest header with a body of its own. A body that is not the one the header commits to fails
/// validation, but must not get the honest block marked as bad: only a block whose body matches its header is.
#[tokio::test]
async fn only_a_block_whose_body_matches_its_header_is_marked_bad() {
    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);
    let tx = spend_genesis_output(&outputs, &key_manager);
    let honest = prepare_block(&store, &blocks[0], vec![tx], &rules, &key_manager);

    // The honest header with only the coinbase as its body
    let (header, _, block_outputs, block_kernels) = honest.clone().dissolve();
    let fake = Block::new(
        header,
        AggregateBody::new_unsorted(
            vec![],
            block_outputs.into_iter().filter(|o| o.is_coinbase()).collect(),
            block_kernels.into_iter().filter(|k| k.is_coinbase()).collect(),
        ),
    );
    let err = store.add_block(Arc::new(fake)).unwrap_err();
    assert!(matches!(err, ChainStorageError::ValidationError { .. }), "{err:?}");
    assert!(!store.bad_block_exists(honest.hash()).unwrap().0);
    assert!(!store.chain_block_or_orphan_block_exists(honest.hash()).unwrap());

    // The honest block is still accepted
    let BlockAddResult::Ok(tip) = store.add_block(Arc::new(honest)).unwrap() else {
        panic!("the honest block was not added");
    };

    // A block whose body matches its header but is invalid (it claims too much in its coinbase) is marked bad
    let invalid = prepare_block_with_extra_coinbase(&store, &tip, vec![], &rules, &key_manager, T);
    let err = store.add_block(Arc::new(invalid.clone())).unwrap_err();
    assert!(matches!(err, ChainStorageError::ValidationError { .. }), "{err:?}");
    assert!(store.bad_block_exists(invalid.hash()).unwrap().0);

    // A peer announcing it is not banned for it: we hold it as bad, the peer sent us nothing invalid
    let mut handlers = new_handlers(&store, new_mempool(), rules);
    let err = handlers
        .handle_new_block_message(NewBlock::from(&invalid), NodeId::default())
        .await
        .unwrap_err();
    assert!(matches!(err, CommsInterfaceError::KnownBadBlock { .. }), "{err:?}");
    assert!(err.get_ban_reason().is_none());
}

/// H1's header and body, with its coinbase kernel swapped for another one: the inputs and outputs still match the
/// header, the kernels do not
// Overflow in test code panics, which is the desired failure mode for a test.
#[allow(clippy::arithmetic_side_effects)]
fn with_swapped_kernel(h1: &Block, rules: &BaseNodeConsensusManager, key_manager: &KeyManager) -> Block {
    let (_, other_kernel, _) = create_coinbase(
        rules.get_block_reward_at(h1.header.height),
        h1.header.height + rules.consensus_constants(0).coinbase_min_maturity(),
        None,
        key_manager,
    );
    let mut kernels = h1
        .body
        .kernels()
        .iter()
        .filter(|k| !k.is_coinbase())
        .cloned()
        .collect::<Vec<_>>();
    kernels.push(other_kernel);
    let mut body = AggregateBody::new_unsorted(h1.body.inputs().clone(), h1.body.outputs().clone(), kernels);
    body.sort();
    Block::new(h1.header.clone(), body)
}

/// An honest chain G <- H0 <- H1, where H1 spends the genesis output, held up to G or (with `hold_h0`) up to H0 and a
/// competing block X on H0, so that H1 does not build on our tip either way
fn honest_chain(
    hold_h0: bool,
) -> (
    BlockchainDatabase<TempDatabase>,
    BaseNodeConsensusManager,
    KeyManager,
    Arc<ChainBlock>,
    Block,
) {
    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);
    let h0 = prepare_block(&store, &blocks[0], vec![], &rules, &key_manager);
    let BlockAddResult::Ok(h0) = store.add_block(Arc::new(h0)).unwrap() else {
        panic!("H0 was not added");
    };
    let tx = spend_genesis_output(&outputs, &key_manager);
    let h1 = prepare_block(&store, &h0, vec![tx], &rules, &key_manager);
    if hold_h0 {
        append_block(
            &store,
            &h0,
            vec![],
            &rules,
            Difficulty::from_u64(10).unwrap(),
            &key_manager,
        )
        .unwrap();
        assert_eq!(store.get_height().unwrap(), 2);
    } else {
        store.rewind_to_height(0).unwrap();
        if store.chain_block_or_orphan_block_exists(*h0.hash()).unwrap() {
            let mut txn = DbTransaction::new();
            txn.delete_orphan(*h0.hash());
            store.write(txn).unwrap();
        }
        assert!(!store.chain_block_or_orphan_block_exists(*h0.hash()).unwrap());
    }
    (store, rules, key_manager, h0, h1)
}

/// An announcement of `block` naming a transaction, so that the full block is fetched
fn announce_with_a_transaction(block: &Block) -> NewBlock {
    NewBlock {
        header: block.header.clone(),
        coinbase_kernels: block
            .body
            .kernels()
            .iter()
            .filter(|k| k.is_coinbase())
            .cloned()
            .collect(),
        coinbase_outputs: block
            .body
            .outputs()
            .iter()
            .filter(|o| o.is_coinbase())
            .cloned()
            .collect(),
        kernel_excess_sigs: vec![PrivateKey::default()],
    }
}

/// A held orphan whose body was never checked against its header fails when an honest relayer completes its chain. The
/// orphan is not marked bad, and the relayer, whose block was fine, is not banned.
#[tokio::test]
async fn a_held_orphan_with_a_fake_body_does_not_get_the_relayer_of_its_parent_banned() {
    let (store, rules, key_manager, h0, h1) = honest_chain(false);
    let fake_h1 = with_swapped_kernel(&h1, &rules, &key_manager);
    let result = store.add_block(Arc::new(fake_h1)).unwrap();
    assert!(matches!(result, BlockAddResult::OrphanBlock), "{result:?}");

    let mut handlers = new_handlers(&store, new_mempool(), rules);
    let err = handlers
        .handle_block(h0.block().clone(), Some(NodeId::default()))
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            CommsInterfaceError::ChainStorageError(ChainStorageError::UnverifiedHeldBlockInvalid { hash, .. })
                if *hash == h1.hash()
        ),
        "{err:?}"
    );
    assert!(err.get_ban_reason().is_none());
    assert!(!store.bad_block_exists(h1.hash()).unwrap().0);
    assert!(!store.chain_block_or_orphan_block_exists(h1.hash()).unwrap());
}

fn assert_banned_and_not_stored(err: &CommsInterfaceError, store: &BlockchainDatabase<TempDatabase>, hash: FixedHash) {
    assert!(matches!(err, CommsInterfaceError::InvalidFullBlock { .. }), "{err:?}");
    assert_eq!(err.get_ban_reason().map(|r| r.ban_duration), Some(BanPeriod::Long));
    assert!(!store.chain_block_or_orphan_block_exists(hash).unwrap());
}

/// A full block fetched for an announcement that does not build on our tip is checked against its header before it is
/// stored as an orphan: a peer that sends another body is banned.
#[tokio::test]
async fn a_fetched_full_block_with_another_body_is_rejected_on_arrival() {
    let (store, rules, _, _, h1) = honest_chain(false);

    // Other outputs: caught without the parent
    let other_outputs = Block::new(
        h1.header.clone(),
        AggregateBody::new_sorted_unchecked(h1.body.inputs().clone(), vec![], h1.body.kernels().clone()),
    );
    let mut handlers = new_handlers_serving(&store, new_mempool(), rules.clone(), other_outputs);
    let err = handlers
        .handle_new_block_message(announce_with_a_transaction(&h1), NodeId::default())
        .await
        .unwrap_err();
    assert_banned_and_not_stored(&err, &store, h1.hash());

    // Another block altogether
    let mut other_block = h1.clone();
    other_block.header.nonce = other_block.header.nonce.wrapping_add(1);
    let mut handlers = new_handlers_serving(&store, new_mempool(), rules.clone(), other_block);
    let err = handlers
        .handle_new_block_message(announce_with_a_transaction(&h1), NodeId::default())
        .await
        .unwrap_err();
    assert_banned_and_not_stored(&err, &store, h1.hash());

    // The honest body is accepted, as an orphan
    let mut handlers = new_handlers_serving(&store, new_mempool(), rules.clone(), h1.clone());
    handlers
        .handle_new_block_message(announce_with_a_transaction(&h1), NodeId::default())
        .await
        .unwrap();
    assert!(store.chain_block_or_orphan_block_exists(h1.hash()).unwrap());
}

/// A swapped kernel is caught on arrival when we hold the block's parent
#[tokio::test]
async fn a_fetched_full_block_with_a_swapped_kernel_is_rejected_on_arrival_when_we_hold_its_parent() {
    let (store, rules, key_manager, _, h1) = honest_chain(true);
    let fake_h1 = with_swapped_kernel(&h1, &rules, &key_manager);
    let mut handlers = new_handlers_serving(&store, new_mempool(), rules, fake_h1);
    let err = handlers
        .handle_new_block_message(announce_with_a_transaction(&h1), NodeId::default())
        .await
        .unwrap_err();
    assert_banned_and_not_stored(&err, &store, h1.hash());
}

/// An announcement without transactions is built from its own coinbase. A block with more than its coinbase can still
/// be announced that way (when it has no kernel of its own), so then the full block is fetched: the honest block is
/// accepted, and a peer that serves another body is banned and nothing is stored.
#[tokio::test]
async fn an_announcement_of_a_non_empty_block_without_its_transactions_fetches_the_full_block() {
    let (store, rules, _, _, h1) = honest_chain(false);
    assert!(!h1.body.inputs().is_empty());
    let announce = || {
        let mut announcement = announce_with_a_transaction(&h1);
        announcement.kernel_excess_sigs = vec![];
        announcement
    };

    // Another body
    let other_outputs = Block::new(
        h1.header.clone(),
        AggregateBody::new_sorted_unchecked(h1.body.inputs().clone(), vec![], h1.body.kernels().clone()),
    );
    let mut handlers = new_handlers_serving(&store, new_mempool(), rules.clone(), other_outputs);
    let err = handlers
        .handle_new_block_message(announce(), NodeId::default())
        .await
        .unwrap_err();
    assert_banned_and_not_stored(&err, &store, h1.hash());

    // The honest block
    let mut handlers = new_handlers_serving(&store, new_mempool(), rules, h1.clone());
    handlers
        .handle_new_block_message(announce(), NodeId::default())
        .await
        .unwrap();
    assert!(store.chain_block_or_orphan_block_exists(h1.hash()).unwrap());
}

/// Our tip T1, and a competing fork F1 <- F2 where F2 spends an output created in F1. We hold F1 as an orphan; F2 is
/// relayed to us (compact, as peers serve their main chain) by peers that have reorged to the fork. Its input is
/// hydrated from F1 and the block is accepted.
#[tokio::test]
async fn a_relayed_fork_block_spending_a_held_orphans_output_is_hydrated_from_it() {
    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);

    // The fork: F1 spends the genesis output, F2 spends one of F1's outputs
    let schema = txn_schema!(from: vec![outputs[0][0].clone()], to: vec![T, T]);
    let (txs, f1_outputs) = schema_to_transaction(&[schema], &key_manager);
    let f1 = prepare_block(&store, &blocks[0], vec![(*txs[0]).clone()], &rules, &key_manager);
    let BlockAddResult::Ok(f1_chain_block) = store.add_block(Arc::new(f1.clone())).unwrap() else {
        panic!("F1 was not added");
    };
    let schema = txn_schema!(from: vec![f1_outputs[0].clone()], to: vec![MicroMinotari::from(5_000)]);
    let (txs, _) = schema_to_transaction(&[schema], &key_manager);
    let f2 = prepare_block(&store, &f1_chain_block, vec![(*txs[0]).clone()], &rules, &key_manager);
    store.rewind_to_height(0).unwrap();

    // Our chain: a stronger T1, with F1 held as an orphan
    append_block(
        &store,
        &blocks[0],
        vec![],
        &rules,
        Difficulty::from_u64(10).unwrap(),
        &key_manager,
    )
    .unwrap();
    if !store.chain_block_or_orphan_block_exists(f1.hash()).unwrap() {
        let result = store.add_block(Arc::new(f1.clone())).unwrap();
        assert!(matches!(result, BlockAddResult::OrphanBlock), "{result:?}");
    }

    // F2's input spends an output that is neither in our database nor in F2
    let f2_compact = f2.to_compact();
    let spent = f2_compact.body.inputs()[0].output_hash();
    assert!(store.fetch_outputs(spent).unwrap().is_empty());
    assert!(f2_compact.body.outputs().iter().all(|o| o.hash() != spent));

    let mut handlers = new_handlers(&store, new_mempool(), rules);
    handlers
        .handle_block(f2_compact, Some(NodeId::default()))
        .await
        .unwrap();
    // (No error, so nothing for the relayer to be banned for)
    // Stored as an orphan, or reorged to (the chain strength is not decided by the test's difficulties alone)
    assert!(store.chain_block_or_orphan_block_exists(f2.hash()).unwrap());
}

/// `body_matches_header` and `inputs_and_outputs_match_header` recompute part of what `calculate_mmr_roots` computes.
/// For real blocks they must agree with it: true for the block as mined, false for any single changed kernel, input or
/// output, for which the full MMR roots do not match the header either.
#[tokio::test]
async fn body_matches_header_agrees_with_the_mmr_roots() {
    fn full_roots_match(store: &BlockchainDatabase<TempDatabase>, block: &Block) -> bool {
        let (_, roots) = store.calculate_mmr_roots(block.clone()).unwrap();
        let header = &block.header;
        header.kernel_mr == roots.kernel_mr &&
            header.kernel_mmr_size == roots.kernel_mmr_size &&
            header.input_mr == roots.input_mr &&
            header.output_mr == roots.output_mr &&
            header.output_smt_size == roots.output_smt_size &&
            header.block_output_mr == roots.block_output_mr &&
            header.validator_node_mr == roots.validator_node_mr &&
            header.validator_node_size == roots.validator_node_size
    }
    fn body_matches(store: &BlockchainDatabase<TempDatabase>, block: &Block) -> bool {
        let db = store.db_read_access().unwrap();
        body_matches_header(&*db, block).unwrap()
    }
    fn with_body(
        block: &Block,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
    ) -> Block {
        let mut body = AggregateBody::new_unsorted(inputs, outputs, kernels);
        body.sort();
        Block::new(block.header.clone(), body)
    }

    let network = Network::LocalNet;
    let (store, blocks, outputs, rules, key_manager) = create_new_blockchain(network);

    // Two transactions (so several kernels), the second spending an output of the first in the same block
    let schema = txn_schema!(from: vec![outputs[0][0].clone()], to: vec![T, T]);
    let (txs1, tx1_outputs) = schema_to_transaction(&[schema], &key_manager);
    let schema = txn_schema!(from: vec![tx1_outputs[0].clone()], to: vec![MicroMinotari::from(5_000)]);
    let (txs2, _) = schema_to_transaction(&[schema], &key_manager);
    let with_txs = prepare_block(
        &store,
        &blocks[0],
        vec![(*txs1[0]).clone(), (*txs2[0]).clone()],
        &rules,
        &key_manager,
    );
    assert!(with_txs.body.kernels().len() >= 3);
    let coinbase_only = prepare_block(&store, &blocks[0], vec![], &rules, &key_manager);

    for block in [&coinbase_only, &with_txs] {
        assert!(full_roots_match(&store, block));
        assert!(body_matches(&store, block));
        assert!(inputs_and_outputs_match_header(block).unwrap());
    }

    let (_, inputs, outputs, kernels) = with_txs.clone().dissolve();
    // A changed kernel
    let mut changed_kernels = kernels.clone();
    changed_kernels[0].fee += MicroMinotari::from(1);
    // A changed input
    let mut changed_inputs = inputs.clone();
    changed_inputs[0].input_data = ExecutionStack::default();
    // A changed output
    let mut changed_outputs = outputs.clone();
    changed_outputs[0].minimum_value_promise += MicroMinotari::from(1);
    let tampered = [
        with_body(&with_txs, inputs.clone(), outputs.clone(), changed_kernels),
        with_body(&with_txs, changed_inputs, outputs.clone(), kernels.clone()),
        with_body(&with_txs, inputs, changed_outputs, kernels),
    ];
    for (i, block) in tampered.iter().enumerate() {
        assert!(!full_roots_match(&store, block), "tamper {i}");
        assert!(!body_matches(&store, block), "tamper {i}");
    }
    // Inputs and outputs, but not kernels, are covered without state
    assert!(inputs_and_outputs_match_header(&tampered[0]).unwrap());
    assert!(!inputs_and_outputs_match_header(&tampered[1]).unwrap());
    assert!(!inputs_and_outputs_match_header(&tampered[2]).unwrap());
}
