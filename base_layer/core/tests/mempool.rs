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

use std::{convert::TryFrom, ops::Deref, sync::Arc, time::Duration};

use helpers::{
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
use randomx_rs::RandomXFlag;
use tari_common::configuration::Network;
use tari_common_types::types::{Commitment, PrivateKey, PublicKey, Signature};
use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        state_machine_service::states::{ListeningInfo, StateInfo, StatusInfo},
    },
    consensus::{ConsensusConstantsBuilder, ConsensusManager, NetworkConsensus},
    mempool::{Mempool, MempoolConfig, MempoolServiceConfig, TxStorageResponse},
    proof_of_work::Difficulty,
    proto,
    transactions::{
        fee::Fee,
        tari_amount::{uT, MicroTari, T},
        test_helpers::{create_unblinded_output, schema_to_transaction, spend_utxos, TestParams},
        transaction_components::{KernelBuilder, OutputFeatures, OutputFlags, Transaction, TransactionOutput},
        transaction_protocol::{build_challenge, TransactionMetadata},
        CryptoFactories,
    },
    tx,
    txn_schema,
    validation::transaction_validators::{TxConsensusValidator, TxInputAndMaturityValidator},
};
use tari_crypto::{keys::PublicKey as PublicKeyTrait, script};
use tari_p2p::{services::liveness::LivenessConfig, tari_message::TariMessageType};
use tari_test_utils::async_assert_eventually;
use tempfile::tempdir;

#[allow(dead_code)]
mod helpers;

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_insert_and_process_published_block() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    let mempool_validator = TxInputAndMaturityValidator::new(store.clone());
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
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();
    // Create 6 new transactions to add to the mempool
    let (orphan, _, _) = tx!(1*T, fee: 100*uT);
    let orphan = Arc::new(orphan);

    let tx2 = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1*T], fee: 20*uT, lock: 0, features: OutputFeatures::default());
    let tx2 = Arc::new(spend_utxos(tx2).0);

    let tx3 = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![1*T],
        fee: 20*uT,
        lock: 4,
        features: OutputFeatures::with_maturity(1)
    );
    let tx3 = Arc::new(spend_utxos(tx3).0);

    let tx5 = txn_schema!(
        from: vec![outputs[1][2].clone()],
        to: vec![1*T],
        fee: 20*uT,
        lock: 3,
        features: OutputFeatures::with_maturity(2)
    );
    let tx5 = Arc::new(spend_utxos(tx5).0);
    let tx6 = txn_schema!(from: vec![outputs[1][3].clone()], to: vec![1 * T], fee: 25*uT, lock: 0, features: OutputFeatures::default());
    let tx6 = spend_utxos(tx6).0;

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
    assert_eq!(stats.total_txs, 1);
    assert_eq!(stats.unconfirmed_txs, 1);
    assert_eq!(stats.reorg_txs, 0);
    let expected_weight = consensus_manager.consensus_constants(0).transaction_weight().calculate(
        1,
        1,
        2,
        TestParams::new().get_size_for_default_metadata(2),
    );
    assert_eq!(stats.total_weight, expected_weight);

    // Spend tx2, so it goes in Reorg pool
    generate_block(&store, &mut blocks, vec![tx2.deref().clone()], &consensus_manager).unwrap();
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
    assert_eq!(stats.total_txs, 1);
    assert_eq!(stats.unconfirmed_txs, 0);
    assert_eq!(stats.reorg_txs, 1);
    assert_eq!(stats.total_weight, 0);
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_time_locked() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    let mempool_validator = TxInputAndMaturityValidator::new(store.clone());
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
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();
    // Block height should be 1
    let mut tx2 = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1*T], fee: 20*uT, lock: 0, features: OutputFeatures::default());
    tx2.lock_height = 3;
    let tx2 = Arc::new(spend_utxos(tx2).0);

    let mut tx3 = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![1*T],
        fee: 4*uT,
        lock: 4,
        features: OutputFeatures::with_maturity(1)
    );
    tx3.lock_height = 2;
    let tx3 = Arc::new(spend_utxos(tx3).0);

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
    generate_block(&store, &mut blocks, vec![tx3.deref().clone()], &consensus_manager).unwrap();
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();

    // Block height increased, so tx2 should no go in.
    assert_eq!(mempool.insert(tx2).await.unwrap(), TxStorageResponse::UnconfirmedPool);
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_retrieve() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    let mempool_validator = TxInputAndMaturityValidator::new(store.clone());
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
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();
    // 1-Block, 8 UTXOs, empty mempool
    let txs = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![], fee: 30*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![], fee: 20*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 40*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][3].clone()], to: vec![], fee: 50*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][4].clone()], to: vec![], fee: 20*uT, lock: 2, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][5].clone()], to: vec![], fee: 20*uT, lock: 3, features: OutputFeatures::default()),
        // Will be time locked when a tx is added to mempool with this as an input:
        txn_schema!(from: vec![outputs[1][6].clone()], to: vec![800_000*uT], fee: 60*uT, lock: 0,
        features: OutputFeatures::with_maturity(4)),
        // Will be time locked when a tx is added to mempool with this as an input:
        txn_schema!(from: vec![outputs[1][7].clone()], to: vec![800_000*uT], fee: 25*uT, lock: 0,
        features: OutputFeatures::with_maturity(3)),
    ];
    let (tx, utxos) = schema_to_transaction(&txs);
    for t in &tx {
        mempool.insert(t.clone()).await.unwrap();
    }
    // 1-block, 8 UTXOs, 8 txs in mempool
    let weighting = consensus_manager.consensus_constants(0).transaction_weight();
    let weight =
        tx[6].calculate_weight(weighting) + tx[2].calculate_weight(weighting) + tx[3].calculate_weight(weighting);
    let retrieved_txs = mempool.retrieve(weight).await.unwrap();
    assert_eq!(retrieved_txs.len(), 3);
    assert!(retrieved_txs.contains(&tx[6]));
    assert!(retrieved_txs.contains(&tx[2]));
    assert!(retrieved_txs.contains(&tx[3]));
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 7);
    // assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.reorg_txs, 0);

    let block2_txns = vec![
        tx[0].deref().clone(),
        tx[1].deref().clone(),
        tx[2].deref().clone(),
        tx[6].deref().clone(),
        tx[7].deref().clone(),
    ];
    // "Mine" block 2
    generate_block(&store, &mut blocks, block2_txns, &consensus_manager).unwrap();
    outputs.push(utxos);
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();
    // 2-blocks, 2 unconfirmed txs in mempool
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 2);
    // assert_eq!(stats.timelocked_txs, 0);
    assert_eq!(stats.reorg_txs, 5);
    // Create transactions wih time-locked inputs
    let txs = vec![
        txn_schema!(from: vec![outputs[2][6].clone()], to: vec![], fee: 80*uT, lock: 0, features: OutputFeatures::default()),
        // account for change output
        txn_schema!(from: vec![outputs[2][8].clone()], to: vec![], fee: 40*uT, lock: 0, features: OutputFeatures::default()),
    ];
    let (tx2, _) = schema_to_transaction(&txs);
    for t in &tx2 {
        mempool.insert(t.clone()).await.unwrap();
    }
    // 2 blocks, 3 unconfirmed txs in mempool, 2 time locked

    // Top 2 txs are tx[3] (fee/g = 50) and tx2[1] (fee/g = 40). tx2[0] (fee/g = 80) is still not matured.
    let weight = tx[3].calculate_weight(weighting) + tx2[1].calculate_weight(weighting);
    let retrieved_txs = mempool.retrieve(weight).await.unwrap();
    let stats = mempool.stats().await.unwrap();

    assert_eq!(stats.unconfirmed_txs, 3);
    // assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.reorg_txs, 5);
    assert_eq!(retrieved_txs.len(), 2);
    assert!(retrieved_txs.contains(&tx[3]));
    assert!(retrieved_txs.contains(&tx2[1]));
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_zero_conf() {
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    let mempool_validator = TxInputAndMaturityValidator::new(store.clone());
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
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();
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
    let (tx01, tx01_out) = spend_utxos(txn_schema!(
        from: vec![outputs[1][0].clone()],
        to: vec![15 * T, 5 * T],
        fee: 10*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx02, tx02_out) = spend_utxos(txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![5 * T, 5 * T],
        fee: 20*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx03, tx03_out) = spend_utxos(txn_schema!(
        from: vec![outputs[1][2].clone()],
        to: vec![5 * T, 5 * T],
        fee: 30*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx04, tx04_out) = spend_utxos(txn_schema!(
        from: vec![outputs[1][3].clone()],
        to: vec![5 * T, 5 * T],
        fee: 40*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
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
    let (tx11, tx11_out) = spend_utxos(txn_schema!(
        from: vec![tx01_out[0].clone()],
        to: vec![7 * T, 4 * T],
        fee: 50*uT, lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx12, tx12_out) = spend_utxos(txn_schema!(
        from: vec![tx01_out[1].clone(), tx02_out[0].clone(), tx02_out[1].clone()],
        to: vec![7 * T, 4 * T],
        fee: 60*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx13, tx13_out) = spend_utxos(txn_schema!(
        from: tx03_out,
        to: vec![4 * T, 4 * T],
        fee: 70*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx14, tx14_out) = spend_utxos(txn_schema!(
        from: tx04_out,
        to: vec![10 * T, 4 * T],
        fee: 80*uT, lock: 0,
        features: OutputFeatures::default()
    ));
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
    let (tx21, tx21_out) = spend_utxos(txn_schema!(
        from: vec![tx11_out[0].clone()],
        to: vec![3 * T, 3 * T],
        fee: 90*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx22, tx22_out) = spend_utxos(txn_schema!(
        from: vec![tx12_out[0].clone()],
        to: vec![3 * T, 3 * T],
        fee: 100*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx23, tx23_out) = spend_utxos(txn_schema!(
        from: vec![tx12_out[1].clone(), tx13_out[0].clone(), tx13_out[1].clone()],
        to: vec![3 * T, 3 * T],
        fee: 110*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx24, tx24_out) = spend_utxos(txn_schema!(
        from: vec![tx14_out[0].clone()],
        to: vec![3 * T, 3 * T],
        fee: 120*uT, lock: 0,
        features: OutputFeatures::default()
    ));
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
    let (tx31, _) = spend_utxos(txn_schema!(
        from: tx21_out,
        to: vec![2 * T, 2 * T],
        fee: 130*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx32, _) = spend_utxos(txn_schema!(
        from: vec![tx11_out[1].clone(), tx22_out[0].clone(), tx22_out[1].clone()],
        to: vec![2 * T, 2 * T],
        fee: 140*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx33, _) = spend_utxos(txn_schema!(
        from: vec![tx14_out[1].clone(), tx23_out[0].clone(), tx23_out[1].clone()],
        to: vec![2 * T, 2 * T],
        fee: 150*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
    let (tx34, _) = spend_utxos(txn_schema!(
        from: tx24_out,
        to: vec![2 * T, 2 * T],
        fee: 160*uT,
        lock: 0,
        features: OutputFeatures::default()
    ));
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
        .retrieve(mempool.stats().await.unwrap().total_weight)
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
        .retrieve(mempool.stats().await.unwrap().total_weight)
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
    let weight = mempool.stats().await.unwrap().total_weight - 1;
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
    assert!(!retrieved_txs.contains(&Arc::new(tx31))); // Missing
    assert!(retrieved_txs.contains(&Arc::new(tx32)));
    assert!(retrieved_txs.contains(&Arc::new(tx33)));
    assert!(retrieved_txs.contains(&Arc::new(tx34)));
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn test_reorg() {
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    let mempool_validator = TxInputAndMaturityValidator::new(db.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );

    // "Mine" Block 1
    let txs = vec![
        txn_schema!(from: vec![outputs[0][0].clone()], to: vec![1 * T, 1 * T], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
    ];
    generate_new_block(&mut db, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();
    mempool.process_published_block(blocks[1].to_arc_block()).await.unwrap();

    // "Mine" block 2
    let schemas = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
    ];
    let (txns2, utxos) = schema_to_transaction(&schemas);
    outputs.push(utxos);
    for tx in &txns2 {
        mempool.insert(tx.clone()).await.unwrap();
    }
    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 3);
    let txns2 = txns2.iter().map(|t| t.deref().clone()).collect();
    generate_block(&db, &mut blocks, txns2, &consensus_manager).unwrap();
    mempool.process_published_block(blocks[2].to_arc_block()).await.unwrap();

    // "Mine" block 3
    let schemas = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[2][1].clone()], to: vec![], fee: 25*uT, lock: 5, features: OutputFeatures::default()),
        txn_schema!(from: vec![outputs[2][2].clone()], to: vec![], fee: 25*uT, lock: 0, features: OutputFeatures::default()),
    ];
    let (txns3, utxos) = schema_to_transaction(&schemas);
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
    )
    .unwrap();
    mempool.process_published_block(blocks[3].to_arc_block()).await.unwrap();

    let stats = mempool.stats().await.unwrap();
    assert_eq!(stats.unconfirmed_txs, 0);
    // assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.reorg_txs, 5);

    db.rewind_to_height(2).unwrap();

    let template = chain_block(blocks[2].block(), vec![], &consensus_manager);
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
    let template = chain_block(blocks[2].block(), vec![], &consensus_manager);
    let reorg_block4 = db.prepare_new_block(template).unwrap();

    // test that process_reorg can handle the case when removed_blocks is empty
    // see https://github.com/tari-project/tari/issues/2101#issuecomment-680726940
    mempool.process_reorg(vec![], vec![reorg_block4.into()]).await.unwrap();
}

static EMISSION: [u64; 2] = [10, 10];
#[tokio::test]
#[allow(clippy::identity_op)]
async fn receive_and_propagate_transaction() {
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_coinbase_lockheight(100)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, utxo) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0)
        .build();
    let (mut alice_node, mut bob_node, mut carol_node, _consensus_manager) =
        create_network_with_3_base_nodes_with_config(
            BaseNodeServiceConfig::default(),
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

    let (tx, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let (orphan, _, _) = tx!(1*T, fee: 100*uT);
    let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
    let orphan_excess_sig = orphan.body.kernels()[0].excess_sig.clone();
    assert!(alice_node.mempool.insert(Arc::new(tx.clone())).await.is_ok());
    assert!(alice_node.mempool.insert(Arc::new(orphan.clone())).await.is_ok());

    alice_node
        .outbound_message_service
        .send_direct(
            bob_node.node_identity.public_key().clone(),
            OutboundDomainMessage::new(
                TariMessageType::NewTransaction,
                proto::types::Transaction::try_from(tx).unwrap(),
            ),
        )
        .await
        .unwrap();
    alice_node
        .outbound_message_service
        .send_direct(
            carol_node.node_identity.public_key().clone(),
            OutboundDomainMessage::new(
                TariMessageType::NewTransaction,
                proto::types::Transaction::try_from(orphan).unwrap(),
            ),
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
async fn consensus_validation_large_tx() {
    let network = Network::LocalNet;
    // We dont want to compute the 19500 limit of local net, so we create smaller blocks
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .with_coinbase_lockheight(1)
        .with_max_block_transaction_weight(500)
        .build();
    let (mut store, mut blocks, mut outputs, consensus_manager) =
        create_new_blockchain_with_constants(network, consensus_constants);
    let mempool_validator = TxConsensusValidator::new(store.clone());
    let mempool = Mempool::new(
        MempoolConfig::default(),
        consensus_manager.clone(),
        Box::new(mempool_validator),
    );
    // Create a block with 1 output
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![5 * T])];
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();

    // build huge tx manually - the TransactionBuilder already has checks for max inputs/outputs
    let factories = CryptoFactories::default();
    let fee_per_gram = 15;
    let input_count = 1;
    let output_count = 39;
    let amount = MicroTari::from(5_000_000);

    let input = outputs[1][0].clone();
    let sum_inputs_blinding_factors = input.spending_key.clone();
    let mut script_offset_pvt = outputs[1][0].script_private_key.clone();
    let inputs = vec![input.as_transaction_input(&factories.commitment).unwrap()];

    let fee = Fee::new(*consensus_manager.consensus_constants(0).transaction_weight()).calculate(
        fee_per_gram.into(),
        1,
        input_count,
        output_count,
        0,
    );
    let amount_per_output = (amount - fee) / output_count as u64;
    let amount_for_last_output = (amount - fee) - amount_per_output * (output_count as u64 - 1);
    let mut unblinded_outputs = Vec::with_capacity(output_count);
    let mut nonce = PrivateKey::default();
    let mut offset = PrivateKey::default();
    for i in 0..output_count {
        let test_params = TestParams::new();
        nonce = nonce + test_params.nonce.clone();
        offset = offset + test_params.offset.clone();
        let output_amount = if i < output_count - 1 {
            amount_per_output
        } else {
            amount_for_last_output
        };
        let output = create_unblinded_output(
            script!(Nop),
            OutputFeatures::default(),
            test_params.clone(),
            output_amount,
        );

        script_offset_pvt = script_offset_pvt - test_params.sender_offset_private_key;
        unblinded_outputs.push(output.clone());
    }

    let mut sum_outputs_blinding_factors = unblinded_outputs[0].spending_key.clone();
    for uo in unblinded_outputs.iter().skip(1) {
        sum_outputs_blinding_factors = sum_outputs_blinding_factors + uo.spending_key.clone();
    }
    let excess_blinding_factor = sum_outputs_blinding_factors - sum_inputs_blinding_factors;

    let outputs = unblinded_outputs
        .iter()
        .map(|o| o.as_transaction_output(&factories))
        .collect::<Result<Vec<TransactionOutput>, _>>()
        .unwrap();

    let tx_meta = TransactionMetadata { fee, lock_height: 0 };

    let public_nonce = PublicKey::from_secret_key(&nonce);
    let offset_blinding_factor = &excess_blinding_factor - &offset;
    let excess = PublicKey::from_secret_key(&offset_blinding_factor);
    let e = build_challenge(&public_nonce, &tx_meta);
    let k = offset_blinding_factor;
    let r = nonce;
    let s = Signature::sign(k, r, &e).unwrap();

    let kernel = KernelBuilder::new()
        .with_fee(fee)
        .with_lock_height(0)
        .with_excess(&Commitment::from_public_key(&excess))
        .with_signature(&s)
        .build()
        .unwrap();
    let kernels = vec![kernel];
    let tx = Transaction::new(inputs, outputs, kernels, offset, script_offset_pvt);

    let height = blocks.len() as u64;
    let constants = consensus_manager.consensus_constants(height);

    // make sure the tx was correctly made and is valid
    let factories = CryptoFactories::default();
    assert!(tx
        .validate_internal_consistency(true, &factories, None, None, u64::MAX)
        .is_ok());
    let weighting = constants.transaction_weight();
    let weight = tx.calculate_weight(weighting);

    // check the tx weight is more than the max for 1 block
    assert!(weight > constants.get_max_block_transaction_weight());

    let response = mempool.insert(Arc::new(tx)).await.unwrap();
    // make sure the tx was not accepted into the mempool
    assert!(matches!(response, TxStorageResponse::NotStored));
}

#[tokio::test]
#[allow(clippy::erasing_op)]
#[allow(clippy::identity_op)]
async fn consensus_validation_unique_id() {
    let mut rng = rand::thread_rng();
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);

    let mempool_validator = TxConsensusValidator::new(store.clone());

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
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();

    // mint new NFT
    let (_, asset) = PublicKey::random_keypair(&mut rng);
    let features = OutputFeatures {
        flags: OutputFlags::MINT_NON_FUNGIBLE,
        parent_public_key: Some(asset.clone()),
        unique_id: Some(vec![1, 2, 3]),
        ..Default::default()
    };
    let txs = vec![txn_schema!(
        from: vec![outputs[1][0].clone()],
        to: vec![1 * T], fee: 100.into(), lock: 0, features: features
    )];
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs, &consensus_manager).unwrap();

    // trying to publish a transaction with the same unique id should fail
    let tx = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![0 * T], fee: 100.into(), lock: 0, features: features
    );
    let (tx, _) = spend_utxos(tx);
    let tx = Arc::new(tx);
    let response = mempool.insert(tx).await.unwrap();
    assert!(matches!(response, TxStorageResponse::NotStoredConsensus));

    // publishing a transaction that spends a unique id to a new output should succeed
    let nft = outputs[2][0].clone();
    let features = nft.features.clone();
    let tx = txn_schema!(
        from: vec![nft],
        to: vec![0 * T], fee: 100.into(), lock: 0, features: features
    );
    let (tx, _) = spend_utxos(tx);
    let tx = Arc::new(tx);
    let response = mempool.insert(tx).await.unwrap();
    assert!(matches!(response, TxStorageResponse::UnconfirmedPool));

    // a different unique_id should be fine
    let features = OutputFeatures {
        flags: OutputFlags::MINT_NON_FUNGIBLE,
        parent_public_key: Some(asset),
        unique_id: Some(vec![4, 5, 6]),
        ..Default::default()
    };
    let tx = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![0 * T], fee: 100.into(), lock: 0, features: features
    );
    let (tx, _) = spend_utxos(tx);
    let tx = Arc::new(tx);
    let response = mempool.insert(tx).await.unwrap();
    assert!(matches!(response, TxStorageResponse::UnconfirmedPool));

    // a different asset should also be fine
    let (_, asset) = PublicKey::random_keypair(&mut rng);
    let features = OutputFeatures {
        flags: OutputFlags::MINT_NON_FUNGIBLE,
        parent_public_key: Some(asset),
        unique_id: Some(vec![4, 5, 6]),
        ..Default::default()
    };
    let tx = txn_schema!(
        from: vec![outputs[1][2].clone()],
        to: vec![0 * T], fee: 100.into(), lock: 0, features: features
    );
    let (tx, _) = spend_utxos(tx);
    let tx = Arc::new(tx);
    let response = mempool.insert(tx).await.unwrap();
    assert!(matches!(response, TxStorageResponse::UnconfirmedPool));

    // a transaction containing duplicates should be rejected
    let (_, asset) = PublicKey::random_keypair(&mut rng);
    let features = OutputFeatures {
        flags: OutputFlags::MINT_NON_FUNGIBLE,
        parent_public_key: Some(asset),
        unique_id: Some(vec![7, 8, 9]),
        ..Default::default()
    };
    let tx = txn_schema!(
        from: vec![outputs[1][3].clone(), outputs[1][4].clone()],
        to: vec![0 * T, 0 * T], fee: 100.into(), lock: 0, features: features
    );
    let (tx, _) = spend_utxos(tx);
    let tx = Arc::new(tx);
    let response = mempool.insert(tx).await.unwrap();
    dbg!(&response);
    assert!(matches!(response, TxStorageResponse::NotStoredConsensus));
}

#[tokio::test]
#[allow(clippy::identity_op)]
async fn block_event_and_reorg_event_handling() {
    // This test creates 2 nodes Alice and Bob
    // Then creates 2 chains B1 -> B2A (diff 1) and B1 -> B2B (diff 10)
    // There are 5 transactions created
    // TX1 the base transaction and then TX2A and TX3A that spend it
    // Double spends TX2B and TX3B are also created spending TX1
    // Both nodes have all transactions in their mempools
    // When block B2A is submitted, then both nodes have TX2A and TX3A in their reorg pools
    // When block B2B is submitted with TX2B, TX3B, then TX2A, TX3A are discarded (Not Stored)
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_constants = NetworkConsensus::from(network).create_consensus_constants();

    let temp_dir = tempdir().unwrap();
    let (block0, utxos0) =
        create_genesis_block_with_coinbase_value(&factories, 100_000_000.into(), &consensus_constants[0]);
    let consensus_manager = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants[0].clone())
        .with_block(block0.clone())
        .build();
    let (mut alice, mut bob, consensus_manager) = create_network_with_2_base_nodes_with_config(
        BaseNodeServiceConfig::default(),
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
    let (tx1, utxos1) = schema_to_transaction(&[txn_schema!(from: vec![utxos0], to: vec![1 * T, 1 * T])]);
    let (txs_a, _utxos2) = schema_to_transaction(&[
        txn_schema!(from: vec![utxos1[0].clone()], to: vec![400_000 * uT, 590_000 * uT]),
        txn_schema!(from: vec![utxos1[1].clone()], to: vec![750_000 * uT, 240_000 * uT]),
    ]);
    let (txs_b, _utxos3) = schema_to_transaction(&[
        txn_schema!(from: vec![utxos1[0].clone()], to: vec![100_000 * uT, 890_000 * uT]),
        txn_schema!(from: vec![utxos1[1].clone()], to: vec![850_000 * uT, 140_000 * uT]),
    ]);
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
        .prepare_new_block(chain_block(block0.block(), vec![], &consensus_manager))
        .unwrap();

    // Add one empty block, so the coinbase UTXO is no longer time-locked.
    assert!(bob.local_nci.submit_block(empty_block.clone(),).await.is_ok());
    assert!(alice.local_nci.submit_block(empty_block.clone(),).await.is_ok());
    alice.mempool.insert(Arc::new(tx1.clone())).await.unwrap();
    bob.mempool.insert(Arc::new(tx1.clone())).await.unwrap();
    let mut block1 = bob
        .blockchain_db
        .prepare_new_block(chain_block(&empty_block, vec![tx1], &consensus_manager))
        .unwrap();
    find_header_with_achieved_difficulty(&mut block1.header, Difficulty::from(1));
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
        .prepare_new_block(chain_block(&block1, vec![tx2a, tx3a], &consensus_manager))
        .unwrap();
    find_header_with_achieved_difficulty(&mut block2a.header, Difficulty::from(1));
    // Block2b also builds on Block1 but has a stronger PoW
    let mut block2b = bob
        .blockchain_db
        .prepare_new_block(chain_block(&block1, vec![tx2b, tx3b], &consensus_manager))
        .unwrap();
    find_header_with_achieved_difficulty(&mut block2b.header, Difficulty::from(10));

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
