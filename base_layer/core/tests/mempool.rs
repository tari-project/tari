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

use helpers::{
    block_builders::{create_genesis_block, generate_block, generate_new_block},
    nodes::{
        create_network_with_2_base_nodes,
        create_network_with_2_base_nodes_with_config,
        create_network_with_3_base_nodes,
    },
    sample_blockchains::create_new_blockchain,
};
use std::{ops::Deref, sync::Arc, time::Duration};
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundEncryption};
use tari_core::{
    base_node::service::BaseNodeServiceConfig,
    helpers::create_mem_db,
    mempool::{
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceError,
        MempoolValidators,
        TxStorageResponse,
    },
    transactions::{
        helpers::{schema_to_transaction, spend_utxos},
        proto,
        tari_amount::{uT, T},
        transaction::{OutputFeatures, Transaction},
        types::CryptoFactories,
    },
    tx,
    txn_schema,
    validation::transaction_validators::TxInputAndMaturityValidator,
};
use tari_mmr::MerkleChangeTrackerConfig;
use tari_p2p::tari_message::TariMessageType;
use tari_test_utils::{async_assert_eventually, random::string};
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn test_insert_and_process_published_block() {
    let (mut store, mut blocks, mut outputs) = create_new_blockchain();
    let mempool_validator = MempoolValidators::new(
        TxInputAndMaturityValidator::new(store.clone()),
        TxInputAndMaturityValidator::new(store.clone()),
    );
    let mempool = Mempool::new(store.clone(), MempoolConfig::default(), mempool_validator);
    // Create a block with 4 outputs
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![2 * T, 2 * T, 2 * T, 2 * T]
    )];
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs).unwrap();
    // Create 6 new transactions to add to the mempool
    let (orphan, _, _) = tx!(1*T, fee: 100*uT);
    let orphan = Arc::new(orphan);

    let tx2 = txn_schema!(from: vec![outputs[1][0].clone()], to: vec![1*T], fee: 20*uT);
    let tx2 = Arc::new(spend_utxos(tx2).0);

    let tx3 = txn_schema!(
        from: vec![outputs[1][1].clone()],
        to: vec![1*T],
        fee: 20*uT,
        lock: 4,
        OutputFeatures::with_maturity(1)
    );
    let tx3 = Arc::new(spend_utxos(tx3).0);

    let tx5 = txn_schema!(
        from: vec![outputs[1][2].clone()],
        to: vec![1*T],
        fee: 20*uT,
        lock: 3,
        OutputFeatures::with_maturity(2)
    );
    let tx5 = Arc::new(spend_utxos(tx5).0);
    let tx6 = txn_schema!(from: vec![outputs[1][3].clone()], to: vec![1 * T]);
    let tx6 = spend_utxos(tx6).0;

    mempool.insert(orphan.clone()).unwrap();
    mempool.insert(tx2.clone()).unwrap();
    mempool.insert(tx3.clone()).unwrap();
    mempool.insert(tx5.clone()).unwrap();
    mempool.process_published_block(&blocks[1]).unwrap();

    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&orphan.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::OrphanPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::PendingPool
    );

    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::PendingPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::NotStored
    );

    let snapshot_txs = mempool.snapshot().unwrap();
    assert_eq!(snapshot_txs.len(), 4);
    assert!(snapshot_txs.contains(&orphan));
    assert!(snapshot_txs.contains(&tx2));
    assert!(snapshot_txs.contains(&tx3));
    assert!(snapshot_txs.contains(&tx5));

    let stats = mempool.stats().unwrap();
    assert_eq!(stats.total_txs, 4);
    assert_eq!(stats.unconfirmed_txs, 1);
    assert_eq!(stats.orphan_txs, 1);
    assert_eq!(stats.timelocked_txs, 2);
    assert_eq!(stats.published_txs, 0);
    assert_eq!(stats.total_weight, 36);

    // Spend tx2, so it goes in Reorg pool, tx5 matures, so goes in Unconfirmed pool
    generate_block(&mut store, &mut blocks, vec![tx2.deref().clone()]).unwrap();
    mempool.process_published_block(&blocks[2]).unwrap();

    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&orphan.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::OrphanPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx2.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::ReorgPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx3.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::PendingPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx5.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        mempool
            .has_tx_with_excess_sig(&tx6.body.kernels()[0].excess_sig)
            .unwrap(),
        TxStorageResponse::NotStored
    );

    let snapshot_txs = mempool.snapshot().unwrap();
    assert_eq!(snapshot_txs.len(), 3);
    assert!(snapshot_txs.contains(&orphan));
    assert!(snapshot_txs.contains(&tx3));
    assert!(snapshot_txs.contains(&tx5));

    let stats = mempool.stats().unwrap();
    assert_eq!(stats.total_txs, 4);
    assert_eq!(stats.unconfirmed_txs, 1);
    assert_eq!(stats.orphan_txs, 1);
    assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.published_txs, 1);
    assert_eq!(stats.total_weight, 36);
}

#[test]
fn test_retrieve() {
    let (mut store, mut blocks, mut outputs) = create_new_blockchain();
    let mempool_validator = MempoolValidators::new(
        TxInputAndMaturityValidator::new(store.clone()),
        TxInputAndMaturityValidator::new(store.clone()),
    );
    let mempool = Mempool::new(store.clone(), MempoolConfig::default(), mempool_validator);
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![1 * T, 1 * T, 1 * T, 1 * T, 1 * T, 1 * T, 1 * T]
    )];
    // "Mine" Block 1
    generate_new_block(&mut store, &mut blocks, &mut outputs, txs).unwrap();
    mempool.process_published_block(&blocks[1]).unwrap();
    // 1-Block, 8 UTXOs, empty mempool
    let txs = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![], fee: 30*uT),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![], fee: 20*uT),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 40*uT),
        txn_schema!(from: vec![outputs[1][3].clone()], to: vec![], fee: 50*uT),
        txn_schema!(from: vec![outputs[1][4].clone()], to: vec![], fee: 20*uT, lock: 2, OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][5].clone()], to: vec![], fee: 20*uT, lock: 3, OutputFeatures::default()),
        // Will be time locked when a tx is added to mempool with this as an input:
        txn_schema!(from: vec![outputs[1][6].clone()], to: vec![800_000*uT], fee: 60*uT, lock: 0,
                        OutputFeatures::with_maturity(4)),
        // Will be time locked when a tx is added to mempool with this as an input:
        txn_schema!(from: vec![outputs[1][7].clone()], to: vec![800_000*uT], fee: 25*uT, lock: 0,
                        OutputFeatures::with_maturity(3)),
    ];
    let (tx, utxos) = schema_to_transaction(&txs);
    tx.iter().for_each(|t| {
        mempool.insert(t.clone()).unwrap();
    });
    // 1-block, 8 UTXOs, 8 txs in mempool
    let weight = tx[6].calculate_weight() + tx[2].calculate_weight() + tx[3].calculate_weight();
    let retrieved_txs = mempool.retrieve(weight).unwrap();
    assert_eq!(retrieved_txs.len(), 3);
    assert!(retrieved_txs.contains(&tx[6]));
    assert!(retrieved_txs.contains(&tx[2]));
    assert!(retrieved_txs.contains(&tx[3]));
    let stats = mempool.stats().unwrap();
    println!("After block 1: {:?}", stats);
    assert_eq!(stats.unconfirmed_txs, 7);
    assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.published_txs, 0);

    let block2_txns = vec![
        tx[0].deref().clone(),
        tx[1].deref().clone(),
        tx[2].deref().clone(),
        tx[6].deref().clone(),
        tx[7].deref().clone(),
    ];
    // "Mine" block 2
    generate_block(&mut store, &mut blocks, block2_txns).unwrap();
    println!("{}", blocks[2]);
    outputs.push(utxos);
    mempool.process_published_block(&blocks[2]).unwrap();
    // 2-blocks, 2 unconfirmed txs in mempool, 0 time locked (tx5 time-lock will expire)
    let stats = mempool.stats().unwrap();
    assert_eq!(stats.unconfirmed_txs, 3);
    assert_eq!(stats.timelocked_txs, 0);
    assert_eq!(stats.published_txs, 5);
    // Create transactions wih time-locked inputs
    let txs = vec![
        txn_schema!(from: vec![outputs[2][6].clone()], to: vec![], fee: 80*uT),
        // account for change output
        txn_schema!(from: vec![outputs[2][8].clone()], to: vec![], fee: 40*uT),
    ];
    let (tx2, _) = schema_to_transaction(&txs);
    tx2.iter().for_each(|t| {
        mempool.insert(t.clone()).unwrap();
    });
    // 2 blocks, 3 unconfirmed txs in mempool, 2 time locked

    // Top 2 txs are tx[3] (fee/g = 50) and tx2[1] (fee/g = 40). tx2[0] (fee/g = 80) is still not matured.
    let weight = tx[3].calculate_weight() + tx2[1].calculate_weight();
    let retrieved_txs = mempool.retrieve(weight).unwrap();
    let stats = mempool.stats().unwrap();
    println!("{:?}", stats);
    assert_eq!(stats.unconfirmed_txs, 4);
    assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.published_txs, 5);
    assert_eq!(retrieved_txs.len(), 2);
    assert!(retrieved_txs.contains(&tx[3]));
    assert!(retrieved_txs.contains(&tx2[1]));
}

#[test]
fn test_reorg() {
    let (mut db, mut blocks, mut outputs) = create_new_blockchain();
    let mempool_validator = MempoolValidators::new(
        TxInputAndMaturityValidator::new(db.clone()),
        TxInputAndMaturityValidator::new(db.clone()),
    );
    let mempool = Mempool::new(db.clone(), MempoolConfig::default(), mempool_validator);

    // "Mine" Block 1
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![1 * T, 1 * T])];
    generate_new_block(&mut db, &mut blocks, &mut outputs, txs).unwrap();
    mempool.process_published_block(&blocks[1]).unwrap();

    // "Mine" block 2
    let schemas = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![]),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![]),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![]),
    ];
    let (txns2, utxos) = schema_to_transaction(&schemas);
    outputs.push(utxos);
    txns2.iter().for_each(|tx| {
        mempool.insert(tx.clone()).unwrap();
    });
    let stats = mempool.stats().unwrap();
    assert_eq!(stats.unconfirmed_txs, 3);
    let txns2 = txns2.iter().map(|t| t.deref().clone()).collect();
    generate_block(&mut db, &mut blocks, txns2).unwrap();
    mempool.process_published_block(&blocks[2]).unwrap();

    // "Mine" block 3
    let schemas = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![]),
        txn_schema!(from: vec![outputs[2][1].clone()], to: vec![], fee: 25*uT, lock: 5, OutputFeatures::default()),
        txn_schema!(from: vec![outputs[2][2].clone()], to: vec![], fee: 25*uT),
    ];
    let (txns3, utxos) = schema_to_transaction(&schemas);
    outputs.push(utxos);
    txns3.iter().for_each(|tx| {
        mempool.insert(tx.clone()).unwrap();
    });
    let txns3: Vec<Transaction> = txns3.iter().map(|t| t.deref().clone()).collect();

    generate_block(&mut db, &mut blocks, vec![txns3[0].clone(), txns3[2].clone()]).unwrap();
    mempool.process_published_block(&blocks[3]).unwrap();

    let stats = mempool.stats().unwrap();
    assert_eq!(stats.unconfirmed_txs, 0);
    assert_eq!(stats.timelocked_txs, 1);

    db.rewind_to_height(2).unwrap();

    mempool.process_reorg(vec![blocks[3].clone()], vec![]).unwrap();
    let stats = mempool.stats().unwrap();
    assert_eq!(stats.unconfirmed_txs, 2);
    assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.published_txs, 3);
}

#[test]
fn test_orphaned_mempool_transactions() {
    let (store, mut blocks, mut outputs) = create_new_blockchain();
    // A parallel store that will "mine" the orphan chain
    let mut miner = create_mem_db();
    miner.add_block(blocks[0].clone()).unwrap();
    let schemas = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![2 * T, 2 * T, 2 * T, 2 * T, 2 * T]
    )];
    generate_new_block(&mut miner, &mut blocks, &mut outputs, schemas.clone()).unwrap();
    store.add_block(blocks[1].clone()).unwrap();
    let schemas = vec![
        txn_schema!(from: vec![outputs[1][0].clone(), outputs[1][1].clone()], to: vec![], fee: 500*uT, lock: 1100, OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][2].clone()], to: vec![], fee: 300*uT, lock: 1700, OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][3].clone()], to: vec![], fee: 100*uT),
    ];
    let (txns, _) = schema_to_transaction(&schemas.clone());
    generate_new_block(&mut miner, &mut blocks, &mut outputs, schemas).unwrap();
    // tx3 and tx4 depend on tx0 and tx1
    let schemas = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![], fee: 200*uT),
        txn_schema!(from: vec![outputs[2][2].clone()], to: vec![], fee: 500*uT, lock: 1000, OutputFeatures::default()),
        txn_schema!(from: vec![outputs[1][4].clone()], to: vec![], fee: 600*uT, lock: 5200, OutputFeatures::default()),
    ];
    let (txns2, _) = schema_to_transaction(&schemas.clone());
    generate_new_block(&mut miner, &mut blocks, &mut outputs, schemas).unwrap();
    let mempool_validator = MempoolValidators::new(
        TxInputAndMaturityValidator::new(store.clone()),
        TxInputAndMaturityValidator::new(store.clone()),
    );
    let mempool = Mempool::new(store.clone(), MempoolConfig::default(), mempool_validator);
    // There are 2 orphan txs
    vec![txns[2].clone(), txns2[0].clone(), txns2[1].clone(), txns2[2].clone()]
        .into_iter()
        .for_each(|t| mempool.insert(t).unwrap());

    let stats = mempool.stats().unwrap();
    assert_eq!(stats.total_txs, 4);
    assert_eq!(stats.unconfirmed_txs, 1);
    assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.orphan_txs, 2);
    store.add_block(blocks[1].clone()).unwrap();
    store.add_block(blocks[2].clone()).unwrap();
    mempool.process_published_block(&blocks[1]).unwrap();
    mempool.process_published_block(&blocks[2]).unwrap();
    let stats = mempool.stats().unwrap();
    assert_eq!(stats.total_txs, 3);
    assert_eq!(stats.unconfirmed_txs, 1);
    assert_eq!(stats.orphan_txs, 0);
}

#[test]
fn request_response_get_stats() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice, bob) = create_network_with_2_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());

    // Create a Genesis block, and a tx spending the genesis output. Then create 2 orphan txs
    let (block0, utxo) = create_genesis_block(&bob.blockchain_db, &factories);

    let (tx1, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let tx1 = Arc::new(tx1);
    let (orphan1, _, _) = tx!(1*T, fee: 100*uT);
    let orphan1 = Arc::new(orphan1);
    let (orphan2, _, _) = tx!(2*T, fee: 200*uT);
    let orphan2 = Arc::new(orphan2);

    bob.blockchain_db.add_block(block0.clone()).unwrap();
    bob.mempool.insert(tx1.clone()).unwrap();
    bob.mempool.insert(orphan1.clone()).unwrap();
    bob.mempool.insert(orphan2.clone()).unwrap();

    // The coinbase tx cannot be spent until maturity, so rxn1 will be in the timelocked pool. The other 2 txns are
    // orphans.
    let stats = bob.mempool.stats().unwrap();
    assert_eq!(stats.total_txs, 3);
    assert_eq!(stats.orphan_txs, 2);
    assert_eq!(stats.unconfirmed_txs, 0);
    assert_eq!(stats.timelocked_txs, 1);
    assert_eq!(stats.published_txs, 0);
    assert_eq!(stats.total_weight, 35);

    runtime.block_on(async {
        // Alice will request mempool stats from Bob, and thus should be identical
        let received_stats = alice.outbound_mp_interface.get_stats().await.unwrap();
        assert_eq!(received_stats.total_txs, 3);
        assert_eq!(received_stats.unconfirmed_txs, 0);
        assert_eq!(received_stats.orphan_txs, 2);
        assert_eq!(received_stats.timelocked_txs, 1);
        assert_eq!(received_stats.published_txs, 0);
        assert_eq!(received_stats.total_weight, 35);
    });

    alice.comms.shutdown().unwrap();
    bob.comms.shutdown().unwrap();
}

#[test]
fn request_response_get_tx_state_with_excess_sig() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());

    let (block0, utxo) = create_genesis_block(&bob_node.blockchain_db, &factories);
    bob_node.blockchain_db.add_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_block(block0.clone()).unwrap();
    let (tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo.clone()], to: vec![2 * T, 2 * T, 2 * T]));
    let (unpublished_tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![3 * T]));
    let (orphan_tx, _, _) = tx!(1*T, fee: 100*uT);
    let tx = Arc::new(tx);
    let orphan_tx = Arc::new(orphan_tx);
    bob_node.mempool.insert(tx.clone()).unwrap();
    carol_node.mempool.insert(tx.clone()).unwrap();
    bob_node.mempool.insert(orphan_tx.clone()).unwrap();
    carol_node.mempool.insert(orphan_tx.clone()).unwrap();

    // Check that the transactions are in the expected pools.
    // Spending the coinbase utxo will be in the pending pool, because cb utxos have a maturity.
    // The orphan tx will be in the orphan pool, while the unadded tx won't be found
    runtime.block_on(async {
        let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
        let unpublished_tx_excess_sig = unpublished_tx.body.kernels()[0].excess_sig.clone();
        let orphan_tx_excess_sig = orphan_tx.body.kernels()[0].excess_sig.clone();
        assert_eq!(
            alice_node
                .outbound_mp_interface
                .get_tx_state_with_excess_sig(tx_excess_sig)
                .await
                .unwrap(),
            TxStorageResponse::PendingPool
        );
        assert_eq!(
            alice_node
                .outbound_mp_interface
                .get_tx_state_with_excess_sig(unpublished_tx_excess_sig)
                .await
                .unwrap(),
            TxStorageResponse::NotStored
        );
        assert_eq!(
            alice_node
                .outbound_mp_interface
                .get_tx_state_with_excess_sig(orphan_tx_excess_sig)
                .await
                .unwrap(),
            TxStorageResponse::OrphanPool
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn receive_and_propagate_transaction() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());

    let (block0, utxo) = create_genesis_block(&alice_node.blockchain_db, &factories);
    alice_node.blockchain_db.add_block(block0.clone()).unwrap();
    bob_node.blockchain_db.add_block(block0.clone()).unwrap();
    carol_node.blockchain_db.add_block(block0.clone()).unwrap();
    let (tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let (orphan, _, _) = tx!(1*T, fee: 100*uT);
    let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
    let orphan_excess_sig = orphan.body.kernels()[0].excess_sig.clone();
    assert!(alice_node.mempool.insert(Arc::new(tx.clone())).is_ok());
    assert!(alice_node.mempool.insert(Arc::new(orphan.clone())).is_ok());

    runtime.block_on(async {
        alice_node
            .outbound_message_service
            .send_direct(
                bob_node.node_identity.public_key().clone(),
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, proto::types::Transaction::from(tx)),
            )
            .await
            .unwrap();
        alice_node
            .outbound_message_service
            .send_direct(
                carol_node.node_identity.public_key().clone(),
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, proto::types::Transaction::from(orphan)),
            )
            .await
            .unwrap();

        async_assert_eventually!(
            bob_node.mempool.has_tx_with_excess_sig(&tx_excess_sig).unwrap(),
            expect = TxStorageResponse::PendingPool,
            max_attempts = 20,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            bob_node.mempool.has_tx_with_excess_sig(&orphan_excess_sig).unwrap(),
            expect = TxStorageResponse::OrphanPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            carol_node.mempool.has_tx_with_excess_sig(&tx_excess_sig).unwrap(),
            expect = TxStorageResponse::PendingPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            carol_node.mempool.has_tx_with_excess_sig(&orphan_excess_sig).unwrap(),
            expect = TxStorageResponse::OrphanPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn service_request_timeout() {
    let mut runtime = Runtime::new().unwrap();

    let mempool_service_config = MempoolServiceConfig {
        request_timeout: Duration::from_millis(1),
    };
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 30,
    };
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        mempool_service_config,
        temp_dir.path().to_str().unwrap(),
    );

    runtime.block_on(async {
        match alice_node.outbound_mp_interface.get_stats().await {
            Err(MempoolServiceError::RequestTimedOut) => assert!(true),
            _ => assert!(false),
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}
