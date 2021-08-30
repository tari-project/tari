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

use std::sync::Arc;

use futures::{channel::mpsc, StreamExt};
use tari_crypto::{
    inputs,
    keys::PublicKey as PublicKeyTrait,
    script,
    script::TariScript,
    tari_utilities::hash::Hashable,
};
use tokio::sync::broadcast;

use helpers::block_builders::append_block;
use tari_common::configuration::Network;
use tari_common_types::{chain_metadata::ChainMetadata, types::PublicKey};
use tari_comms::peer_manager::NodeId;
use tari_core::{
    base_node::{
        comms_interface::{CommsInterfaceError, InboundNodeCommsHandlers, NodeCommsRequest, NodeCommsResponse},
        OutboundNodeCommsInterface,
    },
    blocks::{BlockBuilder, BlockHeader},
    chain_storage::{BlockchainDatabaseConfig, DbTransaction, HistoricalBlock, Validators},
    consensus::{ConsensusManager, NetworkConsensus},
    mempool::{Mempool, MempoolConfig},
    test_helpers::blockchain::{create_store_with_consensus_and_validators_and_config, create_test_blockchain_db},
    transactions::{
        helpers::{create_utxo, spend_utxos},
        tari_amount::MicroTari,
        transaction::{OutputFeatures, TransactionOutput, UnblindedOutput},
        CryptoFactories,
    },
    txn_schema,
    validation::{mocks::MockValidator, transaction_validators::TxInputAndMaturityValidator},
};
use tari_service_framework::{reply_channel, reply_channel::Receiver};

#[allow(dead_code)]
mod helpers;
// use crate::helpers::database::create_test_db;

async fn test_request_responder(
    receiver: &mut Receiver<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>,
    response: NodeCommsResponse,
) {
    let req_context = receiver.next().await.unwrap();
    req_context.reply(Ok(response)).unwrap()
}

fn new_mempool() -> Mempool {
    let mempool_validator = MockValidator::new(true);
    Mempool::new(MempoolConfig::default(), Arc::new(mempool_validator))
}

#[tokio_macros::test]
async fn outbound_get_metadata() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    let metadata = ChainMetadata::new(5, vec![0u8], 3, 0, 5);
    let metadata_response = NodeCommsResponse::ChainMetadata(metadata.clone());
    let (received_metadata, _) = futures::join!(
        outbound_nci.get_metadata(),
        test_request_responder(&mut request_receiver, metadata_response)
    );
    assert_eq!(received_metadata.unwrap(), metadata);
}

#[tokio_macros::test]
async fn inbound_get_metadata() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender.clone());
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let block = store.fetch_block(0).unwrap().block().clone();

    if let Ok(NodeCommsResponse::ChainMetadata(received_metadata)) =
        inbound_nch.handle_request(NodeCommsRequest::GetChainMetadata).await
    {
        assert_eq!(received_metadata.height_of_longest_chain(), 0);
        assert_eq!(received_metadata.best_block(), &block.hash());
        assert_eq!(received_metadata.pruning_horizon(), 0);
    } else {
        panic!();
    }
}

#[tokio_macros::test]
async fn inbound_fetch_kernel_by_excess_sig() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender.clone());
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let block = store.fetch_block(0).unwrap().block().clone();
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

#[tokio_macros::test]
async fn outbound_fetch_headers() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    let mut header = BlockHeader::new(0);
    header.height = 0;
    let header_response = NodeCommsResponse::BlockHeaders(vec![header.clone()]);
    let (received_headers, _) = futures::join!(
        outbound_nci.fetch_headers(vec![0]),
        test_request_responder(&mut request_receiver, header_response)
    );
    let received_headers = received_headers.unwrap();
    assert_eq!(received_headers.len(), 1);
    assert_eq!(received_headers[0], header);
}

#[tokio_macros::test]
async fn inbound_fetch_headers() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let header = store.fetch_block(0).unwrap().header().clone();

    if let Ok(NodeCommsResponse::BlockHeaders(received_headers)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchHeaders(vec![0]))
        .await
    {
        assert_eq!(received_headers.len(), 1);
        assert_eq!(received_headers[0], header);
    } else {
        panic!();
    }
}

#[tokio_macros::test]
async fn outbound_fetch_utxos() {
    let factories = CryptoFactories::default();
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    let (utxo, _, _) = create_utxo(
        MicroTari(10_000),
        &factories,
        Default::default(),
        &TariScript::default(),
    );
    let hash = utxo.hash();
    let utxo_response = NodeCommsResponse::TransactionOutputs(vec![utxo.clone()]);
    let (received_utxos, _) = futures::join!(
        outbound_nci.fetch_utxos(vec![hash]),
        test_request_responder(&mut request_receiver, utxo_response)
    );
    let received_utxos = received_utxos.unwrap();
    assert_eq!(received_utxos.len(), 1);
    assert_eq!(received_utxos[0], utxo);
}

#[tokio_macros::test]
async fn inbound_fetch_utxos() {
    let factories = CryptoFactories::default();

    let store = create_test_blockchain_db();
    let mempool = new_mempool();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let block = store.fetch_block(0).unwrap().block().clone();
    let utxo_1 = block.body.outputs()[0].clone();
    let hash_1 = utxo_1.hash();

    let (utxo_2, _, _) = create_utxo(
        MicroTari(10_000),
        &factories,
        Default::default(),
        &TariScript::default(),
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

#[tokio_macros::test]
async fn outbound_fetch_txos() {
    let factories = CryptoFactories::default();
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    let (txo1, _, _) = create_utxo(
        MicroTari(10_000),
        &factories,
        Default::default(),
        &TariScript::default(),
    );
    let (txo2, _, _) = create_utxo(
        MicroTari(15_000),
        &factories,
        Default::default(),
        &TariScript::default(),
    );
    let hash1 = txo1.hash();
    let hash2 = txo2.hash();
    let txo_response = NodeCommsResponse::TransactionOutputs(vec![txo1.clone(), txo2.clone()]);
    let (received_txos, _) = futures::join!(
        outbound_nci.fetch_txos(vec![hash1, hash2]),
        test_request_responder(&mut request_receiver, txo_response)
    );
    let received_txos = received_txos.unwrap();
    assert_eq!(received_txos.len(), 2);
    assert_eq!(received_txos[0], txo1);
    assert_eq!(received_txos[1], txo2);
}

#[tokio_macros::test]
async fn inbound_fetch_txos() {
    let factories = CryptoFactories::default();
    let store = create_test_blockchain_db();
    let mempool = new_mempool();
    let (block_event_sender, _) = broadcast::channel(50);
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
    );

    let (utxo, _, _) = create_utxo(
        MicroTari(10_000),
        &factories,
        Default::default(),
        &TariScript::default(),
    );
    let (pruned_utxo, _, _) = create_utxo(
        MicroTari(10_000),
        &factories,
        Default::default(),
        &TariScript::default(),
    );
    let (stxo, _, _) = create_utxo(
        MicroTari(10_000),
        &factories,
        Default::default(),
        &TariScript::default(),
    );
    let utxo_hash = utxo.hash();
    let stxo_hash = stxo.hash();
    let pruned_utxo_hash = pruned_utxo.hash();
    let block = store.fetch_block(0).unwrap().block().clone();
    let header_hash = block.header.hash();
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), header_hash.clone(), block.header.height, 6000);
    txn.insert_utxo(stxo.clone(), header_hash.clone(), block.header.height, 6001);
    txn.insert_pruned_utxo(
        pruned_utxo_hash.clone(),
        pruned_utxo.witness_hash(),
        header_hash.clone(),
        5,
        6002,
    );
    assert!(store.commit(txn).is_ok());

    if let Ok(NodeCommsResponse::TransactionOutputs(received_txos)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingTxos(vec![
            utxo_hash,
            stxo_hash,
            pruned_utxo_hash,
        ]))
        .await
    {
        assert_eq!(received_txos.len(), 2);
        assert_eq!(received_txos[0], utxo);
        assert_eq!(received_txos[1], stxo);
    } else {
        panic!();
    }
}

#[tokio_macros::test]
async fn outbound_fetch_blocks() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let network = Network::LocalNet;
    let consensus_constants = NetworkConsensus::from(network).create_consensus_constants();
    let gb = BlockBuilder::new(consensus_constants[0].blockchain_version()).build();
    let block = HistoricalBlock::new(gb, 0, Default::default(), vec![], 0);
    let block_response = NodeCommsResponse::HistoricalBlocks(vec![block.clone()]);
    let (received_blocks, _) = futures::join!(
        outbound_nci.fetch_blocks(vec![0]),
        test_request_responder(&mut request_receiver, block_response)
    );
    let received_blocks = received_blocks.unwrap();
    assert_eq!(received_blocks.len(), 1);
    assert_eq!(received_blocks[0], block);
}

#[tokio_macros::test]
async fn inbound_fetch_blocks() {
    let store = create_test_blockchain_db();
    let mempool = new_mempool();
    let (block_event_sender, _) = broadcast::channel(50);
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let block = store.fetch_block(0).unwrap().block().clone();

    if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingBlocks(vec![0]))
        .await
    {
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(*received_blocks[0].block(), block);
    } else {
        panic!();
    }
}

#[tokio_macros::test]
// Test needs to be updated to new pruned structure.
async fn inbound_fetch_blocks_before_horizon_height() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let block0 = consensus_manager.get_genesis_block();
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
    let mempool_validator = TxInputAndMaturityValidator::new(store.clone());
    let mempool = Mempool::new(MempoolConfig::default(), Arc::new(mempool_validator));
    let (block_event_sender, _) = broadcast::channel(50);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = mpsc::unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_sender,
        store.clone().into(),
        mempool,
        consensus_manager.clone(),
        outbound_nci,
    );
    let script = script!(Nop);
    let (utxo, key, offset) = create_utxo(MicroTari(10_000), &factories, Default::default(), &script);
    let metadata_signature = TransactionOutput::create_final_metadata_signature(
        &MicroTari(10_000),
        &key,
        &script,
        &OutputFeatures::default(),
        &offset,
    )
    .unwrap();
    let unblinded_output = UnblindedOutput::new(
        MicroTari(10_000),
        key.clone(),
        Default::default(),
        script,
        inputs!(PublicKey::from_secret_key(&key)),
        key,
        PublicKey::from_secret_key(&offset),
        metadata_signature,
    );
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), block0.hash().clone(), 0, 4002);
    assert!(store.commit(txn).is_ok());

    let txn = txn_schema!(
        from: vec![unblinded_output],
        to: vec![MicroTari(5_000), MicroTari(4_000)]
    );
    let (txn, _, _) = spend_utxos(txn);
    let block1 = append_block(&store, &block0, vec![txn], &consensus_manager, 1.into()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, 1.into()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, 1.into()).unwrap();
    let block4 = append_block(&store, &block3, vec![], &consensus_manager, 1.into()).unwrap();
    let _block5 = append_block(&store, &block4, vec![], &consensus_manager, 1.into()).unwrap();

    if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingBlocks(vec![1]))
        .await
    {
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0].pruned_outputs().len(), 1)
    } else {
        panic!();
    }

    if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
        .handle_request(NodeCommsRequest::FetchMatchingBlocks(vec![2]))
        .await
    {
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0].block(), block2.block());
    } else {
        panic!();
    }
}
