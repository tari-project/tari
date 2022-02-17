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

use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Utc;
use futures::StreamExt;
use rand::rngs::OsRng;
use tari_common_types::transaction::{TransactionDirection, TransactionStatus, TxId};
use tari_comms::{
    peer_manager::PeerFeatures,
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
    test_utils::node_identity::build_node_identity,
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletRpcServer,
    },
    blocks::BlockHeader,
    proto::{
        base_node::{
            TxLocation as TxLocationProto,
            TxQueryBatchResponse as TxQueryBatchResponseProto,
            TxQueryBatchResponses as TxQueryBatchResponsesProto,
        },
        types::Signature as SignatureProto,
    },
    transactions::{
        tari_amount::{uT, MicroTari, T},
        test_helpers::schema_to_transaction,
        CryptoFactories,
    },
    txn_schema,
};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::Shutdown;
use tari_test_utils::random;
use tari_utilities::Hashable;
use tari_wallet::{
    connectivity_service::{create_wallet_connectivity_mock, WalletConnectivityMock},
    output_manager_service::{
        error::OutputManagerError,
        handle::{OutputManagerHandle, OutputManagerRequest, OutputManagerResponse},
    },
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionEventReceiver, TransactionEventSender},
        protocols::{
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_validation_protocol::TransactionValidationProtocol,
        },
        service::TransactionServiceResources,
        storage::{
            database::TransactionDatabase,
            models::{CompletedTransaction, TxCancellationReason},
            sqlite_db::TransactionServiceSqliteDatabase,
        },
    },
    util::watch::Watch,
};
use tempfile::{tempdir, TempDir};
use tokio::{sync::broadcast, task, time::sleep};

use crate::support::{
    comms_rpc::{connect_rpc_client, BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    utils::make_input,
};

pub async fn setup() -> (
    TransactionServiceResources<TransactionServiceSqliteDatabase, WalletConnectivityMock>,
    OutboundServiceMockState,
    MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    Arc<NodeIdentity>,
    BaseNodeWalletRpcMockState,
    Shutdown,
    TempDir,
    TransactionEventReceiver,
    WalletConnectivityMock,
) {
    let client_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let service = BaseNodeWalletRpcMockService::new();
    let rpc_service_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();

    let mut mock_rpc_server = MockRpcServer::new(server, server_node_identity.clone());
    mock_rpc_server.serve();

    let wallet_connectivity = create_wallet_connectivity_mock();

    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), protocol_name.into())
        .await;

    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let db_name = format!("{}.sqlite3", random::string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let db_connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name), 16).unwrap();

    let db = TransactionDatabase::new(TransactionServiceSqliteDatabase::new(db_connection, None));

    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    task::spawn(oms_reply_channel_task(oms_request_receiver));

    let (oms_event_publisher, _) = broadcast::channel(200);
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(100);
    let outbound_mock_state = mock_outbound_service.get_state();
    task::spawn(mock_outbound_service.run());

    let (ts_event_publisher, ts_event_receiver): (TransactionEventSender, TransactionEventReceiver) =
        broadcast::channel(200);

    let shutdown = Shutdown::new();

    let resources = TransactionServiceResources {
        db,
        output_manager_service: output_manager_service_handle,
        outbound_message_service: outbound_message_requester,
        connectivity: wallet_connectivity.clone(),
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

    (
        resources,
        outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        shutdown,
        temp_dir,
        ts_event_receiver,
        wallet_connectivity,
    )
}

pub async fn add_transaction_to_database(
    tx_id: TxId,
    amount: MicroTari,
    status: Option<TransactionStatus>,
    coinbase_block_height: Option<u64>,
    db: TransactionDatabase<TransactionServiceSqliteDatabase>,
) {
    let factories = CryptoFactories::default();
    let (_utxo, uo0) = make_input(&mut OsRng, 10 * amount, &factories.commitment);
    let (txs1, _uou1) = schema_to_transaction(&[txn_schema!(from: vec![uo0.clone()], to: vec![amount])]);
    let tx1 = (*txs1[0]).clone();
    let completed_tx1 = CompletedTransaction::new(
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
        coinbase_block_height,
        None,
    );
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
            OutputManagerRequest::CancelTransaction(_) => Ok(OutputManagerResponse::TransactionCancelled),
            _ => Err(OutputManagerError::InvalidResponseError(
                "Unhandled request type".to_string(),
            )),
        };

        let _ = reply_tx.send(response);
    }
}

/// A happy path test by submitting a transaction into the mempool
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_broadcast_protocol_submit_success() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    let mut event_stream = resources.event_publisher.subscribe();

    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let timeout_watch = Watch::new(Duration::from_secs(1));

    let protocol = TransactionBroadcastProtocol::new(2.into(), resources.clone(), timeout_watch.get_receiver());
    let join_handle = task::spawn(protocol.execute());

    // Fails because there is no transaction in the database to be broadcast
    assert!(join_handle.await.unwrap().is_err());

    add_transaction_to_database(1.into(), 1 * T, None, None, resources.db.clone()).await;

    let db_completed_tx = resources.db.get_completed_transaction(1.into()).await.unwrap();
    assert!(db_completed_tx.confirmations.is_none());

    let protocol = TransactionBroadcastProtocol::new(1.into(), resources.clone(), timeout_watch.get_receiver());

    task::spawn(protocol.execute());

    // Set Base Node response to be not synced but in mempool
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: true,
        rejection_reason: TxSubmissionRejectionReason::None,
        is_synced: false,
    });

    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(5, Duration::from_secs(6))
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

    // Check that the appropriate events were emitted
    let delay = sleep(Duration::from_secs(5));
    tokio::pin!(delay);
    let mut broadcast = false;
    loop {
        tokio::select! {
            event = event_stream.recv() => {
                if let TransactionEvent::TransactionBroadcast(_) = &*event.unwrap() {
                   broadcast = true;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }

    assert!(broadcast, "Should have received a broadcast event");
}
/// Test submitting a transaction that is immediately rejected
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_broadcast_protocol_submit_rejection() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    let mut event_stream = resources.event_publisher.subscribe();

    add_transaction_to_database(1.into(), 1 * T, None, None, resources.db.clone()).await;
    let timeout_update_watch = Watch::new(Duration::from_secs(1));
    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let protocol = TransactionBroadcastProtocol::new(1.into(), resources.clone(), timeout_update_watch.get_receiver());

    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: false,
        rejection_reason: TxSubmissionRejectionReason::Orphan,
        is_synced: true,
    });

    let join_handle = task::spawn(protocol.execute());

    // Check that the protocol ends with rejection error
    if let Err(e) = join_handle.await.unwrap() {
        if let TransactionServiceError::MempoolRejectionOrphan = e.error {
        } else {
            panic!("Tx broadcast Should have failed with mempool rejection for being an orphan");
        }
    } else {
        panic!("Tx broadcast Should have failed");
    }

    // Check transaction is cancelled in db
    let db_completed_tx = resources.db.get_completed_transaction(1.into()).await;
    assert!(db_completed_tx.is_err());

    // Check that the appropriate events were emitted
    let delay = sleep(Duration::from_secs(1));
    tokio::pin!(delay);
    let mut cancelled = false;
    loop {
        tokio::select! {
            event = event_stream.recv() => {
                if let TransactionEvent::TransactionCancelled(..) = &*event.unwrap() {
                    cancelled = true;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }

    assert!(cancelled, "Should have cancelled transaction");
}

/// Test restarting a protocol which means the first step is a query not a submission, detecting the Tx is not in the
/// mempool, resubmit the tx and then have it mined
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_broadcast_protocol_restart_protocol_as_query() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;

    add_transaction_to_database(1.into(), 1 * T, None, None, resources.db.clone()).await;

    // Set Base Node query response to be not stored, as if the base node does not have the tx in its pool
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    let timeout_update_watch = Watch::new(Duration::from_secs(1));
    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());

    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let protocol = TransactionBroadcastProtocol::new(1.into(), resources.clone(), timeout_update_watch.get_receiver());
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

    // Should receive a resubmission call
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
        confirmations: resources.config.num_confirmations_required,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), TxId::from(1));

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1.into()).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::Broadcast);
}

/// This test will submit a Tx which will be accepted on submission but rejected on query, intially it will be done
/// slower than the resubmission window but then the resubmission window will be reduced so the transaction will be
/// reject twice within the window resulting in a cancelled transaction
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_broadcast_protocol_submit_success_followed_by_rejection() {
    let (
        mut resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    let mut event_stream = resources.event_publisher.subscribe();

    add_transaction_to_database(1.into(), 1 * T, None, None, resources.db.clone()).await;

    resources.config.transaction_mempool_resubmission_window = Duration::from_secs(3);
    resources.config.broadcast_monitoring_timeout = Duration::from_secs(60);

    let timeout_update_watch = Watch::new(Duration::from_secs(1));
    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());

    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let protocol = TransactionBroadcastProtocol::new(1.into(), resources.clone(), timeout_update_watch.get_receiver());

    let join_handle = task::spawn(protocol.execute());

    // Accepted in the mempool on submit but not query
    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 0,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Wait for 1 query
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(30))
        .await
        .unwrap();

    let _ = rpc_service_state
        .wait_pop_submit_transaction_calls(2, Duration::from_secs(30))
        .await
        .unwrap();

    // Check that the protocol ends with rejection error
    if let Err(e) = join_handle.await.unwrap() {
        if let TransactionServiceError::MempoolRejection = e.error {
        } else {
            panic!("Tx broadcast Should have failed with mempool rejection for being time locked");
        }
    } else {
        panic!("Tx broadcast Should have failed");
    }

    // Check transaction is cancelled in db
    let db_completed_tx = resources.db.get_completed_transaction(1.into()).await;
    assert!(db_completed_tx.is_err());

    // Check that the appropriate events were emitted
    let delay = sleep(Duration::from_secs(1));
    tokio::pin!(delay);
    let mut cancelled = false;
    loop {
        tokio::select! {
            event = event_stream.recv() => {
                if let TransactionEvent::TransactionCancelled(..) = &*event.unwrap() {
                cancelled = true;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }

    assert!(cancelled, "Should have cancelled transaction");
}

/// Submit a transaction that is Already Mined for the submission, should end up being completed as the validation will
/// deal with it
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_broadcast_protocol_submit_already_mined() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    add_transaction_to_database(1.into(), 1 * T, None, None, resources.db.clone()).await;

    // Set Base Node to respond with AlreadyMined
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: false,
        rejection_reason: TxSubmissionRejectionReason::AlreadyMined,
        is_synced: true,
    });

    let timeout_update_watch = Watch::new(Duration::from_secs(1));
    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let protocol = TransactionBroadcastProtocol::new(1.into(), resources.clone(), timeout_update_watch.get_receiver());

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
        confirmations: resources.config.num_confirmations_required,
        is_synced: true,
        height_of_longest_chain: 10,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), 1);

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1.into()).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::Completed);
}

/// A test to see that the broadcast protocol can handle a change to the base node address while it runs.
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_broadcast_protocol_submit_and_base_node_gets_changed() {
    let (
        mut resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;

    add_transaction_to_database(1.into(), 1 * T, None, None, resources.db.clone()).await;

    resources.config.broadcast_monitoring_timeout = Duration::from_secs(60);

    rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::NotStored,
        block_hash: None,
        confirmations: 1,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    let timeout_update_watch = Watch::new(Duration::from_secs(1));
    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let protocol = TransactionBroadcastProtocol::new(1.into(), resources.clone(), timeout_update_watch.get_receiver());

    let join_handle = task::spawn(protocol.execute());

    // Wait for 1 queries
    let _ = rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(205))
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

    let mut connection = new_mock_server
        .create_connection(new_server_node_identity.to_peer(), protocol_name.into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    // Set new Base Node response to be accepted
    new_rpc_service_state.set_transaction_query_response(TxQueryResponse {
        location: TxLocation::InMempool,
        block_hash: None,
        confirmations: resources.config.num_confirmations_required,
        is_synced: true,
        height_of_longest_chain: 0,
    });

    // Change Base Node
    wallet_connectivity.notify_base_node_set(new_server_node_identity.to_peer());

    // Wait for 1 query
    let _ = new_rpc_service_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(20))
        .await
        .unwrap();

    // Update old base node to reject the tx to check that the protocol is using the new base node
    rpc_service_state.set_submit_transaction_response(TxSubmissionResponse {
        accepted: false,
        rejection_reason: TxSubmissionRejectionReason::Orphan,
        is_synced: true,
    });

    // Check that the protocol ends with success
    let result = join_handle.await.unwrap();
    assert_eq!(result.unwrap(), TxId::from(1));

    // Check transaction status is updated
    let db_completed_tx = resources.db.get_completed_transaction(1.into()).await.unwrap();
    assert_eq!(db_completed_tx.status, TransactionStatus::Broadcast);
}

/// Test that validation detects transactions becoming mined unconfirmed and then confirmed with some going back to
/// completed
#[tokio::test]
#[allow(clippy::identity_op)]
#[ignore = "broken after validator node merge"]
async fn tx_validation_protocol_tx_becomes_mined_unconfirmed_then_confirmed() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);
    add_transaction_to_database(
        1.into(),
        1 * T,
        Some(TransactionStatus::Broadcast),
        None,
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        2.into(),
        2 * T,
        Some(TransactionStatus::Completed),
        None,
        resources.db.clone(),
    )
    .await;

    let tx2 = resources.db.get_completed_transaction(2.into()).await.unwrap();

    let transaction_query_batch_responses = vec![TxQueryBatchResponseProto {
        signature: Some(SignatureProto::from(
            tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
        )),
        location: TxLocationProto::from(TxLocation::Mined) as i32,
        block_hash: Some([1u8; 16].to_vec()),
        confirmations: 0,
        block_height: 1,
    }];

    let mut batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some([1u8; 16].to_vec()),
        height_of_longest_chain: 1,
    };

    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());

    let protocol = TransactionValidationProtocol::new(
        2.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();

    assert_eq!(
        completed_txs.get(&1.into()).unwrap().status,
        TransactionStatus::Broadcast
    );
    assert_eq!(
        completed_txs.get(&2.into()).unwrap().status,
        TransactionStatus::MinedUnconfirmed
    );

    // set Tx2 back to unmined
    batch_query_response.responses = vec![];
    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());

    let protocol = TransactionValidationProtocol::new(
        3.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();

    assert_eq!(
        completed_txs.get(&1.into()).unwrap().status,
        TransactionStatus::Broadcast
    );
    assert_eq!(
        completed_txs.get(&2.into()).unwrap().status,
        TransactionStatus::MinedUnconfirmed
    );

    // Now the tx will be fully mined
    let transaction_query_batch_responses = vec![TxQueryBatchResponseProto {
        signature: Some(SignatureProto::from(
            tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
        )),
        location: TxLocationProto::from(TxLocation::Mined) as i32,
        block_hash: Some([5u8; 16].to_vec()),
        confirmations: 4,
        block_height: 5,
    }];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some([5u8; 16].to_vec()),
        height_of_longest_chain: 5,
    };

    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());

    let protocol = TransactionValidationProtocol::new(
        4.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();

    assert_eq!(
        completed_txs.get(&2.into()).unwrap().status,
        TransactionStatus::MinedConfirmed
    );
    assert_eq!(completed_txs.get(&2.into()).unwrap().confirmations.unwrap(), 4);
}

/// Test that revalidation clears the correct db fields and calls for validation of is said transactions
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_revalidation() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);
    add_transaction_to_database(
        1.into(),
        1 * T,
        Some(TransactionStatus::Completed),
        None,
        resources.db.clone(),
    )
    .await;
    add_transaction_to_database(
        2.into(),
        2 * T,
        Some(TransactionStatus::Completed),
        None,
        resources.db.clone(),
    )
    .await;

    let tx2 = resources.db.get_completed_transaction(2.into()).await.unwrap();

    // set tx2 as fully mined
    let transaction_query_batch_responses = vec![TxQueryBatchResponseProto {
        signature: Some(SignatureProto::from(
            tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
        )),
        location: TxLocationProto::from(TxLocation::Mined) as i32,
        block_hash: Some([5u8; 16].to_vec()),
        confirmations: 4,
        block_height: 5,
    }];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some([5u8; 16].to_vec()),
        height_of_longest_chain: 5,
    };

    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());

    let protocol = TransactionValidationProtocol::new(
        4.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();

    assert_eq!(
        completed_txs.get(&2.into()).unwrap().status,
        TransactionStatus::MinedConfirmed
    );
    assert_eq!(completed_txs.get(&2.into()).unwrap().confirmations.unwrap(), 4);

    let transaction_query_batch_responses = vec![TxQueryBatchResponseProto {
        signature: Some(SignatureProto::from(
            tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
        )),
        location: TxLocationProto::from(TxLocation::Mined) as i32,
        block_hash: Some([5u8; 16].to_vec()),
        confirmations: 8,
        block_height: 10,
    }];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some([5u8; 16].to_vec()),
        height_of_longest_chain: 10,
    };

    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());
    // revalidate sets all to unvalidated, so lets check that thay are
    resources.db.mark_all_transactions_as_unvalidated().await.unwrap();
    let completed_txs = resources.db.get_completed_transactions().await.unwrap();
    assert_eq!(
        completed_txs.get(&2.into()).unwrap().status,
        TransactionStatus::MinedConfirmed
    );
    assert_eq!(completed_txs.get(&2.into()).unwrap().mined_height, None);
    assert_eq!(completed_txs.get(&2.into()).unwrap().mined_in_block, None);

    let protocol = TransactionValidationProtocol::new(
        5.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();
    // data should now be updated and changed
    assert_eq!(
        completed_txs.get(&2.into()).unwrap().status,
        TransactionStatus::MinedConfirmed
    );
    assert_eq!(completed_txs.get(&2.into()).unwrap().confirmations.unwrap(), 8);
}

/// Test that validation detects transactions becoming mined unconfirmed and then confirmed with some going back to
/// completed
#[tokio::test]
#[allow(clippy::identity_op)]
async fn tx_validation_protocol_reorg() {
    let (
        resources,
        _outbound_mock_state,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _shutdown,
        _temp_dir,
        _transaction_event_receiver,
        wallet_connectivity,
    ) = setup().await;
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    for i in 1..=5 {
        add_transaction_to_database(
            i.into(),
            i * T,
            Some(TransactionStatus::Broadcast),
            None,
            resources.db.clone(),
        )
        .await;
    }

    add_transaction_to_database(
        6.into(),
        6 * T,
        Some(TransactionStatus::Coinbase),
        Some(8),
        resources.db.clone(),
    )
    .await;

    add_transaction_to_database(
        7.into(),
        7 * T,
        Some(TransactionStatus::Coinbase),
        Some(9),
        resources.db.clone(),
    )
    .await;

    let mut block_headers = HashMap::new();
    for i in 0..=10 {
        let mut block_header = BlockHeader::new(1);
        block_header.height = i;
        block_headers.insert(i, block_header.clone());
    }
    rpc_service_state.set_blocks(block_headers.clone());

    let tx1 = resources.db.get_completed_transaction(1.into()).await.unwrap();
    let tx2 = resources.db.get_completed_transaction(2.into()).await.unwrap();
    let tx3 = resources.db.get_completed_transaction(3.into()).await.unwrap();
    let tx4 = resources.db.get_completed_transaction(4.into()).await.unwrap();
    let tx5 = resources.db.get_completed_transaction(5.into()).await.unwrap();
    let coinbase_tx1 = resources.db.get_completed_transaction(6.into()).await.unwrap();
    let coinbase_tx2 = resources.db.get_completed_transaction(7.into()).await.unwrap();

    let transaction_query_batch_responses = vec![
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx1.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&5).unwrap().hash()),
            confirmations: 5,
            block_height: 5,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&6).unwrap().hash()),
            confirmations: 4,
            block_height: 6,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx3.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&7).unwrap().hash()),
            confirmations: 3,
            block_height: 7,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx4.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&8).unwrap().hash()),
            confirmations: 2,
            block_height: 8,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                coinbase_tx1.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&8).unwrap().hash()),
            confirmations: 2,
            block_height: 8,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx5.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&9).unwrap().hash()),
            confirmations: 1,
            block_height: 9,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                coinbase_tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&9).unwrap().hash()),
            confirmations: 1,
            block_height: 9,
        },
    ];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some(block_headers.get(&10).unwrap().hash()),
        height_of_longest_chain: 10,
    };

    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());

    let protocol = TransactionValidationProtocol::new(
        1.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();
    let mut unconfirmed_count = 0;
    let mut confirmed_count = 0;
    for (_k, tx) in completed_txs.iter() {
        if tx.status == TransactionStatus::MinedUnconfirmed {
            unconfirmed_count += 1;
        }
        if tx.status == TransactionStatus::MinedConfirmed {
            confirmed_count += 1;
        }
    }
    assert_eq!(confirmed_count, 3);
    assert_eq!(unconfirmed_count, 4);

    // Now we will reorg to new blocks 8 and 9, tx 4 will disappear and tx5 will appear in block 9, coinbase_tx2 should
    // become invalid and coinbase_tx1 should return to coinbase status

    let _ = block_headers.remove(&9);
    let _ = block_headers.remove(&10);
    let mut block_header = BlockHeader::new(2);
    block_header.height = 8;
    block_headers.insert(8, block_header.clone());

    rpc_service_state.set_blocks(block_headers.clone());

    let transaction_query_batch_responses = vec![
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx1.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&5).unwrap().hash()),
            confirmations: 4,
            block_height: 5,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&6).unwrap().hash()),
            confirmations: 3,
            block_height: 6,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx3.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&7).unwrap().hash()),
            confirmations: 2,
            block_height: 7,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                coinbase_tx1.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx5.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&8).unwrap().hash()),
            confirmations: 1,
            block_height: 8,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                coinbase_tx2.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
    ];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some(block_headers.get(&8).unwrap().hash()),
        height_of_longest_chain: 8,
    };

    rpc_service_state.set_transaction_query_batch_responses(batch_query_response.clone());
    let _ = rpc_service_state.take_get_header_by_height_calls();

    let protocol = TransactionValidationProtocol::new(
        2.into(),
        resources.db.clone(),
        wallet_connectivity.clone(),
        resources.config.clone(),
        resources.event_publisher.clone(),
        resources.output_manager_service.clone(),
    );

    let join_handle = task::spawn(protocol.execute());
    let result = join_handle.await.unwrap();
    assert!(result.is_ok());

    let _calls = rpc_service_state
        .wait_pop_get_header_by_height_calls(5, Duration::from_secs(30))
        .await
        .unwrap();

    assert_eq!(rpc_service_state.take_get_header_by_height_calls().len(), 0);

    let completed_txs = resources.db.get_completed_transactions().await.unwrap();
    assert_eq!(
        completed_txs.get(&4.into()).unwrap().status,
        TransactionStatus::Completed
    );
    assert_eq!(
        completed_txs.get(&5.into()).unwrap().status,
        TransactionStatus::MinedUnconfirmed
    );
    assert_eq!(completed_txs.get(&5.into()).cloned().unwrap().mined_height.unwrap(), 8);
    assert_eq!(completed_txs.get(&5.into()).cloned().unwrap().confirmations.unwrap(), 1);
    assert_eq!(
        completed_txs.get(&7.into()).unwrap().status,
        TransactionStatus::Coinbase
    );
    let cancelled_completed_txs = resources.db.get_cancelled_completed_transactions().await.unwrap();

    assert!(matches!(
        cancelled_completed_txs.get(&6.into()).unwrap().cancelled,
        Some(TxCancellationReason::AbandonedCoinbase)
    ));
}
