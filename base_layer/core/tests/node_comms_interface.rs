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

use futures::{channel::mpsc::unbounded as futures_mpsc_channel_unbounded, executor::block_on, StreamExt};
use tari_broadcast_channel::bounded;
use tari_comms::peer_manager::NodeId;
use tari_core::{
    base_node::{
        comms_interface::{CommsInterfaceError, InboundNodeCommsHandlers, NodeCommsRequest, NodeCommsResponse},
        OutboundNodeCommsInterface,
    },
    blocks::{BlockBuilder, BlockHeader},
    chain_storage::{BlockchainDatabase, ChainMetadata, DbTransaction, HistoricalBlock, MemoryDatabase},
    consensus::{ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    mempool::{Mempool, MempoolConfig, MempoolValidators},
    transactions::{
        helpers::{create_test_kernel, create_utxo},
        tari_amount::MicroTari,
        types::{CryptoFactories, HashDigest},
    },
    validation::transaction_validators::TxInputAndMaturityValidator,
};
use tari_crypto::tari_utilities::hash::Hashable;
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_test_utils::runtime::test_async;

async fn test_request_responder(
    receiver: &mut Receiver<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>,
    response: NodeCommsResponse,
)
{
    let req_context = receiver.next().await.unwrap();
    req_context.reply(Ok(response)).unwrap()
}

fn new_mempool() -> (
    Mempool<MemoryDatabase<HashDigest>>,
    BlockchainDatabase<MemoryDatabase<HashDigest>>,
) {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let mempool_validator = MempoolValidators::new(TxInputAndMaturityValidator {}, TxInputAndMaturityValidator {});
    let mempool = Mempool::new(store.clone(), MempoolConfig::default(), mempool_validator);
    (mempool, store)
}

#[test]
fn outbound_get_metadata() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    block_on(async {
        let metadata = ChainMetadata::new(5, vec![0u8], 3, 5.into());
        let metadata_response = NodeCommsResponse::ChainMetadata(metadata.clone());
        let (received_metadata, _) = futures::join!(
            outbound_nci.get_metadata(),
            test_request_responder(&mut request_receiver, metadata_response)
        );
        assert_eq!(received_metadata.unwrap(), metadata);
    });
}

#[test]
fn inbound_get_metadata() {
    let (mempool, store) = new_mempool();

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let (block_event_publisher, _block_event_subscriber) = bounded(100);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender.clone());
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_publisher,
        store.clone(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let block = store.fetch_block(0).unwrap().block().clone();

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::ChainMetadata(received_metadata)) =
                inbound_nch.handle_request(&NodeCommsRequest::GetChainMetadata).await
            {
                assert_eq!(received_metadata.height_of_longest_chain, Some(0));
                assert_eq!(received_metadata.best_block, Some(block.hash()));
                assert_eq!(received_metadata.pruning_horizon, 2880);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_kernels() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    block_on(async {
        let kernel = create_test_kernel(5.into(), 0);
        let hash = kernel.hash();
        let kernel_response = NodeCommsResponse::TransactionKernels(vec![kernel.clone()]);
        let (received_kernels, _) = futures::join!(
            outbound_nci.fetch_kernels(vec![hash]),
            test_request_responder(&mut request_receiver, kernel_response)
        );
        let received_kernels = received_kernels.unwrap();
        assert_eq!(received_kernels.len(), 1);
        assert_eq!(received_kernels[0], kernel);
    });
}

#[test]
fn inbound_fetch_kernels() {
    let (mempool, store) = new_mempool();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let (block_event_publisher, _block_event_subscriber) = bounded(100);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_publisher,
        store.clone(),
        mempool,
        consensus_manager,
        outbound_nci,
    );

    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone(), true);
    assert!(store.commit(txn).is_ok());

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::TransactionKernels(received_kernels)) = inbound_nch
                .handle_request(&NodeCommsRequest::FetchKernels(vec![hash]))
                .await
            {
                assert_eq!(received_kernels.len(), 1);
                assert_eq!(received_kernels[0], kernel);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_headers() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    block_on(async {
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
    });
}

#[test]
fn inbound_fetch_headers() {
    let (mempool, store) = new_mempool();
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .build();
    let (block_event_publisher, _block_event_subscriber) = bounded(100);
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_publisher,
        store.clone(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let header = store.fetch_block(0).unwrap().block().header.clone();

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::BlockHeaders(received_headers)) = inbound_nch
                .handle_request(&NodeCommsRequest::FetchHeaders(vec![0]))
                .await
            {
                assert_eq!(received_headers.len(), 1);
                assert_eq!(received_headers[0], header);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_utxos() {
    let factories = CryptoFactories::default();
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);

    block_on(async {
        let (utxo, _) = create_utxo(MicroTari(10_000), &factories, None);
        let hash = utxo.hash();
        let utxo_response = NodeCommsResponse::TransactionOutputs(vec![utxo.clone()]);
        let (received_utxos, _) = futures::join!(
            outbound_nci.fetch_utxos(vec![hash]),
            test_request_responder(&mut request_receiver, utxo_response)
        );
        let received_utxos = received_utxos.unwrap();
        assert_eq!(received_utxos.len(), 1);
        assert_eq!(received_utxos[0], utxo);
    });
}

#[test]
fn inbound_fetch_utxos() {
    let factories = CryptoFactories::default();
    let (mempool, store) = new_mempool();
    let (block_event_publisher, _block_event_subscriber) = bounded(100);
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .build();
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_publisher,
        store.clone(),
        mempool,
        consensus_manager,
        outbound_nci,
    );

    let (utxo, _) = create_utxo(MicroTari(10_000), &factories, None);
    let hash = utxo.hash();
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), true);
    assert!(store.commit(txn).is_ok());

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::TransactionOutputs(received_utxos)) = inbound_nch
                .handle_request(&NodeCommsRequest::FetchUtxos(vec![hash]))
                .await
            {
                assert_eq!(received_utxos.len(), 1);
                assert_eq!(received_utxos[0], utxo);
            } else {
                assert!(false);
            }
        });
    });
}

#[test]
fn outbound_fetch_blocks() {
    let (request_sender, mut request_receiver) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let mut outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    block_on(async {
        let gb = BlockBuilder::new(consensus_constants.blockchain_version()).build();
        let block = HistoricalBlock::new(gb, 0, Vec::new());
        let block_response = NodeCommsResponse::HistoricalBlocks(vec![block.clone()]);
        let (received_blocks, _) = futures::join!(
            outbound_nci.fetch_blocks(vec![0]),
            test_request_responder(&mut request_receiver, block_response)
        );
        let received_blocks = received_blocks.unwrap();
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0], block);
    });
}

#[test]
fn inbound_fetch_blocks() {
    let (mempool, store) = new_mempool();
    let (block_event_publisher, _block_event_subscriber) = bounded(100);
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .build();
    let (request_sender, _) = reply_channel::unbounded();
    let (block_sender, _) = futures_mpsc_channel_unbounded();
    let outbound_nci = OutboundNodeCommsInterface::new(request_sender, block_sender);
    let inbound_nch = InboundNodeCommsHandlers::new(
        block_event_publisher,
        store.clone(),
        mempool,
        consensus_manager,
        outbound_nci,
    );
    let block = store.fetch_block(0).unwrap().block().clone();

    test_async(move |rt| {
        rt.spawn(async move {
            if let Ok(NodeCommsResponse::HistoricalBlocks(received_blocks)) = inbound_nch
                .handle_request(&NodeCommsRequest::FetchBlocks(vec![0]))
                .await
            {
                assert_eq!(received_blocks.len(), 1);
                assert_eq!(*received_blocks[0].block(), block);
            } else {
                assert!(false);
            }
        });
    });
}
