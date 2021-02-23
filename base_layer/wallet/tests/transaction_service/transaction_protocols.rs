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

use crate::support::{
    rpc::{BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    utils::{make_input, random_string},
};
use chrono::Utc;
use futures::{FutureExt, StreamExt};
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    peer_manager::PeerFeatures,
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService, RpcStatus},
    test_utils::{
        mocks::{create_connectivity_mock, ConnectivityManagerMockState},
        node_identity::build_node_identity,
    },
    types::CommsPublicKey,
    NodeIdentity,
    Substream,
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletRpcServer,
    },
    transactions::{
        helpers::schema_to_transaction,
        tari_amount::{uT, MicroTari, T},
        types::CryptoFactories,
    },
    txn_schema,
};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::Shutdown;
use tari_wallet::{
    output_manager_service::{
        error::OutputManagerError,
        handle::{OutputManagerHandle, OutputManagerRequest, OutputManagerResponse},
        TxId,
    },
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionEventSender},
        protocols::{
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_validation_protocol::TransactionValidationProtocol,
        },
        service::TransactionServiceResources,
        storage::{
            database::TransactionDatabase,
            models::{CompletedTransaction, TransactionDirection, TransactionStatus},
            sqlite_db::TransactionServiceSqliteDatabase,
        },
    },
    types::ValidationRetryStrategy,
};
use tempfile::{tempdir, TempDir};
use tokio::{sync::broadcast, task, time::delay_for};

// Just in case other options become apparent in later testing
#[derive(PartialEq)]
pub enum TxProtocolTestConfig {
    WithConnection,
    WithoutConnection,
}

pub async fn setup(
    config: TxProtocolTestConfig,
) -> (
    TransactionServiceResources<TransactionServiceSqliteDatabase>,
    ConnectivityManagerMockState,
    OutboundServiceMockState,
    MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>, Substream>,
    Arc<NodeIdentity>,
    BaseNodeWalletRpcMockState,
    broadcast::Sender<Duration>,
    Shutdown,
    TempDir,
) {
    let client_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (connectivity_manager, connectivity_mock) = create_connectivity_mock();

    let connectivity_mock_state = connectivity_mock.get_shared_state();

    connectivity_mock.spawn();

    let service = BaseNodeWalletRpcMockService::new();
    let rpc_service_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();

    let mut mock_server = MockRpcServer::new(server, server_node_identity.clone());

    mock_server.serve();

    if config == TxProtocolTestConfig::WithConnection {
        let connection = mock_server
            .create_connection(server_node_identity.to_peer(), protocol_name.into())
            .await;
        connectivity_mock_state.add_active_connection(connection).await;
    }

    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let db_connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();
    let db = TransactionDatabase::new(TransactionServiceSqliteDatabase::new(db_connection, None));

    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    task::spawn(oms_reply_channel_task(oms_request_receiver));

    let (oms_event_publisher, _) = broadcast::channel(200);
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(100);
    let outbound_mock_state = mock_outbound_service.get_state();
    task::spawn(mock_outbound_service.run());

    let (ts_event_publisher, _): (TransactionEventSender, _) = broadcast::channel(200);

    let shutdown = Shutdown::new();

    let resources = TransactionServiceResources {
        db,
        output_manager_service: output_manager_service_handle,
        outbound_message_service: outbound_message_requester,
        connectivity_manager,
        event_publisher: ts_event_publisher,
        node_identity: client_node_identity,
        factories: CryptoFactories::default(),
        config: TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(3),
            max_tx_query_batch_size: 2,
            ..TransactionServiceConfig::default()
        },
        shutdown_signal: shutdown.to_signal(),
    };

    let (timeout_update_publisher, _) = broadcast::channel(20);

    return (
        resources,
        connectivity_mock_state,
        outbound_mock_state,
        mock_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        shutdown,
        temp_dir,
    );
}

pub async fn add_transaction_to_database(
    tx_id: TxId,
    amount: MicroTari,
    valid: bool,
    status: Option<TransactionStatus>,
    db: TransactionDatabase<TransactionServiceSqliteDatabase>,
)
{
    let factories = CryptoFactories::default();
    let (_utxo, uo0) = make_input(&mut OsRng, 10 * amount, &factories.commitment);
    let (txs1, _uou1) = schema_to_transaction(&vec![txn_schema!(from: vec![uo0.clone()], to: vec![amount])]);
    let tx1 = (*txs1[0]).clone();
    let mut completed_tx1 = CompletedTransaction::new(
        tx_id,
        CommsPublicKey::default(),
        CommsPublicKey::default(),
        amount,
        200 * uT,
        tx1.clone(),
        status.unwrap_or(TransactionStatus::Completed),
        "Test".to_string(),
        Utc::now().naive_local(),
        TransactionDirection::Outbound,
        None,
    );
    completed_tx1.valid = valid;
    db.insert_completed_transaction(tx_id, completed_tx1).await.unwrap();
}

/// Simple task that responds with a OutputManagerResponse::TransactionCancelled response to any request made on this
/// channel
pub async fn oms_reply_channel_task(
    mut receiver: Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>,
) {
    while let Some(request_context) = receiver.next().await {
        let (request, reply_tx) = request_context.split();
        let response = match request {
            OutputManagerRequest::ConfirmTransaction(_) => Ok(OutputManagerResponse::TransactionConfirmed),
            OutputManagerRequest::CancelTransaction(_) => Ok(OutputManagerResponse::TransactionCancelled),
            _ => Err(OutputManagerError::InvalidResponseError(
                "Unhandled request type".to_string(),
            )),
        };

        let _ = reply_tx.send(response);
    }
}

/// A happy path test by submitting a transaction into the mempool, have it mined but unconfirmed and then confirmed.
#[tokio_macros::test]
async fn tx_broadcast_protocol_submit_success() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let mut event_stream = resources.event_publisher.subscribe().fuse();
    let (base_node_update_publisher, _) = broadcast::channel(20);

    let protocol = TransactionBroadcastProtocol::new(
        2,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );
    let join_handle = task::spawn(protocol.execute());

    // Fails because there is no transaqction in the database to be broadcast
    assert!(join_handle.await.unwrap().is_err());

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    // Set Base Node response to be not synced but in mempool
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: true,
        rejection_reason: TxSubmissionRejectionReason::None,
        is_synced: false,
    });

    // Wait for 2 queries
    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(4, Duration::from_secs(5))
        .await
        .unwrap();

    // Accepted in the mempool but not mined yet
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: true,
        rejection_reason: TxSubmissionRejectionReason::None,
        is_synced: true,
    });

    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set Base Node response to be mined but unconfirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: false,
        height_of_longest_chain: 0,
    });
    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::Broadcast);

    // Set Base Node response to be mined but unconfirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });
    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedUnconfirmed);

    // Set base node response to mined and confirmed but not synced
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: false,
        height_of_longest_chain: 0,
    });

    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedConfirmed);

    // Set base node response to mined and confirmed and synced
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedConfirmed);

    // Check that the appropriate events were emitted
    let mut delay = delay_for(Duration::from_secs(1)).fuse();
    let mut broadcast = false;
    let mut unconfirmed = false;
    let mut confirmed = false;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match &*event.unwrap() {
                        TransactionEvent::TransactionMinedUnconfirmed(_, confirmations) => if *confirmations == 1 {
                            unconfirmed = true;
                        }
                        TransactionEvent::TransactionMined(_) => {
                            confirmed = true;
                        },
                        TransactionEvent::TransactionBroadcast(_) => {
                            broadcast = true;
                        },
                        _ => (),
                        }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(
        unconfirmed,
        "Should have received at least 1 TransactionEvent::TransactionMinedUnconfirmed event"
    );
    assert!(confirmed, "Should have received a confirmed event");
    assert!(broadcast, "Should have received a broadcast event");
}

/// Test submitting a transaction that is immediately rejected
#[tokio_macros::test]
async fn tx_broadcast_protocol_submit_rejection() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let mut event_stream = resources.event_publisher.subscribe().fuse();
    let (base_node_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: false,
        rejection_reason: TxSubmissionRejectionReason::Orphan,
        is_synced: true,
    });

    let join_handle = task::spawn(protocol.execute());

    // Check that the protocol ends with rejection error
    if let Err(e) = join_handle.await.unwrap() {
        if let TransactionServiceError::MempoolRejectionOrphan = e.error {
            assert!(true);
        } else {
            assert!(
                false,
                "Tx broadcast Should have failed with mempool rejection for being an orphan"
            );
        }
    } else {
        assert!(false, "Tx broadcast Should have failed");
    }

    // Check transaction is cancelled in db
    let db_completed_tx = resources.db.get_completed_transaction(1).await;
    assert!(db_completed_tx.is_err());

    // Check that the appropriate events were emitted
    let mut delay = delay_for(Duration::from_secs(1)).fuse();
    let mut cancelled = false;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match &*event.unwrap() {
                        TransactionEvent::TransactionCancelled(_) => {
                            cancelled = true;
                        },
                        _ => (),
                        }
            },
            () = delay => {
                break;
            },
        }
    }

    assert!(cancelled, "Should have cancelled transaction");
}

/// Test restarting a protocol which means the first step is a query not a submission, detecting the Tx is not in the
/// mempool, resubmit the tx and then have it mined
#[tokio_macros::test]
async fn tx_broadcast_protocol_restart_protocol_as_query() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    // Set Base Node query response to be not stored, as if the base node does not have the tx in its pool
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    // Check if in mempool (its not)
    // Wait for 1 queries
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set Base Node query response to be InMempool as if the base node does not have the tx in its pool
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::InMempool,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Should receive a resummission call
    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(5))
        .await
        .expect("Should receive a resubmission call");

    // Wait for 1 more query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set base node response to mined and confirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedConfirmed);
}

/// This test will submit a Tx which will be accepted and then dropped from the mempool, resulting in a resubmit which
/// will be rejected and result in a cancelled transaction
#[tokio_macros::test]
async fn tx_broadcast_protocol_submit_success_followed_by_rejection() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let mut event_stream = resources.event_publisher.subscribe().fuse();
    let (base_node_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    // Accepted in the mempool but not mined yet
    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set Base Node response to be rejected by mempool
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Set Base Node to reject resubmission
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: false,
        rejection_reason: TxSubmissionRejectionReason::TimeLocked,
        is_synced: true,
    });

    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Check that the protocol ends with rejection error
    if let Err(e) = join_handle.await.unwrap() {
        println!("{:?}", e);
        if let TransactionServiceError::MempoolRejectionTimeLocked = e.error {
            assert!(true);
        } else {
            assert!(
                false,
                "Tx broadcast Should have failed with mempool rejection for being time locked"
            );
        }
    } else {
        assert!(false, "Tx broadcast Should have failed");
    }

    // Check transaction is cancelled in db
    let db_completed_tx = resources.db.get_completed_transaction(1).await;
    assert!(db_completed_tx.is_err());

    // Check that the appropriate events were emitted
    let mut delay = delay_for(Duration::from_secs(1)).fuse();
    let mut cancelled = false;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match &*event.unwrap() {
                        TransactionEvent::TransactionCancelled(_) => {
                            cancelled = true;
                        },
                        _ => (),
                        }
            },
            () = delay => {
                break;
            },
        }
    }

    assert!(cancelled, "Should have cancelled transaction");
}

/// This test will submit a tx which is accepted and mined but unconfirmed, then the next query it will not exist
/// resulting in a resubmission which we will let run to being mined with success
#[tokio_macros::test]
async fn tx_broadcast_protocol_submit_mined_then_not_mined_resubmit_success() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(5))
        .await
        .expect("Should receive a submission call");

    // Accepted in the mempool but not mined yet
    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set Base Node response to be mined but unconfirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });
    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedUnconfirmed);

    // Set base node response to mined and confirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Should receive a resubmission call
    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(5))
        .await
        .expect("Should receive a resubmission call");

    // Set Base Node response to be mined and confirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required as u64 + 1u64,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedConfirmed);
}

/// Test being unable to connect and then connection becoming available.
#[tokio_macros::test]
async fn tx_broadcast_protocol_connection_problem() {
    let (
        resources,
        connectivity_mock_state,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithoutConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);

    let mut event_stream = resources.event_publisher.subscribe().fuse();

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    // Check that the connection problem event was emitted at least twice
    let mut delay = delay_for(Duration::from_secs(10)).fuse();
    let mut connection_issues = 0;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match &*event.unwrap() {
                    TransactionEvent::TransactionBaseNodeConnectionProblem(_) => {
                        connection_issues +=1 ;
                    }
                    _ => (),
                }
                if connection_issues >= 2 {
                    break;
                }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(connection_issues >= 2, "Should have retried connection at least twice");

    // Now we add the connection
    let connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    connectivity_mock_state.add_active_connection(connection).await;

    // Check that the protocol ends with success
    // Set Base Node response to be mined and confirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required as u64 + 1u64,
        is_synced: true,
        height_of_longest_chain: 0,
    });
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);
}

/// Submit a transaction that is Already Mined for the submission, the subsequent query should confirm the transaction
#[tokio_macros::test]
async fn tx_broadcast_protocol_submit_already_mined() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    // Set Base Node to respond with AlreadyMined
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: false,
        rejection_reason: TxSubmissionRejectionReason::AlreadyMined,
        is_synced: true,
    });

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(5))
        .await
        .expect("Should receive a submission call");

    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set base node response to mined and confirmed
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedConfirmed);
}

/// A test to see that the broadcast protocol can handle a change to the base node address while it runs.
#[tokio_macros::test]
async fn tx_broadcast_protocol_submit_and_base_node_gets_changed() {
    let (
        resources,
        connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(1, 1 * T, true, None, resources.db.clone()).await;

    let protocol = TransactionBroadcastProtocol::new(
        1,
        resources.clone(),
        Duration::from_secs(1),
        server_node_identity.public_key().clone(),
        timeout_update_publisher.subscribe(),
        base_node_update_publisher.subscribe(),
    );

    let join_handle = task::spawn(protocol.execute());

    // Accepted in the mempool but not mined yet
    // Wait for 2 queries
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(2, Duration::from_secs(5))
        .await
        .unwrap();

    // Setup new RPC Server
    let new_server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let service = BaseNodeWalletRpcMockService::new();
    let new_rpc_service_state = service.get_state();

    let new_server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = new_server.as_protocol_name();
    let mut new_mock_server = MockRpcServer::new(new_server, new_server_node_identity.clone());
    new_mock_server.serve();

    let connection = new_mock_server
        .create_connection(new_server_node_identity.to_peer(), protocol_name.into())
        .await;
    connectivity_mock_state.add_active_connection(connection).await;

    // Set new Base Node response to be mined but unconfirmed
    new_rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Change Base Node
    base_node_update_publisher
        .send(new_server_node_identity.public_key().clone())
        .unwrap();

    // Update old base node to reject the tx to check that the protocol is using the new base node
    // Set Base Node query response to be InMempool as if the base node does not have the tx in its pool
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Wait for 1 query
    let _ = new_rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(5))
        .await
        .unwrap();

    // Set base node response to mined and confirmed
    new_rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::MinedConfirmed);
}

/// Validate completed transactions, will check that valid ones stay valid and incorrectly marked invalid tx become
/// valid.
#[tokio_macros::test]
async fn tx_validation_protocol_tx_becomes_valid() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        2,
        2 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        3,
        3 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        4,
        4 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: true,
        height_of_longest_chain: 0,
    });

    rpc_service_state.set_is_synced(false);

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(1),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::UntilSuccess,
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(6, Duration::from_secs(60))
        .await
        .unwrap();

    rpc_service_state.set_is_synced(true);

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(2, Duration::from_secs(60))
        .await
        .unwrap();

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    // Check transaction status is updated
    let db_completed_txs = resources.db.get_completed_transactions().await.unwrap();

    for tx in db_completed_txs.values() {
        assert!(tx.valid, "TxId: {} should be valid", tx.tx_id);
    }
}

/// Validate completed transaction, the transaction should become invalid
#[tokio_macros::test]
async fn tx_validation_protocol_tx_becomes_invalid() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: true,
        height_of_longest_chain: 0,
    });

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(1),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::UntilSuccess,
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    // Check transaction status is updated
    let db_completed_txs = resources.db.get_completed_transactions().await.unwrap();

    for tx in db_completed_txs.values() {
        assert!(!tx.valid, "TxId: {} should be invalid", tx.tx_id);
    }
}

/// Validate completed transactions, the transaction should become invalid
#[tokio_macros::test]
async fn tx_validation_protocol_tx_becomes_unconfirmed() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    // Set Base Node to respond with AlreadyMined
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(1),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::UntilSuccess,
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    // Check transaction status is updated
    let db_completed_txs = resources.db.get_completed_transactions().await.unwrap();

    for tx in db_completed_txs.values() {
        assert_eq!(
            tx.status,
            TransactionStatus::MinedUnconfirmed,
            "TxId: {} should be unconfirmed",
            tx.tx_id
        );
    }
}

/// Test the validation protocol reacts correctly to a change in base node and redoes the full validation based on the
/// new base node
#[tokio_macros::test]
async fn tx_validation_protocol_tx_ends_on_base_node_end() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        mut rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);
    let mut event_stream = resources.event_publisher.subscribe().fuse();

    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        2,
        2 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        3,
        3 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        4,
        4 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        5,
        5 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        6,
        6 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    // Set Base Node to respond with AlreadyMined
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    rpc_service_state.set_response_delay(Some(Duration::from_secs(5)));

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(10),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::UntilSuccess,
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    // Setup new RPC Server
    let new_server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    // Change Base Node
    base_node_update_publisher
        .send(new_server_node_identity.public_key().clone())
        .unwrap();

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let mut delay = delay_for(Duration::from_secs(1)).fuse();
    let mut aborted = false;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match &*event.unwrap() {
                        TransactionEvent::TransactionValidationAborted(_) => {
                            aborted = true;
                        },
                        _ => (),
                        }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(aborted, "Validation protocol should have aborted");
}

/// Test the validation protocol reacts correctly when the RPC client returns an error between calls.
#[tokio_macros::test]
async fn tx_validation_protocol_rpc_client_broken_between_calls() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        mut rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);

    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        2,
        2 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        3,
        3 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        4,
        4 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        5,
        5 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        6,
        6 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    // Set Base Node to respond with AlreadyMined
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    rpc_service_state.set_response_delay(Some(Duration::from_secs(5)));

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(10),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::UntilSuccess,
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    rpc_service_state.set_rpc_status_error(Some(RpcStatus::bad_request("blah".to_string())));

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    rpc_service_state.set_rpc_status_error(None);
    rpc_service_state.set_response_delay(None);

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    // Check transaction status is updated
    let db_completed_txs = resources.db.get_completed_transactions().await.unwrap();

    for tx in db_completed_txs.values() {
        assert!(tx.valid, "TxId: {} should be valid", tx.tx_id);
    }
}

/// Test the validation protocol reacts correctly when the RPC client returns an error between calls and only retry
/// finite amount of times
#[tokio_macros::test]
async fn tx_validation_protocol_rpc_client_broken_finite_retries() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        mut rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);
    let mut event_stream = resources.event_publisher.subscribe().fuse();
    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        2,
        2 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    // Set Base Node to respond with AlreadyMined
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });
    rpc_service_state.set_rpc_status_error(Some(RpcStatus::bad_request("blah".to_string())));

    rpc_service_state.set_response_delay(Some(Duration::from_secs(1)));

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(5),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::Limited(2),
    );

    let join_handle = task::spawn(protocol.execute());

    let _ = rpc_service_state
        .wait_pop_transaction_batch_query_calls(3, Duration::from_secs(60))
        .await
        .unwrap();

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_err());

    // Check that the connection problem event was emitted at least twice
    let mut delay = delay_for(Duration::from_secs(10)).fuse();
    let mut timeouts = 0i32;
    let mut failures = 0i32;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
            log::error!("EVENT: {:?}", event);
                match &*event.unwrap() {
                    TransactionEvent::TransactionValidationTimedOut(_) => {
                        timeouts +=1 ;
                    }
                    TransactionEvent::TransactionValidationFailure(_) => {
                        failures +=1 ;
                    }
                    _ => (),
                }
                if failures + timeouts >= 4 {
                    break;
                }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(timeouts >= 3, "Should have timed out twice");
    assert!(failures >= 1, "Should have failed");
}

/// Validate completed transactions, will check that valid ones stay valid and incorrectly marked invalid tx become
/// valid.
#[tokio_macros::test]
async fn tx_validation_protocol_base_node_not_synced() {
    let (
        resources,
        _connectivity_mock_state,
        _outbound_mock_state,
        _mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _timeout_update_publisher,
        _shutdown,
        _temp_dir,
    ) = setup(TxProtocolTestConfig::WithConnection).await;
    let (base_node_update_publisher, _) = broadcast::channel(20);
    let (_timeout_update_publisher, _) = broadcast::channel(20);
    let mut event_stream = resources.event_publisher.subscribe().fuse();

    add_transaction_to_database(
        1,
        1 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        2,
        2 * T,
        false,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        3,
        3 * T,
        true,
        Some(TransactionStatus::MinedConfirmed),
        resources.db.clone(),
    )
    .await;

    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::Mined,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required.into(),
        is_synced: false,
        height_of_longest_chain: 0,
    });

    rpc_service_state.set_is_synced(false);

    let protocol = TransactionValidationProtocol::new(
        1,
        resources.clone(),
        server_node_identity.public_key().clone(),
        Duration::from_secs(1),
        base_node_update_publisher.subscribe(),
        _timeout_update_publisher.subscribe(),
        ValidationRetryStrategy::Limited(0),
    );

    let join_handle = task::spawn(protocol.execute());

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert!(result.is_err());

    let mut delay = delay_for(Duration::from_secs(10)).fuse();
    let mut delayed = 0i32;
    let mut failures = 0i32;
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match &*event.unwrap() {
                    TransactionEvent::TransactionValidationDelayed(_) => {
                        delayed +=1 ;
                    }
                    TransactionEvent::TransactionValidationFailure(_) => {
                        failures +=1 ;
                    }
                    _ => (),
                }

            },
            () = delay => {
                break;
            },
        }
    }
    assert!(delayed >= 1, "Should have been delayed");
    assert_eq!(failures, 0, "Should not have failed when BN is not synced");
}
