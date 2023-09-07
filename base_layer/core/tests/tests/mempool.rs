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

use std::{convert::TryFrom, ops::Deref, panic, sync::Arc, time::Duration};

use randomx_rs::RandomXFlag;
use tari_common::configuration::Network;
use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_core::{
    base_node::state_machine_service::states::{ListeningInfo, StateInfo, StatusInfo},
    consensus::{ConsensusConstantsBuilder, ConsensusManager},
    mempool::{Mempool, MempoolConfig, MempoolServiceConfig, TxStorageResponse},
    proof_of_work::Difficulty,
    proto,
    transactions::{
        fee::Fee,
        key_manager::{TransactionKeyManagerBranch, TransactionKeyManagerInterface, TxoStage},
        tari_amount::{uT, MicroMinotari, T},
        test_helpers::{
            create_test_core_key_manager_with_memory_db,
            create_wallet_output_with_data,
            schema_to_transaction,
            spend_utxos,
            TestParams,
            TransactionSchema,
            UtxoTestParams,
        },
        transaction_components::{
            KernelBuilder,
            OutputFeatures,
            OutputType,
            RangeProofType,
            Transaction,
            TransactionKernel,
            TransactionKernelVersion,
        },
        transaction_protocol::TransactionMetadata,
        CryptoFactories,
    },
    tx,
    txn_schema,
    validation::{
        transaction::{
            TransactionChainLinkedValidator,
            TransactionFullValidator,
            TransactionInternalConsistencyValidator,
        },
        ValidationError,
    },
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_p2p::{services::liveness::LivenessConfig, tari_message::TariMessageType};
use tari_script::script;
use tari_test_utils::async_assert_eventually;
use tempfile::tempdir;

use crate::helpers::{
    block_builders::{
        chain_block,
        create_genesis_block,
        create_genesis_block_with_coinbase_value,
        find_header_with_achieved_difficulty,
        generate_block,
        generate_new_block,
    },
    nodes::{create_network_with_2_base_nodes_with_config, create_network_with_3_base_nodes_with_config},
    sample_blockchains::{create_new_blockchain, create_new_blockchain_with_constants},
};

#[tokio::test]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
async fn test_insert_and_process_published_block() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let mempool_validator = TransactionChainLinkedValidator::new(store.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    // Create a block with 4 outputs
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![2 * T, 2 * T, 2 * T, 2 * T],fee: 5.into(), lock: 0, features: OutputFeatures::default()
    )];
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    // Create 6 new transactions to add to the mempool
    let (orphan, _, _) = tx!(1*T, fee: 100*uT, &key_manager).expect("Failed to get tx");
    let orphan = Arc::new(orphan);

    let tx2 = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1*T], fee: 20*uT, lock: 0, features: OutputFeatures::default());
    let tx2 = Arc::new(spend_utxos(tx2, &key_manager).await.0);

    let tx3 = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![1*T],
        fee: 20*uT,
        lock: 4,
        features: OutputFeatures{
            maturity: 1,
            ..Default::default()
        }
    );
    let tx3 = Arc::new(spend_utxos(tx3, &key_manager).await.0);

    let tx5 = txn_schema!(
        from: vec![outputs[1][2].clone()],
        to: vec![1*T],
        fee: 20*uT,
        lock: 3,
        features: OutputFeatures{
            maturity: 2,
            ..Default::default()
        }
    );
    let tx5 = Arc::new(spend_utxos(tx5, &key_manager).await.0);
    let tx6 = txn_schema!(from: vec![outputs[1][3].clone()], to: vec![1 * T], fee: 25*uT, lock: 0, features: OutputFeatures::default());
    let tx6 = spend_utxos(tx6, &key_manager).await.0;

    mempool.insert(orphan.clone()).await.unwrap();
    mempool.insert(tx2.clone()).await.unwrap();
    mempool.insert(tx3.clone()).await.unwrap();
    mempool.insert(tx5.clone()).await.unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(orphan.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx2.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx3.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );

    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx5.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx6.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );

    let snapshot_txs = mempool.snapshot().await.unwrap();
    assert_eq!(snapshot_txs.len(), 1);
    assert!(snapshot_txs.contains(&tx2));

    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 1);
    assert_eq!(stats.reorg_txs, 0);
    let expected_weight = tx2
        .body
        .calculate_weight(consensus_manager.consensus_constants(0).transaction_weight_params())
        .unwrap();
    assert_eq!(stats.unconfirmed_weight, expected_weight);

    // Spend tx2, so it goes in Reorg pool
    generate_block(
        &store,
        &mut blocks,
        vec![tx2.deref().clone()],
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();

    assert_eq!(
        mempool
            .has_tx_with_excess_sig(orphan.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx2.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::ReorgPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx3.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx5.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(tx6.body.kernels()[0].excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::NotStored
    );

    let snapshot_txs = mempool.snapshot().await.unwrap();
    assert_eq!(snapshot_txs.len(), 0);

    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 0);
    assert_eq!(stats.reorg_txs, 1);
    assert_eq!(stats.unconfirmed_weight, 0);
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_time_locked() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let mempool_validator = TransactionChainLinkedValidator::new(store.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    // Create a block with 4 outputs
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![2 * T, 2 * T, 2 * T, 2 * T], fee: 5*uT, lock: 0, features: OutputFeatures::default()
    )];
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();
    // Block height should be 1
    let mut tx2 = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1*T], fee: 20*uT, lock: 0, features: OutputFeatures::default());
    tx2.lock_height = 3;
    let tx2 = Arc::new(spend_utxos(tx2, &key_manager).await.0);

    let mut tx3 = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![1*T],
        fee: 4*uT,
        lock: 4,
        features: OutputFeatures{
            maturity: 1,
            ..Default::default()
        }
    );
    tx3.lock_height = 2;
    let tx3 = Arc::new(spend_utxos(tx3, &key_manager).await.0);

    // Tx2 should not go in, but Tx3 should
    assert_eq!(
        mempool.insert(tx2.clone()).await.unwrap(),
        TxStorageResponse::NotStoredTimeLocked
    );
    assert_eq!(
        mempool.insert(tx3.clone()).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    // Spend tx3, so that the height of the chain will increase
    generate_block(
        &store,
        &mut blocks,
        vec![tx3.deref().clone()],
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();

    // Block height increased, so tx2 should no go in.
    assert_eq!(mempool.insert(tx2).await.unwrap(), TxStorageResponse::UnconfirmedPool);
}

// maturities not being checked before
#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_retrieve() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let mempool_validator = TransactionChainLinkedValidator::new(store.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![1 * T, 1 * T, 1 * T, 1 * T, 1 * T, 1 * T, 1 * T]
    )];
    // "Mine" Block 1
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();

    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 0);
    assert_eq!(stats.reorg_txs, 0);

    // 1-Block, 8 UTXOs, empty mempool
    let txs = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![], fee: 30*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![], fee: 20*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 40*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][3].clone()], to: vec![], fee: 50*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][4].clone()], to: vec![], fee: 20*uT, lock: 2, features: OutputFeatures::default()),
        // will get rejected as its time-locked
        txn_schema!(from: vec![outputs[1][5].clone()], to: vec![], fee: 20*uT, lock: 3, features: OutputFeatures::default()),
        // Will be time locked when a tx is added to mempool with this as an input:
        txn_schema!(from: vec![outputs[1][6].clone()], to: vec![800_000*uT], fee: 60*uT, lock: 0,
            features: OutputFeatures{
                maturity: 4,
                ..Default::default()
        }),
        // Will be time locked when a tx is added to mempool with this as an input:
        txn_schema!(from: vec![outputs[1][7].clone()], to: vec![800_000*uT], fee: 25*uT, lock: 0,
            features: OutputFeatures{
            maturity: 2,
            ..Default::default()
        }),
    ];
    let (tx, utxos) = schema_to_transaction(&txs, &key_manager).await;
    for t in &tx {
        mempool.insert(t.clone()).await.unwrap();
    }
    // 1-block, 8 UTXOs, 7 txs in mempool
    let weighting = consensus_manager.consensus_constants(0).transaction_weight_params();
    let weight = tx[6].calculate_weight(weighting).expect("Failed to calculate weight") +
        tx[2].calculate_weight(weighting).expect("Failed to calculate weight") +
        tx[3].calculate_weight(weighting).expect("Failed to calculate weight");
    let retrieved_txs = mempool.retrieve(weight).await.unwrap();
    assert_eq!(retrieved_txs.len(), 3);
    assert!(retrieved_txs.contains(&tx[6]));
    assert!(retrieved_txs.contains(&tx[2]));
    assert!(retrieved_txs.contains(&tx[3]));
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 7);
    assert_eq!(stats.reorg_txs, 0);

    let block2_txns = vec![
        tx[0].deref().clone(),
        tx[1].deref().clone(),
        tx[2].deref().clone(),
        tx[6].deref().clone(),
        tx[7].deref().clone(),
    ];
    // "Mine" block 2
    generate_block(&store, &mut blocks, block2_txns, &consensus_manager, &key_manager)
        .await
        .unwrap();
    outputs.push(utxos);
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();
    // 2-blocks, 2 unconfirmed txs in mempool
    // We mined 5 tx's so 2 should be left in the mempool with the 5 mined ones being in the reorg pool
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 2);
    assert_eq!(stats.reorg_txs, 5);
    // Create transactions wih time-locked inputs
    // Only one will be allowed into the mempool as the one still as a maturity lock on the input.
    let txs = vec![
        txn_schema!(from: vec![outputs[2][6].clone()], to: vec![], fee: 80*uT, lock: 0, features: OutputFeatures::default()),
        // account for change output
        txn_schema!(from: vec![outputs[2][8].clone()], to: vec![], fee: 40*uT, lock: 0, features: OutputFeatures::default()),
    ];
    let (tx2, _) = schema_to_transaction(&txs, &key_manager).await;
    for t in &tx2 {
        mempool.insert(t.clone()).await.unwrap();
    }

    // Top 2 txs are tx[3] (fee/g = 50) and tx2[1] (fee/g = 40). tx2[0] (fee/g = 80) is still not matured.
    let weight = tx[3].calculate_weight(weighting).expect("Failed to calculate weight") +
        tx2[1].calculate_weight(weighting).expect("Failed to calculate weight");
    let retrieved_txs = mempool.retrieve(weight).await.unwrap();
    let stats = mempool.stats().await.unwrap();

    assert_eq!(stats.unconfirmed_txs, 3);
    assert_eq!(stats.reorg_txs, 5);
    assert_eq!(retrieved_txs.len(), 2);
    assert!(retrieved_txs.contains(&tx[3]));
    assert!(retrieved_txs.contains(&tx2[1]));
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_zero_conf_no_piggyback() {
    // This is the scenario described in fetch_highest_priority_txs function.
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let mempool_validator = TransactionChainLinkedValidator::new(store.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![21 * T, 11 * T, 11 * T, 16 * T]
    )];
    // "Mine" Block 1
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();

    let (tx_d, _tx_d_out) = spend_utxos(
        txn_schema!(
            from: vec![outputs[1][1].clone()],
            to: vec![5 * T, 5 * T],
            fee: 12*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx_d.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    let (tx_c, tx_c_out) = spend_utxos(
        txn_schema!(
            from: vec![outputs[1][0].clone()],
            to: vec![15 * T, 5 * T],
            fee: 14*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx_c.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    let (tx_b, tx_b_out) = spend_utxos(
        txn_schema!(
            from: vec![tx_c_out[0].clone()],
            to: vec![7 * T, 4 * T],
            fee: 2*uT, lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx_b.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    let (tx_a, _tx_a_out) = spend_utxos(
        txn_schema!(
            from: vec![tx_b_out[1].clone()],
            to: vec![2 * T, 1 * T],
            fee: 20*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;

    assert_eq!(
        mempool.insert(Arc::new(tx_a.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    let weight = mempool.stats().await.unwrap().unconfirmed_weight - 1;
    let retrieved_txs = mempool.retrieve(weight).await.unwrap();
    assert_eq!(retrieved_txs.len(), 3);
    assert!(retrieved_txs.contains(&Arc::new(tx_d)));
    assert!(retrieved_txs.contains(&Arc::new(tx_c)));
    assert!(retrieved_txs.contains(&Arc::new(tx_b)));
    assert!(!retrieved_txs.contains(&Arc::new(tx_a)));
}

#[tokio::test]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
async fn test_zero_conf() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let mempool_validator = TransactionChainLinkedValidator::new(store.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![21 * T, 11 * T, 11 * T, 16 * T]
    )];
    // "Mine" Block 1
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();

    // This transaction graph will be created, containing 3 levels of zero-conf transactions, inheriting dependent
    // outputs from multiple parents
    //
    // tx01   tx02   tx03   tx04    Basis transactions using mined inputs (lowest fees, increases left to right)
    //   | \    |      |      |
    //   |  \   |      |      |
    //   |   \  |      |      |
    // tx11   tx12   tx13   tx14    Zero-conf level 1 transactions (fees up from previous, increases left to right)
    //   | \    | \    |   /  |
    //   |  |   |  \   |  |   |
    //   |  |   |   \  |  |   |
    // tx21 | tx22   tx23 | tx24    Zero-conf level 2 transactions (fees up from previous, increases left to right)
    //   |  |   |      |  |   |
    //   |   \  |      | /    |
    //   |    \ |      |/     |
    // tx31   tx32   tx33   tx34    Zero-conf level 3 transactions (highest fees, increases left to right)

    // Create 4 original transactions, only submit 3 (hold back tx02)
    let (tx01, tx01_out) = spend_utxos(
        txn_schema!(
            from: vec![outputs[1][0].clone()],
            to: vec![15 * T, 5 * T],
            fee: 10*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx02, tx02_out) = spend_utxos(
        txn_schema!(
            from: vec![outputs[1][1].clone()],
            to: vec![5 * T, 5 * T],
            fee: 20*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx03, tx03_out) = spend_utxos(
        txn_schema!(
            from: vec![outputs[1][2].clone()],
            to: vec![5 * T, 5 * T],
            fee: 30*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx04, tx04_out) = spend_utxos(
        txn_schema!(
            from: vec![outputs[1][3].clone()],
            to: vec![5 * T, 5 * T],
            fee: 40*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx01.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx03.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx04.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    // Create 4 zero-conf level 1 transactions, try to submit all
    let (tx11, tx11_out) = spend_utxos(
        txn_schema!(
            from: vec![tx01_out[0].clone()],
            to: vec![7 * T, 4 * T],
            fee: 50*uT, lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx12, tx12_out) = spend_utxos(
        txn_schema!(
            from: vec![tx01_out[1].clone(), tx02_out[0].clone(), tx02_out[1].clone()],
            to: vec![7 * T, 4 * T],
            fee: 60*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx13, tx13_out) = spend_utxos(
        txn_schema!(
            from: tx03_out,
            to: vec![4 * T, 4 * T],
            fee: 70*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx14, tx14_out) = spend_utxos(
        txn_schema!(
            from: tx04_out,
            to: vec![10 * T, 4 * T],
            fee: 80*uT, lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx11.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx12.clone())).await.unwrap(),
        TxStorageResponse::NotStoredOrphan
    );
    assert_eq!(
        mempool.insert(Arc::new(tx13.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx14.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    // Create 4 zero-conf level 2 transactions, try to submit all
    let (tx21, tx21_out) = spend_utxos(
        txn_schema!(
            from: vec![tx11_out[0].clone()],
            to: vec![3 * T, 3 * T],
            fee: 90*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx22, tx22_out) = spend_utxos(
        txn_schema!(
            from: vec![tx12_out[0].clone()],
            to: vec![3 * T, 3 * T],
            fee: 100*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx23, tx23_out) = spend_utxos(
        txn_schema!(
            from: vec![tx12_out[1].clone(), tx13_out[0].clone(), tx13_out[1].clone()],
            to: vec![3 * T, 3 * T],
            fee: 110*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx24, tx24_out) = spend_utxos(
        txn_schema!(
            from: vec![tx14_out[0].clone()],
            to: vec![3 * T, 3 * T],
            fee: 120*uT, lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx21.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx22.clone())).await.unwrap(),
        TxStorageResponse::NotStoredOrphan
    );
    assert_eq!(
        mempool.insert(Arc::new(tx23.clone())).await.unwrap(),
        TxStorageResponse::NotStoredOrphan
    );
    assert_eq!(
        mempool.insert(Arc::new(tx24.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    // Create 4 zero-conf level 3 transactions, try to submit all
    let (tx31, _) = spend_utxos(
        txn_schema!(
            from: tx21_out,
            to: vec![2 * T, 2 * T],
            fee: 130*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx32, _) = spend_utxos(
        txn_schema!(
            from: vec![tx11_out[1].clone(), tx22_out[0].clone(), tx22_out[1].clone()],
            to: vec![2 * T, 2 * T],
            fee: 140*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx33, _) = spend_utxos(
        txn_schema!(
            from: vec![tx14_out[1].clone(), tx23_out[0].clone(), tx23_out[1].clone()],
            to: vec![2 * T, 2 * T],
            fee: 150*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    let (tx34, _) = spend_utxos(
        txn_schema!(
            from: tx24_out,
            to: vec![2 * T, 2 * T],
            fee: 160*uT,
            lock: 0,
            features: OutputFeatures::default()
        ),
        &key_manager,
    )
    .await;
    assert_eq!(
        mempool.insert(Arc::new(tx31.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx32.clone())).await.unwrap(),
        TxStorageResponse::NotStoredOrphan
    );
    assert_eq!(
        mempool.insert(Arc::new(tx33.clone())).await.unwrap(),
        TxStorageResponse::NotStoredOrphan
    );
    assert_eq!(
        mempool.insert(Arc::new(tx34.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    // Try to retrieve all transactions in the mempool (a couple of our transactions should be missing from retrieved)
    let retrieved_txs = mempool
        .retrieve(mempool.stats().await.unwrap().unconfirmed_weight)
        .await
        .unwrap();
    assert_eq!(retrieved_txs.len(), 10);
    assert!(retrieved_txs.contains(&Arc::new(tx01.clone())));
    assert!(!retrieved_txs.contains(&Arc::new(tx02.clone()))); // Missing
    assert!(retrieved_txs.contains(&Arc::new(tx03.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx04.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx11.clone())));
    assert!(!retrieved_txs.contains(&Arc::new(tx12.clone()))); // Missing
    assert!(retrieved_txs.contains(&Arc::new(tx13.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx14.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx21.clone())));
    assert!(!retrieved_txs.contains(&Arc::new(tx22.clone()))); // Missing
    assert!(!retrieved_txs.contains(&Arc::new(tx23.clone()))); // Missing
    assert!(retrieved_txs.contains(&Arc::new(tx24.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx31.clone())));
    assert!(!retrieved_txs.contains(&Arc::new(tx32.clone()))); // Missing
    assert!(!retrieved_txs.contains(&Arc::new(tx33.clone()))); // Missing
    assert!(retrieved_txs.contains(&Arc::new(tx34.clone())));

    // Submit the missing original transactions
    assert_eq!(
        mempool.insert(Arc::new(tx02.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    // Re-submit failed zero-conf level 1 transactions
    assert_eq!(
        mempool.insert(Arc::new(tx12.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    // Re-submit failed zero-conf level 2 transactions
    assert_eq!(
        mempool.insert(Arc::new(tx22.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx23.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    // Re-submit failed zero-conf level 3 transactions
    assert_eq!(
        mempool.insert(Arc::new(tx32.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool.insert(Arc::new(tx33.clone())).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    // Try to retrieve all transactions in the mempool (all transactions should be retrieved)
    let retrieved_txs = mempool
        .retrieve(mempool.stats().await.unwrap().unconfirmed_weight)
        .await
        .unwrap();
    assert_eq!(retrieved_txs.len(), 16);
    assert!(retrieved_txs.contains(&Arc::new(tx01.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx02.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx03.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx04.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx11.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx12.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx13.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx14.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx21.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx22.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx23.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx24.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx31.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx32.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx33.clone())));
    assert!(retrieved_txs.contains(&Arc::new(tx34.clone())));

    // Verify that a higher priority transaction is not retrieved due to its zero-conf dependency instead of the lowest
    // priority transaction
    let weight = mempool.stats().await.unwrap().unconfirmed_weight - 1;
    let retrieved_txs = mempool.retrieve(weight).await.unwrap();
    assert_eq!(retrieved_txs.len(), 15);
    assert!(retrieved_txs.contains(&Arc::new(tx01)));
    assert!(retrieved_txs.contains(&Arc::new(tx02)));
    assert!(retrieved_txs.contains(&Arc::new(tx03)));
    assert!(retrieved_txs.contains(&Arc::new(tx04)));
    assert!(retrieved_txs.contains(&Arc::new(tx11)));
    assert!(retrieved_txs.contains(&Arc::new(tx12)));
    assert!(retrieved_txs.contains(&Arc::new(tx13)));
    assert!(retrieved_txs.contains(&Arc::new(tx14)));
    assert!(retrieved_txs.contains(&Arc::new(tx21)));
    assert!(retrieved_txs.contains(&Arc::new(tx22)));
    assert!(retrieved_txs.contains(&Arc::new(tx23)));
    assert!(retrieved_txs.contains(&Arc::new(tx24)));
    assert!(retrieved_txs.contains(&Arc::new(tx31)));
    assert!(!retrieved_txs.contains(&Arc::new(tx32))); // Missing
    assert!(retrieved_txs.contains(&Arc::new(tx33)));
    assert!(retrieved_txs.contains(&Arc::new(tx34)));
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_reorg() {
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let mempool_validator =
        TransactionFullValidator::new(CryptoFactories::default(), true, db.clone(), consensus_manager.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );

    // "Mine" Block 1
    let txs = vec![
        txn_schema!(from: vec![outputs[0][0].clone()], to: vec![1 * T, 1 * T], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
    ];
    generate_new_block(
        &mut db,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();

    // "Mine" block 2
    let schemas = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
    ];
    let (txns2, utxos) = schema_to_transaction(&schemas, &key_manager).await;
    outputs.push(utxos);
    for tx in &txns2 {
        mempool.insert(tx.clone()).await.unwrap();
    }
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 3);
    let txns2 = txns2.iter().map(|t| t.deref().clone()).collect();
    generate_block(&db, &mut blocks, txns2, &consensus_manager, &key_manager)
        .await
        .unwrap();
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();

    // "Mine" block 3
    let schemas = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[2][1].clone()], to: vec![], fee: 25*uT, lock: 5, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[2][2].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
    ];
    let (txns3, utxos) = schema_to_transaction(&schemas, &key_manager).await;
    outputs.push(utxos);
    for tx in &txns3 {
        mempool.insert(tx.clone()).await.unwrap();
    }
    let txns3: Vec<Transaction> = txns3.iter().map(|t| t.deref().clone()).collect();

    generate_block(
        &db,
        &mut blocks,
        vec![txns3[0].clone(), txns3[2].clone()],
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    mempool.process_published_block(blocks[3].to_arc_block()).await.unwrap();

    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 0);
    // assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.reorg_txs, 5);

    db.rewind_to_height(2).unwrap();

    let template = chain_block(blocks[2].block(), vec![], &consensus_manager, &key_manager).await;
    let reorg_block3 = db.prepare_new_block(template).unwrap();

    mempool
        .process_reorg(vec![blocks[3].to_arc_block()], vec![reorg_block3.into()])
        .await
        .unwrap();
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 2);
    // assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.reorg_txs, 3);

    // "Mine" block 4
    let template = chain_block(blocks[2].block(), vec![], &consensus_manager, &key_manager).await;
    let reorg_block4 = db.prepare_new_block(template).unwrap();

    // test that process_reorg can handle the case when removed_blocks is empty
    // see https://github.com/tari-project/tari/issues/2101#issuecomment-680726940
    mempool.process_reorg(vec![], vec![reorg_block4.into()]).await.unwrap();
}

static EMISSION: [u64; 2] = [10, 10];
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::identity_op)]
async fn receive_and_propagate_transaction() {
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_coinbase_lockheight(100)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (block0, utxo) = create_genesis_block(&consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0)
        .build()
        .unwrap();
    let (mut alice_node, mut bob_node, mut carol_node, _consensus_manager) =
        create_network_with_3_base_nodes_with_config(
            MempoolServiceConfig::default(),
            LivenessConfig::default(),
            consensus_manager,
            temp_dir.path().to_str().unwrap(),
        )
        .await;
    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });

    let (tx, _) = spend_utxos(
        txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]),
        &key_manager,
    )
    .await;
    let (orphan, _, _) = tx!(1*T, fee: 100*uT, &key_manager).expect("Failed to get tx");
    let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
    let orphan_excess_sig = orphan.body.kernels()[0].excess_sig.clone();
    assert!(alice_node.mempool.insert(Arc::new(tx.clone())).await.is_ok());
    assert!(alice_node.mempool.insert(Arc::new(orphan.clone())).await.is_ok());

    alice_node
        .outbound_message_service
        .send_direct_unencrypted(
            bob_node.node_identity.public_key().clone(),
            OutboundDomainMessage::new(
                &TariMessageType::NewTransaction,
                proto::types::Transaction::try_from(tx).unwrap(),
            ),
            "mempool tests".to_string(),
        )
        .await
        .unwrap();
    alice_node
        .outbound_message_service
        .send_direct_unencrypted(
            carol_node.node_identity.public_key().clone(),
            OutboundDomainMessage::new(
                &TariMessageType::NewTransaction,
                proto::types::Transaction::try_from(orphan).unwrap(),
            ),
            "mempool tests".to_string(),
        )
        .await
        .unwrap();

    async_assert_eventually!(
        bob_node
            .mempool
            .has_tx_with_excess_sig(tx_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::NotStored,
        max_attempts = 20,
        interval = Duration::from_millis(1000)
    );
    async_assert_eventually!(
        carol_node
            .mempool
            .has_tx_with_excess_sig(tx_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::NotStored,
        max_attempts = 10,
        interval = Duration::from_millis(1000)
    );
    // Carol got sent the orphan tx directly, so it will be in her mempool
    async_assert_eventually!(
        carol_node
            .mempool
            .has_tx_with_excess_sig(orphan_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::NotStored,
        max_attempts = 10,
        interval = Duration::from_millis(1000)
    );
    // It's difficult to test a negative here, but let's at least make sure that the orphan TX was not propagated
    // by the time we check it
    async_assert_eventually!(
        bob_node
            .mempool
            .has_tx_with_excess_sig(orphan_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::NotStored,
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn consensus_validation_large_tx() {
    let network = Network::LocalNet;
    // We dont want to compute the 19500 limit of local net, so we create smaller blocks
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .with_coinbase_lockheight(1)
        .with_max_block_transaction_weight(500)
        .build();
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) =
        create_new_blockchain_with_constants(network, consensus_constants).await;
    let mempool_validator = TransactionFullValidator::new(
        CryptoFactories::default(),
        true,
        store.clone(),
        consensus_manager.clone(),
    );
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    // Create a block with 1 output
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![5 * T])];
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();

    // build huge tx manually - the TransactionBuilder already has checks for max inputs/outputs
    let fee_per_gram = 15;
    let input_count = 1;
    let output_count = 39;
    let amount = MicroMinotari::from(5_000_000);

    let input = outputs[1][0].clone();
    let inputs = vec![input.to_transaction_input(&key_manager).await.unwrap()];
    let input_script_keys = vec![input.script_key_id];

    let fee = Fee::new(*consensus_manager.consensus_constants(0).transaction_weight_params()).calculate(
        fee_per_gram.into(),
        1,
        input_count,
        output_count,
        0,
    );
    let amount_per_output = (amount - fee) / output_count as u64;
    let amount_for_last_output = (amount - fee) - amount_per_output * (output_count as u64 - 1);
    let mut wallet_outputs = Vec::with_capacity(output_count);
    let (input_kernel_nonce, mut pub_nonce) = key_manager
        .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
        .await
        .unwrap();
    let mut pub_excess = PublicKey::default() -
        key_manager
            .get_txo_kernel_signature_excess_with_offset(&input.spending_key_id, &input_kernel_nonce)
            .await
            .unwrap();
    let mut sender_offsets = Vec::new();

    for i in 0..output_count {
        let test_params = TestParams::new(&key_manager).await;
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        let output = create_wallet_output_with_data(
            script!(Nop),
            OutputFeatures::default(),
            &test_params,
            output_amount,
            &key_manager,
        )
        .await
        .unwrap();
        pub_excess = pub_excess +
            key_manager
                .get_txo_kernel_signature_excess_with_offset(&output.spending_key_id, &test_params.kernel_nonce_key_id)
                .await
                .unwrap();
        pub_nonce = pub_nonce + test_params.kernel_nonce_key_pk;
        sender_offsets.push(test_params.sender_offset_key_id.clone());

        wallet_outputs.push((output.clone(), test_params.kernel_nonce_key_id));
    }

    let mut agg_sig = Signature::default();
    let mut outputs = Vec::new();
    let mut offset = PrivateKey::default();
    let tx_meta = TransactionMetadata::new(fee, 0);
    let kernel_version = TransactionKernelVersion::get_current_version();
    let kernel_message = TransactionKernel::build_kernel_signature_message(
        &kernel_version,
        tx_meta.fee,
        tx_meta.lock_height,
        &tx_meta.kernel_features,
        &tx_meta.burn_commitment,
    );
    for (output, nonce_id) in wallet_outputs {
        outputs.push(output.to_transaction_output(&key_manager).await.unwrap());
        offset = &offset +
            &key_manager
                .get_txo_private_kernel_offset(&output.spending_key_id, &nonce_id)
                .await
                .unwrap();
        let sig = key_manager
            .get_partial_txo_kernel_signature(
                &output.spending_key_id,
                &nonce_id,
                &pub_nonce,
                &pub_excess,
                &kernel_version,
                &kernel_message,
                &tx_meta.kernel_features,
                TxoStage::Output,
            )
            .await
            .unwrap();
        agg_sig = &agg_sig + sig;
    }

    offset = &offset -
        &key_manager
            .get_txo_private_kernel_offset(&input.spending_key_id, &input_kernel_nonce)
            .await
            .unwrap();
    let sig = key_manager
        .get_partial_txo_kernel_signature(
            &input.spending_key_id,
            &input_kernel_nonce,
            &pub_nonce,
            &pub_excess,
            &kernel_version,
            &kernel_message,
            &tx_meta.kernel_features,
            TxoStage::Input,
        )
        .await
        .unwrap();
    agg_sig = &agg_sig + sig;

    let kernel = KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(0)
        .with_excess(&Commitment::from_public_key(&pub_excess))
        .with_features(tx_meta.kernel_features)
        .with_signature(agg_sig)
        .build()
        .unwrap();
    let kernels = vec![kernel];
    let script_offset = key_manager
        .get_script_offset(&input_script_keys, &sender_offsets)
        .await
        .unwrap();
    let mut tx = Transaction::new(inputs, outputs, kernels, offset, script_offset);
    tx.body.sort();

    let height = blocks.len() as u64;
    let constants = consensus_manager.consensus_constants(height);

    // make sure the tx was correctly made and is valid
    let factories = CryptoFactories::default();
    let validator = TransactionInternalConsistencyValidator::new(true, consensus_manager.clone(), factories);
    let err = validator.validate(&tx, None, None, u64::MAX).unwrap_err();
    assert!(matches!(err, ValidationError::BlockTooLarge { .. }));

    let weighting = constants.transaction_weight_params();
    let weight = tx.calculate_weight(weighting).expect("Failed to calculate weight");

    // check the tx weight is more than the max for 1 block
    assert!(weight > constants.max_block_transaction_weight());

    let response = mempool.insert(Arc::new(tx)).await.unwrap();
    // make sure the tx was not accepted into the mempool
    assert!(matches!(response, TxStorageResponse::NotStored));
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn validation_reject_min_fee() {
    let network = Network::LocalNet;
    // We dont want to compute the 19500 limit of local net, so we create smaller blocks
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .with_coinbase_lockheight(1)
        .with_max_block_transaction_weight(500)
        .build();
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) =
        create_new_blockchain_with_constants(network, consensus_constants).await;
    let mempool_validator = TransactionFullValidator::new(
        CryptoFactories::default(),
        true,
        store.clone(),
        consensus_manager.clone(),
    );
    let mut mempool_config = MempoolConfig::default();
    mempool_config.unconfirmed_pool.min_fee = 1;
    let mempool = Mempool::new(mempool_config, consensus_manager.clone(), Box::new(mempool_validator));
    // Create a block with 1 output
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![5 * T])];
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();

    // build huge 0 fee tx manually
    let input = outputs[1][0].clone();
    let inputs = vec![input.to_transaction_input(&key_manager).await.unwrap()];
    let input_script_keys = vec![input.script_key_id];

    let fee = 0.into();

    let (input_kernel_nonce, mut pub_nonce) = key_manager
        .get_next_key(TransactionKeyManagerBranch::KernelNonce.get_branch_key())
        .await
        .unwrap();
    let mut pub_excess = PublicKey::default() -
        key_manager
            .get_txo_kernel_signature_excess_with_offset(&input.spending_key_id, &input_kernel_nonce)
            .await
            .unwrap();
    let mut sender_offsets = Vec::new();

    let test_params = TestParams::new(&key_manager).await;
    let wallet_output = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &test_params,
        input.value,
        &key_manager,
    )
    .await
    .unwrap();
    pub_excess = pub_excess +
        key_manager
            .get_txo_kernel_signature_excess_with_offset(
                &wallet_output.spending_key_id,
                &test_params.kernel_nonce_key_id,
            )
            .await
            .unwrap();
    pub_nonce = pub_nonce + test_params.kernel_nonce_key_pk;
    sender_offsets.push(test_params.sender_offset_key_id.clone());

    let mut agg_sig = Signature::default();
    let mut offset = PrivateKey::default();
    let tx_meta = TransactionMetadata::new(fee, 0);
    let kernel_version = TransactionKernelVersion::get_current_version();
    let kernel_message = TransactionKernel::build_kernel_signature_message(
        &kernel_version,
        tx_meta.fee,
        tx_meta.lock_height,
        &tx_meta.kernel_features,
        &tx_meta.burn_commitment,
    );

    let tx_output = wallet_output.to_transaction_output(&key_manager).await.unwrap();
    offset = &offset +
        &key_manager
            .get_txo_private_kernel_offset(&wallet_output.spending_key_id, &test_params.kernel_nonce_key_id)
            .await
            .unwrap();
    let sig = key_manager
        .get_partial_txo_kernel_signature(
            &wallet_output.spending_key_id,
            &test_params.kernel_nonce_key_id,
            &pub_nonce,
            &pub_excess,
            &kernel_version,
            &kernel_message,
            &tx_meta.kernel_features,
            TxoStage::Output,
        )
        .await
        .unwrap();
    agg_sig = &agg_sig + sig;

    offset = &offset -
        &key_manager
            .get_txo_private_kernel_offset(&input.spending_key_id, &input_kernel_nonce)
            .await
            .unwrap();
    let sig = key_manager
        .get_partial_txo_kernel_signature(
            &input.spending_key_id,
            &input_kernel_nonce,
            &pub_nonce,
            &pub_excess,
            &kernel_version,
            &kernel_message,
            &tx_meta.kernel_features,
            TxoStage::Input,
        )
        .await
        .unwrap();
    agg_sig = &agg_sig + sig;

    let kernel = KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(0)
        .with_excess(&Commitment::from_public_key(&pub_excess))
        .with_features(tx_meta.kernel_features)
        .with_signature(agg_sig)
        .build()
        .unwrap();
    let kernels = vec![kernel];
    let script_offset = key_manager
        .get_script_offset(&input_script_keys, &sender_offsets)
        .await
        .unwrap();
    let mut tx = Transaction::new(inputs, vec![tx_output], kernels, offset, script_offset);
    tx.body.sort();

    // make sure the tx was correctly made and is valid
    let factories = CryptoFactories::default();
    let validator = TransactionInternalConsistencyValidator::new(true, consensus_manager.clone(), factories);
    validator.validate(&tx, None, None, u64::MAX).unwrap();
    let response = mempool.insert(Arc::new(tx)).await.unwrap();
    // make sure the tx was not accepted into the mempool
    assert!(matches!(response, TxStorageResponse::NotStoredFeeTooLow));
}

#[tokio::test]
#[allow(clippy::erasing_op)]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
async fn consensus_validation_versions() {
    use tari_core::transactions::transaction_components::{
        OutputFeaturesVersion,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutputVersion,
    };

    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
    let cc = consensus_manager.consensus_constants(0);

    // check the current localnet defaults
    assert_eq!(
        cc.input_version_range().clone(),
        TransactionInputVersion::V0..=TransactionInputVersion::V0
    );
    assert_eq!(
        cc.kernel_version_range().clone(),
        TransactionKernelVersion::V0..=TransactionKernelVersion::V0
    );
    assert_eq!(
        cc.output_version_range().clone().outputs,
        TransactionOutputVersion::V0..=TransactionOutputVersion::V0
    );
    assert_eq!(
        cc.output_version_range().clone().features,
        OutputFeaturesVersion::V0..=OutputFeaturesVersion::V0
    );

    let mempool_validator = TransactionFullValidator::new(
        CryptoFactories::default(),
        true,
        store.clone(),
        consensus_manager.clone(),
    );

    let _mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );

    let test_params = TestParams::new(&key_manager).await;
    let params = UtxoTestParams::with_value(1 * T);
    let output_v0_features_v0 = test_params.create_output(params, &key_manager).await.unwrap();
    assert_eq!(output_v0_features_v0.version, TransactionOutputVersion::V0);
    assert_eq!(output_v0_features_v0.features.version, OutputFeaturesVersion::V0);

    let test_params = TestParams::new(&key_manager).await;
    let mut params = UtxoTestParams::with_value(1 * T);
    params.output_version = Some(TransactionOutputVersion::V1);
    let output_v1_features_v0 = test_params.create_output(params, &key_manager).await.unwrap();
    assert_eq!(output_v1_features_v0.version, TransactionOutputVersion::V1);
    assert_eq!(output_v1_features_v0.features.version, OutputFeaturesVersion::V0);

    let features_v1 = OutputFeatures::new(
        OutputFeaturesVersion::V1,
        OutputType::default(),
        0,
        Default::default(),
        None,
        RangeProofType::BulletProofPlus,
    );

    let test_params = TestParams::new(&key_manager).await;
    let mut params = UtxoTestParams::with_value(1 * T);
    params.features = features_v1.clone();
    let output_v0_features_v1 = test_params.create_output(params, &key_manager).await.unwrap();
    assert_eq!(output_v0_features_v1.version, TransactionOutputVersion::V0);
    assert_eq!(output_v0_features_v1.features.version, OutputFeaturesVersion::V1);

    let test_params = TestParams::new(&key_manager).await;
    let mut params = UtxoTestParams::with_value(1 * T);
    params.features = features_v1;
    let mut output_v1_features_v1 = test_params.create_output(params, &key_manager).await.unwrap();
    output_v1_features_v1.version = TransactionOutputVersion::V1;
    assert_eq!(output_v1_features_v1.version, TransactionOutputVersion::V1);
    assert_eq!(output_v1_features_v1.features.version, OutputFeaturesVersion::V1);

    let schema = txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![2 * T, 2 * T, 2 * T, 2 * T, 2 * T]
    );
    let txs = vec![schema];
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();
    let validator = TransactionInternalConsistencyValidator::new(true, consensus_manager, CryptoFactories::default());
    // Cases:
    // invalid input version
    let tx_schema = TransactionSchema {
        from: vec![outputs[1][0].clone()],
        to: vec![1 * T],
        to_outputs: vec![],
        fee: 25.into(),
        lock_height: 0,
        features: Default::default(),
        script: script![Nop],
        input_data: None,
        covenant: Default::default(),
        input_version: Some(TransactionInputVersion::V1),
        output_version: None,
    };
    let (tx, _) = spend_utxos(tx_schema, &key_manager).await;
    validator.validate(&tx, Some(25.into()), None, u64::MAX).unwrap_err();

    // invalid output version
    let tx_schema = TransactionSchema {
        from: vec![outputs[1][1].clone()],
        to: vec![],
        to_outputs: vec![output_v1_features_v0],
        fee: 25.into(),
        lock_height: 0,
        features: Default::default(),
        script: script![Nop],
        input_data: None,
        covenant: Default::default(),
        input_version: None,
        output_version: Some(TransactionOutputVersion::V1),
    };

    let (tx, _) = spend_utxos(tx_schema, &key_manager).await;
    validator.validate(&tx, Some(25.into()), None, u64::MAX).unwrap_err();

    // invalid output features version
    let tx_schema = TransactionSchema {
        from: vec![outputs[1][2].clone()],
        to: vec![],
        to_outputs: vec![output_v0_features_v1],
        fee: 25.into(),
        lock_height: 0,
        features: Default::default(),
        script: script![Nop],
        input_data: None,
        covenant: Default::default(),
        input_version: None,
        output_version: None,
    };

    let (tx, _) = spend_utxos(tx_schema, &key_manager).await;
    validator.validate(&tx, Some(25.into()), None, u64::MAX).unwrap_err();
}

#[tokio::test]
async fn consensus_validation_unique_excess_sig() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;

    let mempool_validator = TransactionFullValidator::new(
        CryptoFactories::default(),
        true,
        store.clone(),
        consensus_manager.clone(),
    );

    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );

    // Create a block with 5 outputs
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![2 * T, 2 * T, 2 * T, 2 * T, 2 * T], fee: 25.into(), lock: 0, features: OutputFeatures::default()
    )];
    generate_new_block(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        &consensus_manager,
        &key_manager,
    )
    .await
    .unwrap();

    let schema = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1_500_000 * uT]);
    let (tx1, _) = spend_utxos(schema.clone(), &key_manager).await;
    generate_block(&store, &mut blocks, vec![tx1.clone()], &consensus_manager, &key_manager)
        .await
        .unwrap();

    // trying to submit a transaction with an existing excess signature already in the chain is an error
    let tx = Arc::new(tx1);
    let response = mempool.insert(tx).await.unwrap();
    assert!(matches!(response, TxStorageResponse::NotStoredAlreadyMined));
}

#[tokio::test]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
async fn block_event_and_reorg_event_handling() {
    // This test creates 2 nodes Alice and Bob
    // Then creates 2 chains B1 -> B2A (diff 1) and B1 -> B2B (diff 10)
    // There are 5 transactions created
    // TX1 the base transaction and then TX2A and TX3A that spend it
    // Double spends TX2B and TX3B are also created spending TX1
    // Both nodes have all transactions in their mempools
    // When block B2A is submitted, then both nodes have TX2A and TX3A in their reorg pools
    // When block B2B is submitted with TX2B, TX3B, then TX2A, TX3A are discarded (Not Stored)
    let network = Network::LocalNet;
    let key_manager = create_test_core_key_manager_with_memory_db();
    let consensus_constants = ConsensusConstantsBuilder::new(Network::LocalNet)
        .with_coinbase_lockheight(1)
        .build();

    let temp_dir = tempdir().unwrap();
    let (block0, utxos0) =
        create_genesis_block_with_coinbase_value(100_000_000.into(), &consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants.clone())
        .with_block(block0.clone())
        .build()
        .unwrap();
    let (mut alice, mut bob, consensus_manager) = create_network_with_2_base_nodes_with_config(
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    )
    .await;
    alice.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });

    // Bob creates Block 1 and sends it to Alice. Alice adds it to her chain and creates a block event that the Mempool
    // service will receive.
    let (tx1, utxos1) =
        schema_to_transaction(&[txn_schema!(from: vec![utxos0], to: vec![1 * T, 1 * T])], &key_manager).await;
    let (txs_a, _utxos2) = schema_to_transaction(
        &[
            txn_schema!(from: vec![utxos1[0].clone()], to: vec![400_000 * uT, 590_000 * uT]),
            txn_schema!(from: vec![utxos1[1].clone()], to: vec![750_000 * uT, 240_000 * uT]),
        ],
        &key_manager,
    )
    .await;
    let (txs_b, _utxos3) = schema_to_transaction(
        &[
            txn_schema!(from: vec![utxos1[0].clone()], to: vec![100_000 * uT, 890_000 * uT]),
            txn_schema!(from: vec![utxos1[1].clone()], to: vec![850_000 * uT, 140_000 * uT]),
        ],
        &key_manager,
    )
    .await;
    let tx1 = (*tx1[0]).clone();
    let tx2a = (*txs_a[0]).clone();
    let tx3a = (*txs_a[1]).clone();
    let tx2b = (*txs_b[0]).clone();
    let tx3b = (*txs_b[1]).clone();
    let tx1_excess_sig = tx1.body.kernels()[0].excess_sig.clone();
    let tx2a_excess_sig = tx2a.body.kernels()[0].excess_sig.clone();
    let tx3a_excess_sig = tx3a.body.kernels()[0].excess_sig.clone();
    let tx2b_excess_sig = tx2b.body.kernels()[0].excess_sig.clone();
    let tx3b_excess_sig = tx3b.body.kernels()[0].excess_sig.clone();

    // These blocks are manually constructed to allow the block event system to be used.
    let empty_block = bob
        .blockchain_db
        .prepare_new_block(chain_block(block0.block(), vec![], &consensus_manager, &key_manager).await)
        .unwrap();

    // Add one empty block, so the coinbase UTXO is no longer time-locked.
    assert!(bob.local_nci.submit_block(empty_block.clone(),).await.is_ok());
    assert!(alice.local_nci.submit_block(empty_block.clone(),).await.is_ok());
    alice.mempool.insert(Arc::new(tx1.clone())).await.unwrap();
    bob.mempool.insert(Arc::new(tx1.clone())).await.unwrap();
    let mut block1 = bob
        .blockchain_db
        .prepare_new_block(chain_block(&empty_block, vec![tx1], &consensus_manager, &key_manager).await)
        .unwrap();
    find_header_with_achieved_difficulty(&mut block1.header, Difficulty::from_u64(1).unwrap());
    // Add Block1 - tx1 will be moved to the ReorgPool.
    assert!(bob.local_nci.submit_block(block1.clone(),).await.is_ok());
    async_assert_eventually!(
        alice
            .mempool
            .has_tx_with_excess_sig(tx1_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::ReorgPool,
        max_attempts = 20,
        interval = Duration::from_millis(1000)
    );
    alice.mempool.insert(Arc::new(tx2a.clone())).await.unwrap();
    alice.mempool.insert(Arc::new(tx3a.clone())).await.unwrap();
    alice.mempool.insert(Arc::new(tx2b.clone())).await.unwrap();
    alice.mempool.insert(Arc::new(tx3b.clone())).await.unwrap();
    bob.mempool.insert(Arc::new(tx2a.clone())).await.unwrap();
    bob.mempool.insert(Arc::new(tx3a.clone())).await.unwrap();
    bob.mempool.insert(Arc::new(tx2b.clone())).await.unwrap();
    bob.mempool.insert(Arc::new(tx3b.clone())).await.unwrap();

    let mut block2a = bob
        .blockchain_db
        .prepare_new_block(chain_block(&block1, vec![tx2a, tx3a], &consensus_manager, &key_manager).await)
        .unwrap();
    find_header_with_achieved_difficulty(&mut block2a.header, Difficulty::from_u64(1).unwrap());
    // Block2b also builds on Block1 but has a stronger PoW
    let mut block2b = bob
        .blockchain_db
        .prepare_new_block(chain_block(&block1, vec![tx2b, tx3b], &consensus_manager, &key_manager).await)
        .unwrap();
    find_header_with_achieved_difficulty(&mut block2b.header, Difficulty::from_u64(10).unwrap());

    // Add Block2a - tx2b and tx3b will be discarded as double spends.
    assert!(bob.local_nci.submit_block(block2a.clone(),).await.is_ok());

    async_assert_eventually!(
        bob.mempool
            .has_tx_with_excess_sig(tx2a_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::ReorgPool,
        max_attempts = 20,
        interval = Duration::from_millis(1000)
    );
    async_assert_eventually!(
        alice
            .mempool
            .has_tx_with_excess_sig(tx2a_excess_sig.clone())
            .await
            .unwrap(),
        expect = TxStorageResponse::ReorgPool,
        max_attempts = 20,
        interval = Duration::from_millis(1000)
    );
    assert_eq!(
        alice
            .mempool
            .has_tx_with_excess_sig(tx3a_excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::ReorgPool
    );
    assert_eq!(
        alice
            .mempool
            .has_tx_with_excess_sig(tx2b_excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::ReorgPool
    );
    assert_eq!(
        alice
            .mempool
            .has_tx_with_excess_sig(tx3b_excess_sig.clone())
            .await
            .unwrap(),
        TxStorageResponse::ReorgPool
    );
}
