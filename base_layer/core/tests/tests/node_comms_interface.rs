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
use std::sync::Arc;

use tari_common::configuration::Network;
use tari_comms::test_utils::mocks::create_connectivity_mock;
use tari_core::{
    base_node::comms_interface::{
        GetNewBlockTemplateRequest,
        InboundNodeCommsHandlers,
        NodeCommsRequest,
        NodeCommsResponse,
        OutboundNodeCommsInterface,
    },
    chain_storage::{BlockchainDatabaseConfig, Validators},
    consensus::BaseNodeConsensusManager,
    mempool::{Mempool, MempoolConfig},
    proof_of_work::randomx_factory::RandomXFactory,
    test_helpers::{
        blockchain::{create_store_with_consensus_and_validators_and_config, create_test_blockchain_db},
        create_consensus_rules,
    },
    validation::{mocks::MockValidator, transaction::TransactionChainLinkedValidator},
};
use tari_script::script;
use tari_service_framework::reply_channel;
use tari_transaction_components::{
    MicroMinotari,
    key_manager::KeyManager,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
    test_helpers::create_utxo,
    transaction_components::covenants::Covenant,
};
use tokio::sync::{broadcast, mpsc};

use crate::helpers::{block_builders::append_block, sample_blockchains::create_new_blockchain};

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
    // underlying storage and last-seen broadcast channel.
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool.clone(),
        consensus_manager.clone(),
        outbound_nci,
        connectivity,
        randomx_factory,
    );

    let handle = tokio::spawn(async move {
        inbound_nch
            .handle_request(NodeCommsRequest::GetNewBlockTemplate(GetNewBlockTemplateRequest {
                algo: PowAlgorithm::Sha3x,
                max_weight: 0,
            }))
            .await
    });

    // Nudge the spawned handler so it reaches its wait before we advance the chain (best-effort, for coverage). The
    // assertion below does not depend on winning this race: the chain is advanced to height 3 *before* the mempool is
    // notified, so the mempool can never be observed ahead of a stale tip. Whatever the scheduling, once both the store
    // and mempool are at height 3 the handler builds the template on height 3 (-> template height 4). Notifying right
    // away (no fixed sleep) also gives the handler's internal timeout maximal slack, so it cannot flake on a loaded CI
    // runner where `retries = 0`.
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
