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

use std::sync::{Arc, RwLock};

use tari_common::configuration::Network;
use tari_comms::test_utils::mocks::create_connectivity_mock;
use tari_core::{
    base_node::comms_interface::{
        InboundNodeCommsHandlers,
        NodeCommsRequest,
        NodeCommsResponse,
        OutboundNodeCommsInterface,
    },
    chain_storage::{BlockchainDatabaseConfig, Validators},
    consensus::ConsensusManager,
    covenants::Covenant,
    mempool::{Mempool, MempoolConfig},
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    test_helpers::{
        blockchain::{create_store_with_consensus_and_validators_and_config, create_test_blockchain_db},
        create_consensus_rules,
    },
    transactions::{
        key_manager::{
            create_memory_db_key_manager,
            MemoryDbKeyManager,
            TransactionKeyManagerBranch,
            TransactionKeyManagerInterface,
        },
        tari_amount::MicroMinotari,
        test_helpers::{create_utxo, TestParams, TransactionSchema},
        transaction_components::{
            OutputFeatures,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
            WalletOutputBuilder,
        },
        transaction_protocol::transaction_initializer::SenderTransactionInitializer,
        SenderTransactionProtocol,
    },
    txn_schema,
    validation::{mocks::MockValidator, transaction::TransactionChainLinkedValidator},
    OutputSmt,
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{inputs, script, ExecutionStack};
use tari_service_framework::reply_channel;
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
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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
    let (store, blocks, _outputs, consensus_manager, _key_manager) = create_new_blockchain(network).await;
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
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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
    let (store, blocks, _outputs, consensus_manager, _key_manager) = create_new_blockchain(network).await;
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

    let key_manager = create_memory_db_key_manager();
    let (utxo_2, _, _) = create_utxo(
        MicroMinotari(10_000),
        &key_manager,
        &Default::default(),
        &script!(Nop),
        &Covenant::default(),
        MicroMinotari::zero(),
    )
    .await;
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
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

async fn initialize_sender_transaction_protocol_for_overflow_test(
    key_manager: &MemoryDbKeyManager,
    txn_schema: TransactionSchema,
) -> SenderTransactionInitializer<MemoryDbKeyManager> {
    let constants = ConsensusManager::builder(Network::LocalNet)
        .build()
        .unwrap()
        .consensus_constants(0)
        .clone();
    let mut stx_builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
    let change = TestParams::new(key_manager).await;
    let script_public_key = key_manager
        .get_public_key_at_key_id(&change.script_key_id)
        .await
        .unwrap();
    stx_builder
        .with_lock_height(txn_schema.lock_height)
        .with_fee_per_gram(txn_schema.fee)
        .with_change_data(
            script!(PushPubKey(Box::new(script_public_key))),
            ExecutionStack::default(),
            change.script_key_id,
            change.spend_key_id,
            Covenant::default(),
        );

    for tx_input in &txn_schema.from {
        stx_builder.with_input(tx_input.clone()).await.unwrap();
    }
    for tx_output in txn_schema.to {
        let (spending_key, _) = key_manager
            .get_next_key(TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
            .await
            .unwrap();
        let (sender_offset_key_id, sender_offset_public_key) = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let (script_key_id, _) = key_manager
            .get_next_key(TransactionKeyManagerBranch::ScriptKey.get_branch_key())
            .await
            .unwrap();
        let script_public_key = key_manager.get_public_key_at_key_id(&script_key_id).await.unwrap();
        let input_data = match &txn_schema.input_data {
            Some(data) => data.clone(),
            None => inputs!(script_public_key),
        };
        let version = match txn_schema.output_version {
            Some(data) => data,
            None => TransactionOutputVersion::get_current_version(),
        };
        let output = WalletOutputBuilder::new(tx_output, spending_key)
            .with_features(txn_schema.features.clone())
            .with_script(txn_schema.script.clone())
            .encrypt_data_for_recovery(key_manager, None)
            .await
            .unwrap()
            .with_input_data(input_data)
            .with_covenant(txn_schema.covenant.clone())
            .with_version(version)
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(script_key_id.clone())
            .sign_as_sender_and_receiver(key_manager, &sender_offset_key_id)
            .await
            .unwrap()
            .try_build(key_manager)
            .await
            .unwrap();

        stx_builder.with_output(output, sender_offset_key_id).await.unwrap();
    }
    for mut utxo in txn_schema.to_outputs {
        let (sender_offset_key_id, _) = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let metadata_message = TransactionOutput::metadata_signature_message(&utxo);
        utxo.metadata_signature = key_manager
            .get_metadata_signature(
                &utxo.spending_key_id,
                &utxo.value.into(),
                &sender_offset_key_id,
                &utxo.version,
                &metadata_message,
                utxo.features.range_proof_type,
            )
            .await
            .unwrap();

        stx_builder.with_output(utxo, sender_offset_key_id).await.unwrap();
    }

    stx_builder
}

#[tokio::test]
async fn test_sender_transaction_protocol_for_overflow() {
    let key_manager = create_memory_db_key_manager();
    let script = script!(Nop);
    let amount = MicroMinotari(u64::MAX); // This is the adversary's attack!
    let output_features = OutputFeatures::default();
    let covenant = Covenant::default();
    let (utxo, spending_key_id, sender_offset_key_id) = create_utxo(
        amount,
        &key_manager,
        &output_features,
        &script,
        &covenant,
        MicroMinotari::zero(),
    )
    .await;

    let wallet_output = WalletOutput::new_with_rangeproof(
        TransactionOutputVersion::get_current_version(),
        amount,
        spending_key_id.clone(),
        output_features,
        script,
        inputs!(key_manager.get_public_key_at_key_id(&spending_key_id).await.unwrap()),
        spending_key_id,
        key_manager
            .get_public_key_at_key_id(&sender_offset_key_id)
            .await
            .unwrap(),
        utxo.metadata_signature,
        0,
        covenant,
        utxo.encrypted_data,
        utxo.minimum_value_promise,
        utxo.proof,
    );

    // Test overflow in inputs
    let txn_schema =
        // This is the adversary's attack!
        txn_schema!(from: vec![wallet_output.clone(), wallet_output.clone()], to: vec![MicroMinotari(5_000)]);
    let stx_builder = initialize_sender_transaction_protocol_for_overflow_test(&key_manager, txn_schema).await;
    assert_eq!(
        format!("{:?}", stx_builder.build().await.unwrap_err()),
        "Total inputs being spent amount overflow"
    );

    // Test overflow in outputs to self
    let txn_schema =
        // This is the adversary's attack!
        txn_schema!(from: vec![wallet_output.clone()], to: vec![MicroMinotari(5_000), MicroMinotari(u64::MAX)]);
    let stx_builder = initialize_sender_transaction_protocol_for_overflow_test(&key_manager, txn_schema).await;
    assert_eq!(
        format!("{:?}", stx_builder.build().await.unwrap_err()),
        "Total outputs to self amount overflow"
    );

    // Test overflow in total input value (inputs + outputs to self + fee)
    let txn_schema =
        // This is the adversary's attack!
        txn_schema!(from: vec![wallet_output.clone()], to: vec![MicroMinotari(u64::MAX)]);
    let stx_builder = initialize_sender_transaction_protocol_for_overflow_test(&key_manager, txn_schema).await;
    assert_eq!(
        format!("{:?}", stx_builder.build().await.unwrap_err()),
        "Total input value overflow"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn inbound_fetch_blocks_before_horizon_height() {
    let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let block0 = consensus_manager.get_genesis_block();
    let key_manager = create_memory_db_key_manager();
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
    let smt = Arc::new(RwLock::new(OutputSmt::new()));
    let store =
        create_store_with_consensus_and_validators_and_config(consensus_manager.clone(), validators, config, smt);
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
    .await
    .unwrap();
    let (block2, _) = append_block(
        &store,
        &block1,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .await
    .unwrap();
    let (block3, _) = append_block(
        &store,
        &block2,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .await
    .unwrap();
    let (block4, _) = append_block(
        &store,
        &block3,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .await
    .unwrap();
    let (_block5, _) = append_block(
        &store,
        &block4,
        vec![],
        &consensus_manager,
        Difficulty::min(),
        &key_manager,
    )
    .await
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
