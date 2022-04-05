// Copyright 2021. The Tari Project
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

use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::{Duration as ChronoDuration, Utc};
use rand::{rngs::OsRng, RngCore};
use tari_comms::{
    peer_manager::PeerFeatures,
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
    test_utils::{
        mocks::{create_connectivity_mock, ConnectivityManagerMockState},
        node_identity::build_node_identity,
    },
};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcServer,
    blocks::BlockHeader,
    proto::base_node::{ChainMetadata, TipInfoResponse},
    transactions::{tari_amount::MicroTari, transaction_components::UnblindedOutput, CryptoFactories},
};
use tari_key_manager::cipher_seed::CipherSeed;
use tari_service_framework::reply_channel;
use tari_shutdown::Shutdown;
use tari_test_utils::random;
use tari_utilities::{epoch_time::EpochTime, Hashable};
use tari_wallet::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::{create_wallet_connectivity_mock, WalletConnectivityInterface, WalletConnectivityMock},
    output_manager_service::storage::models::DbUnblindedOutput,
    storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::run_migration_and_create_sqlite_connection,
    },
    utxo_scanner_service::{
        handle::UtxoScannerEvent,
        service::{ScannedBlock, UtxoScannerService},
        uxto_scanner_service_builder::UtxoScannerMode,
    },
};
use tempfile::{tempdir, TempDir};
use tokio::{
    sync::{broadcast, mpsc},
    task,
    time,
};

pub mod support;

use support::{
    base_node_service_mock::MockBaseNodeService,
    comms_rpc::{BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState, UtxosByBlock},
    output_manager_service_mock::{make_output_manager_service_mock, OutputManagerMockState},
    transaction_service_mock::make_transaction_service_mock,
    utils::make_input,
};
use tari_comms::types::CommsPublicKey;
use tari_wallet::{
    transaction_service::handle::TransactionServiceRequest,
    util::watch::Watch,
    utxo_scanner_service::handle::UtxoScannerHandle,
};

use crate::support::transaction_service_mock::TransactionServiceMockState;

pub struct UtxoScannerTestInterface {
    scanner_service: Option<UtxoScannerService<WalletSqliteDatabase>>,
    scanner_handle: UtxoScannerHandle,
    wallet_db: WalletDatabase<WalletSqliteDatabase>,
    base_node_service_event_publisher: broadcast::Sender<Arc<BaseNodeEvent>>,
    rpc_service_state: BaseNodeWalletRpcMockState,
    _rpc_mock_server: MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    _comms_connectivity_mock_state: ConnectivityManagerMockState,
    _wallet_connectivity_mock: WalletConnectivityMock,
    transaction_service_mock_state: TransactionServiceMockState,
    oms_mock_state: OutputManagerMockState,
    shutdown_signal: Shutdown,
    _temp_dir: TempDir,
}

async fn setup(
    mode: UtxoScannerMode,
    previous_db: Option<WalletDatabase<WalletSqliteDatabase>>,
    recovery_message: Option<String>,
    one_sided_message: Option<String>,
) -> UtxoScannerTestInterface {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    // Base Node Service Mock
    let (sender, receiver_bns) = reply_channel::unbounded();
    let (event_publisher_bns, _) = broadcast::channel(100);
    let base_node_service_handle = BaseNodeServiceHandle::new(sender, event_publisher_bns.clone());
    let mut mock_base_node_service = MockBaseNodeService::new(receiver_bns, shutdown.to_signal());
    mock_base_node_service.set_default_base_node_state();
    task::spawn(mock_base_node_service.run());

    // BaseNodeRpcService Mock
    let service = BaseNodeWalletRpcMockService::new();
    let rpc_service_state = service.get_state();
    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut mock_server = MockRpcServer::new(server, server_node_identity.clone());
    mock_server.serve();

    let rpc_server_connection = mock_server
        .create_connection(server_node_identity.to_peer(), protocol_name.into())
        .await;

    let (comms_connectivity, connectivity_mock) = create_connectivity_mock();
    let comms_connectivity_mock_state = connectivity_mock.get_shared_state();
    comms_connectivity_mock_state
        .add_active_connection(rpc_server_connection)
        .await;
    task::spawn(connectivity_mock.run());

    let wallet_connectivity_mock = create_wallet_connectivity_mock();

    let (ts_mock, ts_handle) = make_transaction_service_mock(shutdown.to_signal());
    let transaction_service_mock_state = ts_mock.get_state();
    task::spawn(ts_mock.run());

    let (oms_mock, oms_handle) = make_output_manager_service_mock(shutdown.to_signal());
    let oms_mock_state = oms_mock.get_state();
    task::spawn(oms_mock.run());

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (event_sender, _) = broadcast::channel(200);

    let temp_dir = tempdir().unwrap();
    let wallet_db = match previous_db {
        None => {
            let path_string = temp_dir.path().to_str().unwrap().to_string();
            let db_name = format!("{}.sqlite3", random::string(8).as_str());
            let db_path = format!("{}/{}", path_string, db_name);
            // let db_path = "/tmp/test.sqlite3";

            let db_connection = run_migration_and_create_sqlite_connection(&db_path, 16).unwrap();

            WalletDatabase::new(
                WalletSqliteDatabase::new(db_connection, None).expect("Should be able to create wallet database"),
            )
        },
        Some(db) => db,
    };

    let recovery_message_watch = Watch::new("unset".to_string());
    let one_sided_message_watch = Watch::new("unset".to_string());

    let recovery_message_watch_receiver = recovery_message_watch.get_receiver();
    let one_sided_message_watch_receiver = one_sided_message_watch.get_receiver();

    let scanner_handle = UtxoScannerHandle::new(event_sender.clone(), one_sided_message_watch, recovery_message_watch);

    let mut scanner_service_builder = UtxoScannerService::<WalletSqliteDatabase>::builder();

    scanner_service_builder
        .with_peers(vec![server_node_identity.public_key().clone()])
        .with_retry_limit(1)
        .with_mode(mode);

    if let Some(message) = one_sided_message {
        scanner_service_builder.with_one_sided_message(message);
    }

    if let Some(message) = recovery_message {
        scanner_service_builder.with_recovery_message(message);
    }

    let scanner_service = scanner_service_builder.build_with_resources(
        wallet_db.clone(),
        comms_connectivity,
        wallet_connectivity_mock.get_current_base_node_watcher(),
        oms_handle,
        ts_handle,
        node_identity,
        factories,
        shutdown.to_signal(),
        event_sender,
        base_node_service_handle,
        one_sided_message_watch_receiver,
        recovery_message_watch_receiver,
    );

    UtxoScannerTestInterface {
        scanner_service: Some(scanner_service),
        scanner_handle,
        wallet_db,
        base_node_service_event_publisher: event_publisher_bns,
        rpc_service_state,
        _rpc_mock_server: mock_server,
        _comms_connectivity_mock_state: comms_connectivity_mock_state,
        _wallet_connectivity_mock: wallet_connectivity_mock,
        transaction_service_mock_state,
        oms_mock_state,
        shutdown_signal: shutdown,
        _temp_dir: temp_dir,
    }
}

pub struct TestBlockData {
    block_headers: HashMap<u64, BlockHeader>,
    unblinded_outputs: HashMap<u64, Vec<UnblindedOutput>>,
    utxos_by_block: Vec<UtxosByBlock>,
}

/// Generates a set of block headers and unblinded outputs for each header. The `birthday_offset` specifies at which
/// block in the `num_block` the birthday timestamp will have passed i.e. it occured during the previous block period.
/// e.g. with `num_blocks` = 10 and `birthday_offset` = 5 the birthday timestamp will occur between block 4 and 5
async fn generate_block_headers_and_utxos(
    start_height: u64,
    num_blocks: u64,
    birthday_epoch_time: u64,
    birthday_offset: u64,
    only_coinbase: bool,
) -> TestBlockData {
    let factories = CryptoFactories::default();
    let mut block_headers = HashMap::new();
    let mut utxos_by_block = Vec::new();
    let mut unblinded_outputs = HashMap::new();
    for i in start_height..num_blocks + start_height {
        let mut block_header = BlockHeader::new(0);
        block_header.height = i;
        block_header.timestamp =
            EpochTime::from((birthday_epoch_time as i64 + (i as i64 - birthday_offset as i64) * 100i64 + 5) as u64);
        block_headers.insert(i, block_header.clone());
        // Generate utxos for this block
        let mut block_outputs = Vec::new();

        for _j in 0..=i + 1 {
            let (_ti, uo) = make_input(
                &mut OsRng,
                MicroTari::from(100 + OsRng.next_u64() % 1000),
                &factories.commitment,
                None,
            )
            .await;
            block_outputs.push(uo);
            if only_coinbase {
                break;
            }
        }

        let transaction_outputs = block_outputs
            .clone()
            .iter()
            .map(|uo| uo.as_transaction_output(&factories).unwrap())
            .collect();
        let utxos = UtxosByBlock {
            height: i,
            header_hash: block_header.hash(),
            utxos: transaction_outputs,
        };
        utxos_by_block.push(utxos);
        unblinded_outputs.insert(i, block_outputs);
    }
    TestBlockData {
        block_headers,
        unblinded_outputs,
        utxos_by_block,
    }
}

#[tokio::test]
async fn test_utxo_scanner_recovery() {
    let factories = CryptoFactories::default();
    let mut test_interface = setup(UtxoScannerMode::Recovery, None, None, None).await;

    let cipher_seed = CipherSeed::new();
    let birthday_epoch_time = u64::from(cipher_seed.birthday() - 2) * 60 * 60 * 24;
    test_interface.wallet_db.set_master_seed(cipher_seed).await.unwrap();

    const NUM_BLOCKS: u64 = 11;
    const BIRTHDAY_OFFSET: u64 = 5;

    let TestBlockData {
        block_headers,
        unblinded_outputs,
        utxos_by_block,
    } = generate_block_headers_and_utxos(0, NUM_BLOCKS, birthday_epoch_time, BIRTHDAY_OFFSET, false).await;

    test_interface
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface.rpc_service_state.set_blocks(block_headers.clone());

    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(NUM_BLOCKS - 1),
        best_block: Some(block_headers.get(&(NUM_BLOCKS - 1)).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };
    test_interface.rpc_service_state.set_tip_info_response(TipInfoResponse {
        metadata: Some(chain_metadata),
        is_synced: true,
    });

    // Adding half the outputs of the blocks to the OMS mock
    let mut db_unblinded_outputs = Vec::new();
    let mut total_outputs_to_recover = 0;
    let mut total_amount_to_recover = MicroTari::from(0);
    for (h, outputs) in &unblinded_outputs {
        for output in outputs.iter().skip(outputs.len() / 2) {
            let dbo = DbUnblindedOutput::from_unblinded_output(output.clone(), &factories, None).unwrap();
            // Only the outputs in blocks after the birthday should be included in the recovered total
            if *h >= NUM_BLOCKS.saturating_sub(BIRTHDAY_OFFSET).saturating_sub(2) {
                total_outputs_to_recover += 1;
                total_amount_to_recover += dbo.unblinded_output.value;
            }
            db_unblinded_outputs.push(dbo);
        }
    }
    test_interface
        .oms_mock_state
        .set_recoverable_outputs(db_unblinded_outputs);

    let mut scanner_event_stream = test_interface.scanner_handle.get_event_receiver();

    tokio::spawn(test_interface.scanner_service.take().unwrap().run());

    let delay = time::sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => {
                panic!("Completed event should have arrived by now.");
            }
            event = scanner_event_stream.recv() => {
                if let UtxoScannerEvent::Completed {
                    final_height,
                    num_recovered,
                    value_recovered,
                    time_taken: _,} = event.unwrap() {
                    assert_eq!(final_height, NUM_BLOCKS-1);
                    assert_eq!(num_recovered, total_outputs_to_recover);
                    assert_eq!(value_recovered, total_amount_to_recover);
                    break;
                }
            }
        }
    }
}
#[tokio::test]
async fn test_utxo_scanner_recovery_with_restart() {
    let factories = CryptoFactories::default();
    let mut test_interface = setup(UtxoScannerMode::Recovery, None, None, None).await;

    let cipher_seed = CipherSeed::new();
    let birthday_epoch_time = u64::from(cipher_seed.birthday() - 2) * 60 * 60 * 24;
    test_interface.wallet_db.set_master_seed(cipher_seed).await.unwrap();

    test_interface
        .scanner_handle
        .set_one_sided_payment_message("one".to_string());
    test_interface
        .scanner_handle
        .set_recovery_message("recover".to_string());

    const NUM_BLOCKS: u64 = 11;
    const BIRTHDAY_OFFSET: u64 = 5;
    const SYNC_INTERRUPT: u64 = 6;

    let TestBlockData {
        block_headers,
        unblinded_outputs,
        utxos_by_block,
    } = generate_block_headers_and_utxos(0, NUM_BLOCKS, birthday_epoch_time, BIRTHDAY_OFFSET, false).await;

    test_interface
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface.rpc_service_state.set_blocks(block_headers.clone());

    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(NUM_BLOCKS - 1),
        best_block: Some(block_headers.get(&(NUM_BLOCKS - 1)).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };
    test_interface.rpc_service_state.set_tip_info_response(TipInfoResponse {
        metadata: Some(chain_metadata.clone()),
        is_synced: true,
    });

    // Adding half the outputs of the blocks to the OMS mock
    let mut db_unblinded_outputs = Vec::new();
    let mut total_outputs_to_recover = 0;
    let mut total_amount_to_recover = MicroTari::from(0);
    for (h, outputs) in &unblinded_outputs {
        for output in outputs.iter().skip(outputs.len() / 2) {
            let dbo = DbUnblindedOutput::from_unblinded_output(output.clone(), &factories, None).unwrap();
            // Only the outputs in blocks after the birthday should be included in the recovered total
            if *h >= NUM_BLOCKS.saturating_sub(BIRTHDAY_OFFSET).saturating_sub(2) {
                total_outputs_to_recover += 1;
                total_amount_to_recover += dbo.unblinded_output.value;
            }
            db_unblinded_outputs.push(dbo);
        }
    }
    test_interface
        .oms_mock_state
        .set_recoverable_outputs(db_unblinded_outputs.clone());

    let (tx, rx) = mpsc::channel(100);
    test_interface.rpc_service_state.set_utxos_by_block_trigger_channel(rx);

    tokio::spawn(test_interface.scanner_service.take().unwrap().run());

    tx.send(SYNC_INTERRUPT as usize).await.unwrap();

    let _result = test_interface
        .rpc_service_state
        .wait_pop_sync_utxos_by_block_calls(1, Duration::from_secs(30))
        .await
        .unwrap();

    // Confirm the recovery message and source pub key are the defaults.
    let requests = test_interface.transaction_service_mock_state.drain_requests();
    assert!(!requests.is_empty());
    for req in requests {
        if let TransactionServiceRequest::ImportUtxoWithStatus {
            amount: _,
            source_public_key,
            message,
            maturity: _,
            import_status: _,
            tx_id: _,
            current_height: _,
        } = req
        {
            assert_eq!(message, "Output found on blockchain during Wallet Recovery".to_string());
            assert_eq!(source_public_key, CommsPublicKey::default());
        }
    }

    test_interface.shutdown_signal.trigger();

    let mut test_interface2 = setup(
        UtxoScannerMode::Recovery,
        Some(test_interface.wallet_db),
        Some("recovery".to_string()),
        None,
    )
    .await;
    test_interface2
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface2.rpc_service_state.set_blocks(block_headers.clone());
    test_interface2
        .rpc_service_state
        .set_tip_info_response(TipInfoResponse {
            metadata: Some(chain_metadata),
            is_synced: true,
        });
    test_interface2
        .oms_mock_state
        .set_recoverable_outputs(db_unblinded_outputs);
    let mut scanner_event_stream = test_interface2.scanner_handle.get_event_receiver();
    tokio::spawn(test_interface2.scanner_service.take().unwrap().run());

    let delay = time::sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => {
                panic!("Completed event should have arrived by now.");
            }
            event = scanner_event_stream.recv() => {
                if let UtxoScannerEvent::Completed {
                    final_height,
                    num_recovered,
                    value_recovered,
                    time_taken: _,} = event.unwrap() {
                    assert_eq!(final_height, NUM_BLOCKS-1);
                    assert_eq!(num_recovered, total_outputs_to_recover);
                    assert_eq!(value_recovered, total_amount_to_recover);
                    break;
                }
            }
        }
    }

    // Confirm the recovery message changed using the builder method
    let requests = test_interface2.transaction_service_mock_state.drain_requests();
    assert!(!requests.is_empty());
    for req in requests {
        if let TransactionServiceRequest::ImportUtxoWithStatus {
            amount: _,
            source_public_key: _,
            message,
            maturity: _,
            import_status: _,
            tx_id: _,
            current_height: _,
        } = req
        {
            assert_eq!(message, "recovery".to_string());
        }
    }
}

#[tokio::test]
async fn test_utxo_scanner_recovery_with_restart_and_reorg() {
    let factories = CryptoFactories::default();
    let mut test_interface = setup(UtxoScannerMode::Recovery, None, None, None).await;

    let cipher_seed = CipherSeed::new();
    let birthday_epoch_time = u64::from(cipher_seed.birthday() - 2) * 60 * 60 * 24;
    test_interface.wallet_db.set_master_seed(cipher_seed).await.unwrap();

    const NUM_BLOCKS: u64 = 11;
    const BIRTHDAY_OFFSET: u64 = 5;
    const SYNC_INTERRUPT: u64 = 6;

    let TestBlockData {
        mut block_headers,
        mut unblinded_outputs,
        utxos_by_block,
    } = generate_block_headers_and_utxos(0, NUM_BLOCKS, birthday_epoch_time, BIRTHDAY_OFFSET, false).await;

    test_interface
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface.rpc_service_state.set_blocks(block_headers.clone());

    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(NUM_BLOCKS - 1),
        best_block: Some(block_headers.get(&(NUM_BLOCKS - 1)).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };
    test_interface.rpc_service_state.set_tip_info_response(TipInfoResponse {
        metadata: Some(chain_metadata.clone()),
        is_synced: true,
    });

    // Adding half the outputs of the blocks to the OMS mock
    let mut db_unblinded_outputs = Vec::new();
    for outputs in unblinded_outputs.values() {
        for output in outputs.iter().skip(outputs.len() / 2) {
            let dbo = DbUnblindedOutput::from_unblinded_output(output.clone(), &factories, None).unwrap();
            db_unblinded_outputs.push(dbo);
        }
    }
    test_interface
        .oms_mock_state
        .set_recoverable_outputs(db_unblinded_outputs.clone());

    let (tx, rx) = mpsc::channel(100);
    test_interface.rpc_service_state.set_utxos_by_block_trigger_channel(rx);

    tokio::spawn(test_interface.scanner_service.take().unwrap().run());

    tx.send(SYNC_INTERRUPT as usize).await.unwrap();

    let _result = test_interface
        .rpc_service_state
        .wait_pop_sync_utxos_by_block_calls(1, Duration::from_secs(30))
        .await
        .unwrap();

    test_interface.shutdown_signal.trigger();

    // So at this point we have synced to block 6. We are going to create a reorg back to block 4 so that blocks 5-10
    // are new blocks.
    block_headers.retain(|h, _| h <= &4u64);
    unblinded_outputs.retain(|h, _| h <= &4u64);
    let mut utxos_by_block = utxos_by_block
        .into_iter()
        .filter(|u| u.height <= 4)
        .collect::<Vec<UtxosByBlock>>();

    let TestBlockData {
        block_headers: new_block_headers,
        unblinded_outputs: new_unblinded_outputs,
        utxos_by_block: mut new_utxos_by_block,
    } = generate_block_headers_and_utxos(5, 5, birthday_epoch_time + 500, 0, false).await;

    block_headers.extend(new_block_headers);
    utxos_by_block.append(&mut new_utxos_by_block);
    unblinded_outputs.extend(new_unblinded_outputs);

    let mut test_interface2 = setup(UtxoScannerMode::Recovery, Some(test_interface.wallet_db), None, None).await;
    test_interface2
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface2.rpc_service_state.set_blocks(block_headers.clone());
    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(9),
        best_block: Some(block_headers.get(&9).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };
    test_interface2
        .rpc_service_state
        .set_tip_info_response(TipInfoResponse {
            metadata: Some(chain_metadata),
            is_synced: true,
        });

    // calculate new recoverable outputs for the reorg
    // Adding half the outputs of the blocks to the OMS mock
    let mut db_unblinded_outputs = Vec::new();
    let mut total_outputs_to_recover = 0;
    let mut total_amount_to_recover = MicroTari::from(0);
    for (h, outputs) in &unblinded_outputs {
        for output in outputs.iter().skip(outputs.len() / 2) {
            let dbo = DbUnblindedOutput::from_unblinded_output(output.clone(), &factories, None).unwrap();
            // Only the outputs in blocks after the birthday should be included in the recovered total
            if *h >= 4 {
                total_outputs_to_recover += 1;
                total_amount_to_recover += dbo.unblinded_output.value;
            }
            db_unblinded_outputs.push(dbo);
        }
    }

    test_interface2
        .oms_mock_state
        .set_recoverable_outputs(db_unblinded_outputs);

    let mut scanner_event_stream = test_interface2.scanner_handle.get_event_receiver();
    tokio::spawn(test_interface2.scanner_service.take().unwrap().run());

    let delay = time::sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => {
                panic!("Completed event should have arrived by now.");
            }
            event = scanner_event_stream.recv() => {
                if let UtxoScannerEvent::Completed {
                    final_height,
                    num_recovered,
                    value_recovered,
                    time_taken: _,} = event.unwrap() {
                    assert_eq!(final_height, 9);
                    assert_eq!(num_recovered, total_outputs_to_recover);
                    assert_eq!(value_recovered, total_amount_to_recover);
                    break;
                }
            }
        }
    }
}

#[tokio::test]
async fn test_utxo_scanner_scanned_block_cache_clearing() {
    let mut test_interface = setup(UtxoScannerMode::Recovery, None, None, None).await;

    for h in 0u64..800u64 {
        let num_outputs = if h % 2 == 1 { Some(1) } else { None };
        let mut header_hash = h.to_le_bytes().to_vec();
        header_hash.extend([0u8; 24].to_vec());
        test_interface
            .wallet_db
            .save_scanned_block(ScannedBlock {
                header_hash,
                height: h,
                num_outputs,
                amount: None,
                timestamp: Utc::now()
                    .naive_utc()
                    .checked_sub_signed(ChronoDuration::days(1000))
                    .unwrap(),
            })
            .await
            .unwrap();
    }

    let cipher_seed = CipherSeed::new();
    let birthday_epoch_time = u64::from(cipher_seed.birthday() - 2) * 60 * 60 * 24;
    test_interface.wallet_db.set_master_seed(cipher_seed).await.unwrap();

    const NUM_BLOCKS: u64 = 11;
    const BIRTHDAY_OFFSET: u64 = 5;

    let TestBlockData {
        block_headers,
        unblinded_outputs: _unblinded_outputs,
        utxos_by_block,
    } = generate_block_headers_and_utxos(800, NUM_BLOCKS, birthday_epoch_time, BIRTHDAY_OFFSET, true).await;

    test_interface
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface.rpc_service_state.set_blocks(block_headers.clone());

    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(800 + NUM_BLOCKS - 1),
        best_block: Some(block_headers.get(&(800 + NUM_BLOCKS - 1)).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };
    test_interface.rpc_service_state.set_tip_info_response(TipInfoResponse {
        metadata: Some(chain_metadata),
        is_synced: true,
    });

    let first_block_header = block_headers.get(&(800)).unwrap().clone();
    test_interface
        .wallet_db
        .save_scanned_block(ScannedBlock {
            header_hash: first_block_header.hash(),
            height: first_block_header.height,
            num_outputs: Some(0),
            amount: None,
            timestamp: Utc::now().naive_utc(),
        })
        .await
        .unwrap();

    let mut scanner_event_stream = test_interface.scanner_handle.get_event_receiver();

    tokio::spawn(test_interface.scanner_service.take().unwrap().run());

    let delay = time::sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => {
                panic!("Completed event should have arrived by now.");
            }
            event = scanner_event_stream.recv() => {
                if let UtxoScannerEvent::Completed {
                    final_height:_,
                    num_recovered:_,
                    value_recovered:_,
                    time_taken: _,} = event.unwrap(){
                    break;
                }
            }
        }
    }
    let scanned_blocks = test_interface.wallet_db.get_scanned_blocks().await.unwrap();

    use tari_wallet::utxo_scanner_service::service::SCANNED_BLOCK_CACHE_SIZE;
    let threshold = 800 + NUM_BLOCKS - 1 - SCANNED_BLOCK_CACHE_SIZE;

    // Below the threshold the even indices had no outputs and should be cleared
    for i in 0..threshold {
        if i % 2 == 0 {
            assert!(!scanned_blocks.iter().any(|sb| sb.height == i));
        }
    }
    // Check that above the threshold the even indices are still there
    for i in threshold..800 {
        if i % 2 == 0 {
            assert!(scanned_blocks.iter().any(|sb| sb.height == i));
        }
    }
}

#[tokio::test]
async fn test_utxo_scanner_one_sided_payments() {
    let factories = CryptoFactories::default();
    let mut test_interface = setup(
        UtxoScannerMode::Scanning,
        None,
        None,
        Some("one-sided non-default".to_string()),
    )
    .await;

    let cipher_seed = CipherSeed::new();
    let birthday_epoch_time = u64::from(cipher_seed.birthday() - 2) * 60 * 60 * 24;
    test_interface.wallet_db.set_master_seed(cipher_seed).await.unwrap();

    const NUM_BLOCKS: u64 = 11;
    const BIRTHDAY_OFFSET: u64 = 5;

    let TestBlockData {
        mut block_headers,
        unblinded_outputs,
        mut utxos_by_block,
    } = generate_block_headers_and_utxos(0, NUM_BLOCKS, birthday_epoch_time, BIRTHDAY_OFFSET, false).await;

    test_interface
        .rpc_service_state
        .set_utxos_by_block(utxos_by_block.clone());
    test_interface.rpc_service_state.set_blocks(block_headers.clone());

    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(NUM_BLOCKS - 1),
        best_block: Some(block_headers.get(&(NUM_BLOCKS - 1)).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };
    test_interface.rpc_service_state.set_tip_info_response(TipInfoResponse {
        metadata: Some(chain_metadata),
        is_synced: true,
    });

    // Adding half the outputs of the blocks to the OMS mock
    let mut db_unblinded_outputs = Vec::new();
    let mut total_outputs_to_recover = 0;
    let mut total_amount_to_recover = MicroTari::from(0);
    for (h, outputs) in &unblinded_outputs {
        for output in outputs.iter().skip(outputs.len() / 2) {
            let dbo = DbUnblindedOutput::from_unblinded_output(output.clone(), &factories, None).unwrap();
            // Only the outputs in blocks after the birthday should be included in the recovered total
            if *h >= NUM_BLOCKS.saturating_sub(BIRTHDAY_OFFSET).saturating_sub(2) {
                total_outputs_to_recover += 1;
                total_amount_to_recover += dbo.unblinded_output.value;
            }
            db_unblinded_outputs.push(dbo);
        }
    }
    test_interface
        .oms_mock_state
        .set_one_sided_payments(db_unblinded_outputs.clone());

    let mut scanner_event_stream = test_interface.scanner_handle.get_event_receiver();

    tokio::spawn(test_interface.scanner_service.take().unwrap().run());

    let delay = time::sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => {
                panic!("Completed event should have arrived by now.");
            }
            event = scanner_event_stream.recv() => {
                if let UtxoScannerEvent::Completed {
                    final_height,
                    num_recovered,
                    value_recovered,
                    time_taken: _,} = event.unwrap() {
                    assert_eq!(final_height, NUM_BLOCKS-1);
                    assert_eq!(num_recovered, total_outputs_to_recover);
                    assert_eq!(value_recovered, total_amount_to_recover);
                    break;
                }
            }
        }
    }

    let requests = test_interface.transaction_service_mock_state.drain_requests();
    assert!(!requests.is_empty());
    for req in requests {
        if let TransactionServiceRequest::ImportUtxoWithStatus {
            amount: _,
            source_public_key: _,
            message,
            maturity: _,
            import_status: _,
            tx_id: _,
            current_height: _,
        } = req
        {
            assert_eq!(message, "one-sided non-default".to_string());
        }
    }

    // Now we add a new block and emit a NewBlockDetected event to trigger another round of scan and
    // see if the updated message appears in the newly found Faux tx
    let mut block_header11 = BlockHeader::new(0);
    block_header11.height = 11;
    block_header11.timestamp = EpochTime::from(block_headers.get(&10).unwrap().timestamp.as_u64() + 1000000u64);
    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(666000u64), &factories.commitment, None).await;

    let block11 = UtxosByBlock {
        height: NUM_BLOCKS,
        header_hash: block_header11.hash(),
        utxos: vec![uo.as_transaction_output(&factories).unwrap()],
    };

    utxos_by_block.push(block11);
    block_headers.insert(NUM_BLOCKS, block_header11);

    db_unblinded_outputs.push(DbUnblindedOutput::from_unblinded_output(uo, &factories, None).unwrap());
    test_interface
        .oms_mock_state
        .set_one_sided_payments(db_unblinded_outputs);

    test_interface.rpc_service_state.set_utxos_by_block(utxos_by_block);
    test_interface.rpc_service_state.set_blocks(block_headers.clone());

    test_interface
        .scanner_handle
        .set_one_sided_payment_message("new one-sided message".to_string());

    let chain_metadata = ChainMetadata {
        height_of_longest_chain: Some(NUM_BLOCKS),
        best_block: Some(block_headers.get(&(NUM_BLOCKS)).unwrap().clone().hash()),
        accumulated_difficulty: Vec::new(),
        pruned_height: 0,
    };

    test_interface.rpc_service_state.set_tip_info_response(TipInfoResponse {
        metadata: Some(chain_metadata),
        is_synced: true,
    });
    time::sleep(Duration::from_secs(5)).await;

    test_interface
        .base_node_service_event_publisher
        .send(Arc::new(BaseNodeEvent::NewBlockDetected(11)))
        .unwrap();

    let delay = time::sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => {
                panic!("Completed event should have arrived by now.");
            }
            event = scanner_event_stream.recv() => {
                if let UtxoScannerEvent::Completed {
                    final_height,
                    num_recovered: _,
                    value_recovered: _,
                    time_taken: _,} = event.unwrap() {
                    assert_eq!(final_height, NUM_BLOCKS);

                    break;
                }
            }
        }
    }

    let requests = test_interface.transaction_service_mock_state.drain_requests();
    assert!(!requests.is_empty());

    for req in requests {
        if let TransactionServiceRequest::ImportUtxoWithStatus {
            amount: _,
            source_public_key: _,
            message,
            maturity: _,
            import_status: _,
            tx_id: _,
            current_height: h,
        } = req
        {
            println!("{:?}", h);
            assert_eq!(message, "new one-sided message".to_string());
        }
    }
}
