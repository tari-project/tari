// Copyright 2020. The Tari Project
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
    block_builders::create_genesis_block_with_coinbase_value,
    event_stream::event_stream_next,
    nodes::{random_node_identity, BaseNodeBuilder},
};

use core::iter;
use futures::{FutureExt, StreamExt};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    transports::MemoryTransport,
    types::CommsPublicKey,
};
use tari_comms_dht::DhtConfig;
use tari_core::{
    base_node::{service::BaseNodeServiceConfig, state_machine_service::states::StateEvent},
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    mempool::{MempoolServiceConfig, TxStorageResponse},
    mining::Miner,
    transactions::{tari_amount::MicroTari, transaction::Transaction, types::CryptoFactories},
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::{initialization::CommsConfig, services::liveness::LivenessConfig, transport::TransportType};
use tari_shutdown::Shutdown;
use tari_test_utils::async_assert_eventually;
use tari_wallet::{
    contacts_service::storage::memory_db::ContactsServiceMemoryDatabase,
    output_manager_service::storage::memory_db::OutputManagerMemoryDatabase,
    storage::memory_db::WalletMemoryDatabase,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionEvent,
        storage::memory_db::TransactionMemoryDatabase,
    },
    wallet::WalletConfig,
    Wallet,
};
use tempfile::tempdir;
use tokio::{
    runtime::{Builder, Runtime},
    sync::broadcast,
    time::delay_for,
};

pub fn random_string(len: usize) -> String {
    iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
}

pub fn get_next_memory_address() -> Multiaddr {
    let port = MemoryTransport::acquire_next_memsocket_port();
    format!("/memory/{}", port).parse().unwrap()
}

fn create_runtime() -> Runtime {
    Builder::new()
        .threaded_scheduler()
        .enable_all()
        .core_threads(8)
        .build()
        .unwrap()
}

fn create_peer(public_key: CommsPublicKey, net_address: Multiaddr) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        net_address.into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
        &[],
        Default::default(),
    )
}
static EMISSION: [u64; 2] = [10, 10];
#[test]
fn wallet_base_node_integration_test() {
    let shutdown = Shutdown::new();
    let alice_temp_dir = tempdir().unwrap();
    let bob_temp_dir = tempdir().unwrap();
    let factories = CryptoFactories::default();

    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let base_node_identity = random_node_identity();

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    // Base Node Setup
    let mut base_node_runtime = create_runtime();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, utxo0) =
        create_genesis_block_with_coinbase_value(&factories, 100_000_000.into(), &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (base_node, _consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(base_node_identity.clone())
        .with_base_node_service_config(BaseNodeServiceConfig::default())
        .with_mmr_cache_config(MmrCacheConfig { rewind_hist_len: 10 })
        .with_mempool_service_config(MempoolServiceConfig::default())
        .with_liveness_service_config(LivenessConfig::default())
        .with_consensus_manager(consensus_manager.clone())
        .start(&mut base_node_runtime, alice_temp_dir.path().to_str().unwrap());

    log::info!("Finished Starting Base Node");

    // Alice Wallet setup
    let alice_comms_config = CommsConfig {
        node_identity: alice_node_identity.clone(),
        transport_type: TransportType::Memory {
            listener_address: alice_node_identity.public_address(),
        },
        datastore_path: alice_temp_dir.path().to_path_buf(),
        peer_database_name: random_string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig::default_local_test(),
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
    };
    let alice_wallet_config = WalletConfig::new(
        alice_comms_config,
        factories.clone(),
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(1),
            low_power_polling_timeout: Duration::from_secs(10),
            ..Default::default()
        }),
        Network::Rincewind,
        None,
    );
    let mut runtime = create_runtime();
    let mut alice_wallet = runtime
        .block_on(Wallet::new(
            alice_wallet_config,
            WalletMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            OutputManagerMemoryDatabase::new(),
            ContactsServiceMemoryDatabase::new(),
            shutdown.to_signal(),
        ))
        .unwrap();
    let mut alice_event_stream = alice_wallet.transaction_service.get_event_stream_fused();

    runtime
        .block_on(alice_wallet.set_base_node_peer(
            (*base_node_identity.public_key()).clone(),
            base_node_identity.public_address().clone().to_string(),
        ))
        .unwrap();

    runtime
        .block_on(alice_wallet.comms.peer_manager().add_peer(create_peer(
            bob_node_identity.public_key().clone(),
            bob_node_identity.public_address(),
        )))
        .unwrap();

    // Bob Wallet setup
    let bob_comms_config = CommsConfig {
        node_identity: bob_node_identity.clone(),
        transport_type: TransportType::Memory {
            listener_address: bob_node_identity.public_address(),
        },
        datastore_path: bob_temp_dir.path().to_path_buf(),
        peer_database_name: random_string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig::default_local_test(),
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
    };
    let bob_wallet_config = WalletConfig::new(bob_comms_config, factories.clone(), None, Network::Rincewind, None);

    let bob_wallet = runtime
        .block_on(Wallet::new(
            bob_wallet_config,
            WalletMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            OutputManagerMemoryDatabase::new(),
            ContactsServiceMemoryDatabase::new(),
            shutdown.to_signal(),
        ))
        .unwrap();

    runtime
        .block_on(bob_wallet.comms.peer_manager().add_peer(create_peer(
            alice_node_identity.public_key().clone(),
            alice_node_identity.public_address(),
        )))
        .unwrap();

    log::info!("Finished Starting Wallets");

    // Transaction
    runtime
        .block_on(alice_wallet.output_manager_service.add_output(utxo0))
        .unwrap();
    runtime
        .block_on(
            alice_wallet
                .comms
                .connectivity()
                .wait_for_connectivity(Duration::from_secs(10)),
        )
        .unwrap();

    let value = MicroTari::from(1000);
    runtime
        .block_on(alice_wallet.transaction_service.send_transaction(
            bob_node_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            "MONAAHHH!".to_string(),
        ))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut broadcast = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionBroadcast(_e) = (*event.unwrap()).clone() {
                        broadcast = true;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(broadcast, "Transaction has not been broadcast before timeout");
    });

    let transactions = runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();

    assert_eq!(transactions.len(), 1);
    let mut transaction: Option<Transaction> = None;
    for (_, tx) in transactions {
        transaction = Some(tx.transaction.clone());
        let tx_excess_sig = tx.transaction.body.kernels()[0].excess_sig.clone();
        runtime.block_on(async {
            async_assert_eventually!(
                base_node.mempool.has_tx_with_excess_sig(tx_excess_sig.clone()).unwrap(),
                expect = TxStorageResponse::UnconfirmedPool,
                max_attempts = 20,
                interval = Duration::from_millis(1000)
            );
        });
    }
    runtime
        .block_on(alice_wallet.transaction_service.set_low_power_mode())
        .unwrap();
    let transaction = transaction.expect("Transaction must be present");

    // Setup and start the miner
    let shutdown = Shutdown::new();
    let mut miner = Miner::new(shutdown.to_signal(), consensus_manager, &base_node.local_nci, 1);
    miner.enable_mining_flag().store(true, Ordering::Relaxed);
    let (state_event_sender, state_event_receiver) = broadcast::channel(1);
    miner.subscribe_to_node_state_events(state_event_receiver);
    miner.subscribe_to_mempool_state_events(base_node.local_mp_interface.get_mempool_state_event_stream());
    let mut miner_utxo_stream = miner.get_utxo_receiver_channel().fuse();
    runtime.spawn(miner.mine());

    runtime.block_on(async {
        // Simulate block sync
        state_event_sender
            .send(Arc::new(StateEvent::BlocksSynchronized))
            .unwrap();
        // Wait for miner to finish mining block 1
        assert!(event_stream_next(&mut miner_utxo_stream, Duration::from_secs(20))
            .await
            .is_some());
        // Check that the mined block was submitted to the base node service and the block was added to the blockchain
        let block1 = base_node.blockchain_db.fetch_block(1).unwrap().block().clone();
        assert_eq!(block1.body.outputs().len(), 3);

        // Check if the outputs of tx1 appeared as outputs in block1
        let mut found_tx_outputs = 0;
        for tx_output in transaction.body.outputs() {
            for block_output in block1.body.outputs() {
                if tx_output == block_output {
                    found_tx_outputs += 1;
                    break;
                }
            }
        }
        assert_eq!(found_tx_outputs, transaction.body.outputs().len());
    });

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut mined = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionMined(_e) = (*event.unwrap()).clone() {
                        mined = true;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(mined, "Transaction has not been mined before timeout");
    });
}
