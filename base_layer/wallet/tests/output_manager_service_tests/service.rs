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
use std::{collections::HashMap, sync::Arc, time::Duration};

use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    transaction::TxId,
    types::{PrivateKey, PublicKey},
};
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
    test_utils::node_identity::build_node_identity,
};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcServer,
    blocks::BlockHeader,
    consensus::{ConsensusConstantsBuilder, ConsensusEncodingSized},
    covenants::Covenant,
    proto::base_node::{QueryDeletedResponse, UtxoQueryResponse, UtxoQueryResponses},
    transactions::{
        fee::Fee,
        tari_amount::{uT, MicroTari},
        test_helpers::{create_unblinded_output, TestParams as TestParamsHelpers},
        transaction_components::{OutputFeatures, OutputFlags},
        transaction_protocol::sender::TransactionSenderMessage,
        weight::TransactionWeight,
        CryptoFactories,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    common::Blake256,
    inputs,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    script,
    script::TariScript,
};
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::Mnemonic};
use tari_p2p::Network;
use tari_service_framework::reply_channel;
use tari_shutdown::Shutdown;
use tari_utilities::Hashable;
use tari_wallet::{
    base_node_service::{
        handle::{BaseNodeEvent, BaseNodeServiceHandle},
        mock_base_node_service::MockBaseNodeService,
        service::BaseNodeState,
    },
    connectivity_service::{create_wallet_connectivity_mock, WalletConnectivityMock},
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerStorageError},
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::OutputManagerService,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::SpendingPriority,
            sqlite_db::OutputManagerSqliteDatabase,
            OutputStatus,
        },
    },
    test_utils::create_consensus_constants,
    transaction_service::handle::TransactionServiceHandle,
};
use tokio::{
    sync::{broadcast, broadcast::channel},
    task,
    time::sleep,
};

use crate::support::{
    comms_rpc::{connect_rpc_client, BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    data::get_temp_sqlite_database_connection,
    utils::{make_input, make_input_with_features, TestParams},
};

fn default_metadata_byte_size() -> usize {
    TransactionWeight::latest().round_up_metadata_size(
        OutputFeatures::default().consensus_encode_exact_size() + script![Nop].consensus_encode_exact_size(),
    )
}

#[allow(clippy::type_complexity)]
async fn setup_output_manager_service<T: OutputManagerBackend + 'static>(
    backend: T,
    with_connection: bool,
) -> (
    OutputManagerHandle,
    WalletConnectivityMock,
    Shutdown,
    TransactionServiceHandle,
    MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    Arc<NodeIdentity>,
    BaseNodeWalletRpcMockState,
    broadcast::Sender<Arc<BaseNodeEvent>>,
) {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let (oms_event_publisher, _) = broadcast::channel(200);

    let (ts_request_sender, _ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher);

    let constants = create_consensus_constants(0);

    let (sender, receiver_bns) = reply_channel::unbounded();
    let (event_publisher_bns, _) = broadcast::channel(100);
    let basenode_service_handle = BaseNodeServiceHandle::new(sender, event_publisher_bns.clone());
    let mut mock_base_node_service = MockBaseNodeService::new(receiver_bns, shutdown.to_signal());
    mock_base_node_service.set_default_base_node_state();
    task::spawn(mock_base_node_service.run());

    let wallet_connectivity_mock = create_wallet_connectivity_mock();
    // let (connectivity, connectivity_mock) = create_connectivity_mock();
    // let connectivity_mock_state = connectivity_mock.get_shared_state();
    // task::spawn(connectivity_mock.run());

    let service = BaseNodeWalletRpcMockService::new();
    let rpc_service_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let mut mock_server = MockRpcServer::new(server, server_node_identity.clone());
    mock_server.serve();

    if with_connection {
        let mut connection = mock_server
            .create_connection(server_node_identity.to_peer(), protocol_name.into())
            .await;

        wallet_connectivity_mock.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);
    }

    let cipher_seed = CipherSeed::from_mnemonic(
        &[
            "parade".to_string(),
            "genius".to_string(),
            "cradle".to_string(),
            "milk".to_string(),
            "perfect".to_string(),
            "ride".to_string(),
            "online".to_string(),
            "world".to_string(),
            "lady".to_string(),
            "apple".to_string(),
            "rent".to_string(),
            "business".to_string(),
            "oppose".to_string(),
            "force".to_string(),
            "tumble".to_string(),
            "escape".to_string(),
            "tongue".to_string(),
            "camera".to_string(),
            "ceiling".to_string(),
            "edge".to_string(),
            "shine".to_string(),
            "gauge".to_string(),
            "fossil".to_string(),
            "orphan".to_string(),
        ],
        None,
    )
    .unwrap();

    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig {
            base_node_query_timeout: Duration::from_secs(10),
            max_utxo_query_size: 2,
            peer_dial_retry_timeout: Duration::from_secs(5),
            ..Default::default()
        },
        oms_request_receiver,
        OutputManagerDatabase::new(backend),
        oms_event_publisher.clone(),
        factories,
        constants,
        shutdown.to_signal(),
        basenode_service_handle,
        wallet_connectivity_mock.clone(),
        cipher_seed,
        server_node_identity.clone(),
    )
    .await
    .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    task::spawn(async move { output_manager_service.start().await.unwrap() });

    (
        output_manager_service_handle,
        wallet_connectivity_mock,
        shutdown,
        ts_handle,
        mock_server,
        server_node_identity,
        rpc_service_state,
        event_publisher_bns,
    )
}

pub async fn setup_oms_with_bn_state<T: OutputManagerBackend + 'static>(
    backend: T,
    height: Option<u64>,
    node_identity: Arc<NodeIdentity>,
) -> (
    OutputManagerHandle,
    Shutdown,
    TransactionServiceHandle,
    BaseNodeServiceHandle,
    broadcast::Sender<Arc<BaseNodeEvent>>,
) {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let (oms_event_publisher, _) = broadcast::channel(200);

    let (ts_request_sender, _ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher);

    let constants = create_consensus_constants(0);

    let (sender, receiver_bns) = reply_channel::unbounded();
    let (event_publisher_bns, _) = broadcast::channel(100);

    let base_node_service_handle = BaseNodeServiceHandle::new(sender, event_publisher_bns.clone());
    let mut mock_base_node_service = MockBaseNodeService::new(receiver_bns, shutdown.to_signal());
    mock_base_node_service.set_base_node_state(height);
    task::spawn(mock_base_node_service.run());
    let connectivity = create_wallet_connectivity_mock();

    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig {
            base_node_query_timeout: Duration::from_secs(10),
            max_utxo_query_size: 2,
            peer_dial_retry_timeout: Duration::from_secs(5),
            ..Default::default()
        },
        oms_request_receiver,
        OutputManagerDatabase::new(backend),
        oms_event_publisher.clone(),
        factories,
        constants,
        shutdown.to_signal(),
        base_node_service_handle.clone(),
        connectivity,
        CipherSeed::new(),
        node_identity,
    )
    .await
    .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    task::spawn(async move { output_manager_service.start().await.unwrap() });

    (
        output_manager_service_handle,
        shutdown,
        ts_handle,
        base_node_service_handle,
        event_publisher_bns,
    )
}

fn generate_sender_transaction_message(amount: MicroTari) -> (TxId, TransactionSenderMessage) {
    let factories = CryptoFactories::default();

    let alice = TestParams::new(&mut OsRng);

    let (utxo, input) = make_input(&mut OsRng, 2 * amount, &factories.commitment);
    let mut builder = SenderTransactionProtocol::builder(1, create_consensus_constants(0));
    let script_private_key = PrivateKey::random(&mut OsRng);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari(20))
        .with_offset(alice.offset.clone())
        .with_private_nonce(alice.nonce.clone())
        .with_change_secret(alice.change_spend_key)
        .with_input(utxo, input)
        .with_amount(0, amount)
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            OutputFeatures::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(
            script!(Nop),
            inputs!(PublicKey::from_secret_key(&script_private_key)),
            script_private_key,
        );

    let mut stp = builder.build::<Blake256>(&factories, None, u64::MAX).unwrap();
    let tx_id = stp.get_tx_id().unwrap();
    (
        tx_id,
        TransactionSenderMessage::new_single_round_message(stp.build_single_round_message().unwrap()),
    )
}

#[tokio::test]
async fn fee_estimate() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let factories = CryptoFactories::default();
    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let (_, uo) = make_input(&mut OsRng.clone(), MicroTari::from(3000), &factories.commitment);
    oms.add_output(uo, None).await.unwrap();
    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight());
    // minimum fpg
    let fee_per_gram = MicroTari::from(1);
    let fee = oms
        .fee_estimate(MicroTari::from(100), fee_per_gram, 1, 1)
        .await
        .unwrap();
    assert_eq!(
        fee,
        fee_calc.calculate(fee_per_gram, 1, 1, 2, 2 * default_metadata_byte_size())
    );

    let fee_per_gram = MicroTari::from(5);
    for outputs in 1..5 {
        let fee = oms
            .fee_estimate(MicroTari::from(100), fee_per_gram, 1, outputs)
            .await
            .unwrap();

        assert_eq!(
            fee,
            fee_calc.calculate(
                fee_per_gram,
                1,
                1,
                outputs + 1,
                default_metadata_byte_size() * (outputs + 1)
            )
        );
    }

    // not enough funds
    let err = oms
        .fee_estimate(MicroTari::from(2750), fee_per_gram, 1, 1)
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));
}

#[allow(clippy::identity_op)]
#[tokio::test]
async fn test_utxo_selection_no_chain_metadata() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    // no chain metadata
    let (mut oms, _shutdown, _, _, _) = setup_oms_with_bn_state(
        OutputManagerSqliteDatabase::new(connection, None),
        None,
        server_node_identity,
    )
    .await;

    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight());
    // no utxos - not enough funds
    let amount = MicroTari::from(1000);
    let fee_per_gram = MicroTari::from(2);
    let err = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // create 10 utxos with maturity at heights from 1 to 10
    for i in 1..=10 {
        let (_, uo) = make_input_with_features(
            &mut OsRng.clone(),
            i * amount,
            &factories.commitment,
            Some(OutputFeatures::with_maturity(i)),
        );
        oms.add_output(uo.clone(), None).await.unwrap();
    }

    // but we have no chain state so the lowest maturity should be used
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that lowest 2 maturities were encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 8);
    for (index, utxo) in utxos.iter().enumerate() {
        let i = index as u64 + 3;
        assert_eq!(utxo.features.maturity, i);
        assert_eq!(utxo.value, i * amount);
    }

    // test that we can get a fee estimate with no chain metadata
    let fee = oms.fee_estimate(amount, fee_per_gram, 1, 2).await.unwrap();
    let expected_fee = fee_calc.calculate(fee_per_gram, 1, 1, 3, default_metadata_byte_size() * 3);
    assert_eq!(fee, expected_fee);

    // test if a fee estimate would be possible with pending funds included
    // at this point 52000 uT is still spendable, with pending change incoming of 1690 uT
    // so instead of returning "not enough funds", return "funds pending"
    let spendable_amount = (3..=10).sum::<u64>() * amount;
    let err = oms
        .fee_estimate(spendable_amount, fee_per_gram, 1, 2)
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::FundsPending));

    // test not enough funds
    let broke_amount = spendable_amount + MicroTari::from(2000);
    let err = oms.fee_estimate(broke_amount, fee_per_gram, 1, 2).await.unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // coin split uses the "Largest" selection strategy
    let (_, tx, utxos_total_value) = oms.create_coin_split(amount, 5, fee_per_gram, None).await.unwrap();
    let expected_fee = fee_calc.calculate(fee_per_gram, 1, 1, 6, default_metadata_byte_size() * 6);
    assert_eq!(tx.body.get_total_fee(), expected_fee);
    assert_eq!(utxos_total_value, MicroTari::from(10_000));

    // test that largest utxo was encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 7);
    for (index, utxo) in utxos.iter().enumerate() {
        let i = index as u64 + 3;
        assert_eq!(utxo.features.maturity, i);
        assert_eq!(utxo.value, i * amount);
    }
}

#[allow(clippy::identity_op)]
#[tokio::test]
async fn test_utxo_selection_with_chain_metadata() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    // setup with chain metadata at a height of 6
    let (mut oms, _shutdown, _, _, _) = setup_oms_with_bn_state(
        OutputManagerSqliteDatabase::new(connection, None),
        Some(6),
        server_node_identity,
    )
    .await;
    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight());

    // no utxos - not enough funds
    let amount = MicroTari::from(1000);
    let fee_per_gram = MicroTari::from(2);
    let err = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // create 10 utxos with maturity at heights from 1 to 10
    for i in 1..=10 {
        let (_, uo) = make_input_with_features(
            &mut OsRng.clone(),
            i * amount,
            &factories.commitment,
            Some(OutputFeatures::with_maturity(i)),
        );
        oms.add_output(uo.clone(), None).await.unwrap();
    }

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 10);

    // test fee estimates
    let fee = oms.fee_estimate(amount, fee_per_gram, 1, 2).await.unwrap();
    let expected_fee = fee_calc.calculate(fee_per_gram, 1, 2, 3, default_metadata_byte_size() * 3);
    assert_eq!(fee, expected_fee);

    // test fee estimates are maturity aware
    // even though we have utxos for the fee, they can't be spent because they are not mature yet
    let spendable_amount = (1..=6).sum::<u64>() * amount;
    let err = oms
        .fee_estimate(spendable_amount, fee_per_gram, 1, 2)
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // test coin split is maturity aware
    let (_, tx, utxos_total_value) = oms.create_coin_split(amount, 5, fee_per_gram, None).await.unwrap();
    assert_eq!(utxos_total_value, MicroTari::from(6_000));
    let expected_fee = fee_calc.calculate(fee_per_gram, 1, 1, 6, default_metadata_byte_size() * 6);
    assert_eq!(tx.body.get_total_fee(), expected_fee);

    // test that largest spendable utxo was encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 9);
    let found = utxos.iter().any(|u| u.value == 6 * amount);
    assert!(!found, "An unspendable utxo was selected");

    // test transactions
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that utxos with the lowest 2 maturities were encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 7);
    for utxo in utxos.iter() {
        assert_ne!(utxo.features.maturity, 1);
        assert_ne!(utxo.value, amount);
        assert_ne!(utxo.features.maturity, 2);
        assert_ne!(utxo.value, 2 * amount);
    }

    // when the amount is greater than the largest utxo, then "Largest" selection strategy is used
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            6 * amount,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that utxos with the highest spendable 2 maturities were encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 5);
    for utxo in utxos.iter() {
        assert_ne!(utxo.features.maturity, 4);
        assert_ne!(utxo.value, 4 * amount);
        assert_ne!(utxo.features.maturity, 5);
        assert_ne!(utxo.value, 5 * amount);
    }
}

#[tokio::test]
async fn test_utxo_selection_with_tx_priority() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    // setup with chain metadata at a height of 6
    let (mut oms, _shutdown, _, _, _) = setup_oms_with_bn_state(
        OutputManagerSqliteDatabase::new(connection, None),
        Some(6),
        server_node_identity,
    )
    .await;

    let amount = MicroTari::from(2000);
    let fee_per_gram = MicroTari::from(2);

    // we create two outputs, one as coinbase-high priority one as normal so we can track them
    let (_, uo) = make_input_with_features(
        &mut OsRng.clone(),
        amount,
        &factories.commitment,
        Some(OutputFeatures::create_coinbase(1)),
    );
    oms.add_output(uo, Some(SpendingPriority::HtlcSpendAsap)).await.unwrap();
    let (_, uo) = make_input_with_features(
        &mut OsRng.clone(),
        amount,
        &factories.commitment,
        Some(OutputFeatures::with_maturity(1)),
    );
    oms.add_output(uo, None).await.unwrap();

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 2);

    // test transactions
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroTari::from(1000),
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that the utxo with the lowest priority was left
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 1);

    assert!(!utxos[0].features.flags.contains(OutputFlags::COINBASE_OUTPUT));
}

#[tokio::test]
async fn send_not_enough_funds() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;
    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(200 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        oms.add_output(uo, None).await.unwrap();
    }

    match oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroTari::from(num_outputs * 2000),
            None,
            None,
            MicroTari::from(4),
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
    {
        Err(OutputManagerError::NotEnoughFunds) => {},
        _ => panic!(),
    }
}

#[tokio::test]
async fn send_no_change() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let fee_per_gram = MicroTari::from(4);
    let constants = create_consensus_constants(0);
    let fee_without_change =
        Fee::new(*constants.transaction_weight()).calculate(fee_per_gram, 1, 2, 1, default_metadata_byte_size());
    let value1 = 5000;
    oms.add_output(
        create_unblinded_output(
            script!(Nop),
            OutputFeatures::default(),
            TestParamsHelpers::new(),
            MicroTari::from(value1),
        ),
        None,
    )
    .await
    .unwrap();
    let value2 = 8000;
    oms.add_output(
        create_unblinded_output(
            script!(Nop),
            OutputFeatures::default(),
            TestParamsHelpers::new(),
            MicroTari::from(value2),
        ),
        None,
    )
    .await
    .unwrap();

    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroTari::from(value1 + value2) - fee_without_change,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();

    assert_eq!(stp.get_amount_to_self().unwrap(), MicroTari::from(0));
    assert_eq!(
        oms.get_balance().await.unwrap().pending_incoming_balance,
        MicroTari::from(0)
    );
}
#[tokio::test]
async fn send_not_enough_for_change() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let fee_per_gram = MicroTari::from(4);
    let constants = create_consensus_constants(0);
    let fee_without_change = Fee::new(*constants.transaction_weight()).calculate(fee_per_gram, 1, 2, 1, 0);
    let value1 = MicroTari(500);
    oms.add_output(
        create_unblinded_output(
            TariScript::default(),
            OutputFeatures::default(),
            TestParamsHelpers::new(),
            value1,
        ),
        None,
    )
    .await
    .unwrap();
    let value2 = MicroTari(800);
    oms.add_output(
        create_unblinded_output(
            TariScript::default(),
            OutputFeatures::default(),
            TestParamsHelpers::new(),
            value2,
        ),
        None,
    )
    .await
    .unwrap();

    match oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            value1 + value2 + uT - fee_without_change,
            None,
            None,
            fee_per_gram,
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
    {
        Err(OutputManagerError::NotEnoughFunds) => {},
        _ => panic!(),
    }
}

#[tokio::test]
async fn cancel_transaction() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        oms.add_output(uo, None).await.unwrap();
    }
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroTari::from(1000),
            None,
            None,
            MicroTari::from(4),
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();

    match oms.cancel_transaction(1.into()).await {
        Err(OutputManagerError::OutputManagerStorageError(OutputManagerStorageError::ValueNotFound)) => {},
        _ => panic!("Value should not exist"),
    }

    oms.cancel_transaction(stp.get_tx_id().unwrap()).await.unwrap();

    assert_eq!(oms.get_unspent_outputs().await.unwrap().len(), num_outputs);
}

#[tokio::test]
#[ignore = "to be fixed"]
async fn cancel_transaction_and_reinstate_inbound_tx() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend.clone(), true).await;

    let value = MicroTari::from(5000);
    let (tx_id, sender_message) = generate_sender_transaction_message(value);
    let _rtp = oms.get_recipient_transaction(sender_message).await.unwrap();
    assert_eq!(oms.get_unspent_outputs().await.unwrap().len(), 0);

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(balance.pending_incoming_balance, value);

    oms.cancel_transaction(tx_id).await.unwrap();

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(balance.pending_incoming_balance, MicroTari::from(0));

    oms.reinstate_cancelled_inbound_transaction_outputs(tx_id)
        .await
        .unwrap();

    let balance = oms.get_balance().await.unwrap();

    assert_eq!(balance.pending_incoming_balance, value);
}

#[tokio::test]
async fn test_get_balance() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let balance = oms.get_balance().await.unwrap();

    assert_eq!(MicroTari::from(0), balance.available_balance);

    let mut total = MicroTari::from(0);
    let output_val = MicroTari::from(2000);
    let (_ti, uo) = make_input(&mut OsRng.clone(), output_val, &factories.commitment);
    total += uo.value;
    oms.add_output(uo, None).await.unwrap();

    let (_ti, uo) = make_input(&mut OsRng.clone(), output_val, &factories.commitment);
    total += uo.value;
    oms.add_output(uo, None).await.unwrap();

    let send_value = MicroTari::from(1000);
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            send_value,
            None,
            None,
            MicroTari::from(4),
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();

    let change_val = stp.get_change_amount().unwrap();

    let recv_value = MicroTari::from(1500);
    let (_tx_id, sender_message) = generate_sender_transaction_message(recv_value);
    let _rtp = oms.get_recipient_transaction(sender_message).await.unwrap();

    let balance = oms.get_balance().await.unwrap();

    assert_eq!(output_val, balance.available_balance);
    assert_eq!(MicroTari::from(0), balance.time_locked_balance.unwrap());
    assert_eq!(recv_value + change_val, balance.pending_incoming_balance);
    assert_eq!(output_val, balance.pending_outgoing_balance);
}

#[tokio::test]
async fn sending_transaction_with_short_term_clear() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend.clone(), true).await;

    let available_balance = 10_000 * uT;
    let (_ti, uo) = make_input(&mut OsRng.clone(), available_balance, &factories.commitment);
    oms.add_output(uo, None).await.unwrap();

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance);
    assert_eq!(balance.time_locked_balance.unwrap(), MicroTari::from(0));

    // Check that funds are encumbered and then unencumbered if the pending tx is not confirmed before restart
    let _stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroTari::from(1000),
            None,
            None,
            MicroTari::from(4),
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, MicroTari::from(0));
    assert_eq!(balance.time_locked_balance.unwrap(), MicroTari::from(0));
    assert_eq!(balance.pending_outgoing_balance, available_balance);

    drop(oms);
    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend.clone(), true).await;

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance);
    assert_eq!(balance.time_locked_balance.unwrap(), MicroTari::from(0));

    // Check that is the pending tx is confirmed that the encumberance persists after restart
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroTari::from(1000),
            None,
            None,
            MicroTari::from(4),
            None,
            "".to_string(),
            script!(Nop),
            Covenant::default(),
        )
        .await
        .unwrap();
    let sender_tx_id = stp.get_tx_id().unwrap();
    oms.confirm_pending_transaction(sender_tx_id).await.unwrap();

    drop(oms);
    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, MicroTari::from(0));
    assert_eq!(balance.time_locked_balance.unwrap(), MicroTari::from(0));
    assert_eq!(balance.pending_outgoing_balance, available_balance);
}

#[tokio::test]
async fn coin_split_with_change() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let val1 = 6_000 * uT;
    let val2 = 7_000 * uT;
    let val3 = 8_000 * uT;
    let (_ti, uo1) = make_input(&mut OsRng, val1, &factories.commitment);
    let (_ti, uo2) = make_input(&mut OsRng, val2, &factories.commitment);
    let (_ti, uo3) = make_input(&mut OsRng, val3, &factories.commitment);
    assert!(oms.add_output(uo1, None).await.is_ok());
    assert!(oms.add_output(uo2, None).await.is_ok());
    assert!(oms.add_output(uo3, None).await.is_ok());

    let fee_per_gram = MicroTari::from(5);
    let split_count = 8;
    let (_tx_id, coin_split_tx, amount) = oms
        .create_coin_split(1000.into(), split_count, fee_per_gram, None)
        .await
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 2);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count + 1);
    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight());
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        2,
        split_count + 1,
        (split_count + 1) * default_metadata_byte_size(),
    );
    assert_eq!(coin_split_tx.body.get_total_fee(), expected_fee);
    assert_eq!(amount, val2 + val3);
}

#[tokio::test]
async fn coin_split_no_change() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let fee_per_gram = MicroTari::from(4);
    let split_count = 15;
    let constants = create_consensus_constants(0);
    let fee_calc = Fee::new(*constants.transaction_weight());
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        3,
        split_count,
        split_count * default_metadata_byte_size(),
    );
    let val1 = 4_000 * uT;
    let val2 = 5_000 * uT;
    let val3 = 6_000 * uT + expected_fee;
    let (_ti, uo1) = make_input(&mut OsRng, val1, &factories.commitment);
    let (_ti, uo2) = make_input(&mut OsRng, val2, &factories.commitment);
    let (_ti, uo3) = make_input(&mut OsRng, val3, &factories.commitment);
    assert!(oms.add_output(uo1, None).await.is_ok());
    assert!(oms.add_output(uo2, None).await.is_ok());
    assert!(oms.add_output(uo3, None).await.is_ok());

    let (_tx_id, coin_split_tx, amount) = oms
        .create_coin_split(1000.into(), split_count, fee_per_gram, None)
        .await
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 3);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count);
    assert_eq!(coin_split_tx.body.get_total_fee(), expected_fee);
    assert_eq!(amount, val1 + val2 + val3);
}

#[tokio::test]
async fn handle_coinbase() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let reward1 = MicroTari::from(1000);
    let fees1 = MicroTari::from(500);
    let value1 = reward1 + fees1;
    let reward2 = MicroTari::from(2000);
    let fees2 = MicroTari::from(500);
    let value2 = reward2 + fees2;
    let reward3 = MicroTari::from(3000);
    let fees3 = MicroTari::from(500);
    let value3 = reward3 + fees3;

    let _ = oms
        .get_coinbase_transaction(1u64.into(), reward1, fees1, 1)
        .await
        .unwrap();
    assert_eq!(oms.get_unspent_outputs().await.unwrap().len(), 0);
    assert_eq!(oms.get_balance().await.unwrap().pending_incoming_balance, value1);
    let _tx2 = oms
        .get_coinbase_transaction(2u64.into(), reward2, fees2, 1)
        .await
        .unwrap();
    assert_eq!(oms.get_unspent_outputs().await.unwrap().len(), 0);
    assert_eq!(oms.get_balance().await.unwrap().pending_incoming_balance, value2);
    let tx3 = oms
        .get_coinbase_transaction(3u64.into(), reward3, fees3, 2)
        .await
        .unwrap();
    assert_eq!(oms.get_unspent_outputs().await.unwrap().len(), 0);
    assert_eq!(
        oms.get_balance().await.unwrap().pending_incoming_balance,
        value2 + value3
    );

    let output = tx3.body.outputs()[0].clone();

    let rewind_public_keys = oms.get_rewind_public_keys().await.unwrap();
    let rewind_result = output
        .rewind_range_proof_value_only(
            &factories.range_proof,
            &rewind_public_keys.rewind_public_key,
            &rewind_public_keys.rewind_blinding_public_key,
        )
        .unwrap();
    assert_eq!(rewind_result.committed_value, value3);
}

#[tokio::test]
async fn test_txo_validation() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let oms_db = backend.clone();

    let (
        mut oms,
        wallet_connectivity,
        _shutdown,
        _ts,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        base_node_service_event_publisher,
    ) = setup_output_manager_service(backend, true).await;

    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let output1_value = 1_000_000;
    let output1 = create_unblinded_output(
        script!(Nop),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(output1_value),
    );
    let output1_tx_output = output1.as_transaction_output(&factories).unwrap();
    oms.add_output_with_tx_id(1.into(), output1.clone(), None)
        .await
        .unwrap();

    let output2_value = 2_000_000;
    let output2 = create_unblinded_output(
        script!(Nop),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(output2_value),
    );
    let output2_tx_output = output2.as_transaction_output(&factories).unwrap();

    oms.add_output_with_tx_id(2.into(), output2.clone(), None)
        .await
        .unwrap();

    let output3_value = 4_000_000;
    let output3 = create_unblinded_output(
        script!(Nop),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(output3_value),
    );

    oms.add_output_with_tx_id(3.into(), output3.clone(), None)
        .await
        .unwrap();

    let mut block1_header = BlockHeader::new(1);
    block1_header.height = 1;
    let mut block4_header = BlockHeader::new(1);
    block4_header.height = 4;

    let mut block_headers = HashMap::new();
    block_headers.insert(1, block1_header.clone());
    block_headers.insert(4, block4_header.clone());
    rpc_service_state.set_blocks(block_headers.clone());

    // These responses will mark outputs 1 and 2 and mined confirmed
    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().into()),
            mmr_position: 1,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output1_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().into()),
            mmr_position: 2,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output2_tx_output.hash(),
        },
    ];

    let utxo_query_responses = UtxoQueryResponses {
        best_block: block4_header.hash(),
        height_of_longest_chain: 4,
        responses,
    };

    rpc_service_state.set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent in the transaction that produced output4
    let query_deleted_response = QueryDeletedResponse {
        best_block: block4_header.hash(),
        height_of_longest_chain: 4,
        deleted_positions: vec![],
        not_deleted_positions: vec![1, 2],
        heights_deleted_at: vec![],
        blocks_deleted_in: vec![],
    };

    rpc_service_state.set_query_deleted_response(query_deleted_response.clone());
    oms.validate_txos().await.unwrap();
    let _utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    oms.prepare_transaction_to_send(
        4.into(),
        MicroTari::from(900_000),
        None,
        None,
        MicroTari::from(10),
        None,
        "".to_string(),
        script!(Nop),
        Covenant::default(),
    )
    .await
    .unwrap();

    let recv_value = MicroTari::from(8_000_000);
    let (_recv_tx_id, sender_message) = generate_sender_transaction_message(recv_value);

    let _ = oms.get_recipient_transaction(sender_message).await.unwrap();

    oms.get_coinbase_transaction(6u64.into(), MicroTari::from(15_000_000), MicroTari::from(1_000_000), 2)
        .await
        .unwrap();

    let mut outputs = oms_db.fetch_pending_incoming_outputs().unwrap();
    assert_eq!(outputs.len(), 3);

    let o5_pos = outputs
        .iter()
        .position(|o| o.unblinded_output.value == MicroTari::from(8_000_000))
        .unwrap();
    let output5 = outputs.remove(o5_pos);
    let o6_pos = outputs
        .iter()
        .position(|o| o.unblinded_output.value == MicroTari::from(16_000_000))
        .unwrap();
    let output6 = outputs.remove(o6_pos);
    let output4 = outputs[0].clone();

    let output4_tx_output = output4.unblinded_output.as_transaction_output(&factories).unwrap();
    let output5_tx_output = output5.unblinded_output.as_transaction_output(&factories).unwrap();
    let output6_tx_output = output6.unblinded_output.as_transaction_output(&factories).unwrap();

    let balance = oms.get_balance().await.unwrap();

    assert_eq!(
        balance.available_balance,
        MicroTari::from(output2_value) + MicroTari::from(output3_value)
    );
    assert_eq!(MicroTari::from(0), balance.time_locked_balance.unwrap());
    assert_eq!(balance.pending_outgoing_balance, MicroTari::from(output1_value));
    assert_eq!(
        balance.pending_incoming_balance,
        MicroTari::from(output1_value) -
            MicroTari::from(900_000) -
            MicroTari::from(1260) + //Output4 = output 1 -900_000 and 1260 for fees
            MicroTari::from(8_000_000) +
            MicroTari::from(16_000_000)
    );

    // Output 1:    Spent in Block 5 - Unconfirmed
    // Output 2:    Mined block 1   Confirmed Block 4
    // Output 3:    Imported so will have Unspent status.
    // Output 4:    Received in Block 5 - Unconfirmed - Change from spending Output 1
    // Output 5:    Received in Block 5 - Unconfirmed
    // Output 6:    Coinbase from Block 5 - Unconfirmed

    let mut block5_header = BlockHeader::new(1);
    block5_header.height = 5;
    block_headers.insert(5, block5_header.clone());
    rpc_service_state.set_blocks(block_headers.clone());

    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().into()),
            mmr_position: 1,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output1_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().into()),
            mmr_position: 2,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output2_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output4_tx_output.clone().into()),
            mmr_position: 4,
            mined_height: 5,
            mined_in_block: block5_header.hash(),
            output_hash: output4_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output5_tx_output.clone().into()),
            mmr_position: 5,
            mined_height: 5,
            mined_in_block: block5_header.hash(),
            output_hash: output5_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output6_tx_output.clone().into()),
            mmr_position: 6,
            mined_height: 5,
            mined_in_block: block5_header.hash(),
            output_hash: output6_tx_output.hash(),
        },
    ];

    let mut utxo_query_responses = UtxoQueryResponses {
        best_block: block5_header.hash(),
        height_of_longest_chain: 5,
        responses,
    };

    rpc_service_state.set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent in the transaction that produced output4
    let mut query_deleted_response = QueryDeletedResponse {
        best_block: block5_header.hash(),
        height_of_longest_chain: 5,
        deleted_positions: vec![1],
        not_deleted_positions: vec![2, 4, 5, 6],
        heights_deleted_at: vec![5],
        blocks_deleted_in: vec![block5_header.hash()],
    };

    rpc_service_state.set_query_deleted_response(query_deleted_response.clone());

    oms.validate_txos().await.unwrap();

    let utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    assert_eq!(utxo_query_calls[0].len(), 4);

    let query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(query_deleted_calls[0].mmr_positions.len(), 5);

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroTari::from(output2_value) + MicroTari::from(output3_value)
    );
    assert_eq!(MicroTari::from(0), balance.time_locked_balance.unwrap());

    assert_eq!(oms.get_unspent_outputs().await.unwrap().len(), 2);

    assert!(oms.get_spent_outputs().await.unwrap().is_empty());

    // Now we will update the mined_height in the responses so that the outputs are confirmed
    // Output 1:    Spent in Block 5 - Confirmed
    // Output 2:    Mined block 1   Confirmed Block 4
    // Output 3:    Imported so will have Unspent status
    // Output 4:    Received in Block 5 - Confirmed - Change from spending Output 1
    // Output 5:    Received in Block 5 - Confirmed
    // Output 6:    Coinbase from Block 5 - Confirmed

    utxo_query_responses.height_of_longest_chain = 8;
    utxo_query_responses.best_block = [8u8; 16].to_vec();
    rpc_service_state.set_utxo_query_response(utxo_query_responses);

    query_deleted_response.height_of_longest_chain = 8;
    query_deleted_response.best_block = [8u8; 16].to_vec();
    rpc_service_state.set_query_deleted_response(query_deleted_response);

    oms.validate_txos().await.unwrap();

    let utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    // The spent transaction is not checked during this second validation
    assert_eq!(utxo_query_calls[0].len(), 4);

    let query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(query_deleted_calls[0].mmr_positions.len(), 5);

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroTari::from(output2_value) + MicroTari::from(output3_value) + MicroTari::from(output1_value) -
            MicroTari::from(900_000) -
            MicroTari::from(1260) + //spent 900_000 and 1260 for fees
            MicroTari::from(8_000_000) +    //output 5
            MicroTari::from(16_000_000) // output 6
    );
    assert_eq!(balance.pending_outgoing_balance, MicroTari::from(0));
    assert_eq!(balance.pending_incoming_balance, MicroTari::from(0));
    assert_eq!(MicroTari::from(0), balance.time_locked_balance.unwrap());

    // Trigger another validation and only Output3 should be checked
    oms.validate_txos().await.unwrap();

    let utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(utxo_query_calls.len(), 1);
    assert_eq!(utxo_query_calls[0].len(), 1);
    assert_eq!(
        utxo_query_calls[0][0],
        output3.as_transaction_output(&factories).unwrap().hash()
    );

    // Now we will create responses that result in a reorg of block 5, keeping block4 the same.
    // Output 1:    Spent in Block 5 - Unconfirmed
    // Output 2:    Mined block 1   Confirmed Block 4
    // Output 3:    Imported so will have Unspent
    // Output 4:    Received in Block 5 - Unconfirmed - Change from spending Output 1
    // Output 5:    Reorged out
    // Output 6:    Reorged out
    let block5_header_reorg = BlockHeader::new(2);
    block5_header.height = 5;
    let mut block_headers = HashMap::new();
    block_headers.insert(1, block1_header.clone());
    block_headers.insert(4, block4_header.clone());
    block_headers.insert(5, block5_header_reorg.clone());
    rpc_service_state.set_blocks(block_headers.clone());

    // Update UtxoResponses to not have the received output5 and coinbase output6
    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().into()),
            mmr_position: 1,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output1_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().into()),
            mmr_position: 2,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output2_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output4_tx_output.clone().into()),
            mmr_position: 4,
            mined_height: 5,
            mined_in_block: block5_header_reorg.hash(),
            output_hash: output4_tx_output.hash(),
        },
    ];

    let mut utxo_query_responses = UtxoQueryResponses {
        best_block: block5_header_reorg.hash(),
        height_of_longest_chain: 5,
        responses,
    };

    rpc_service_state.set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent in the transaction that produced output4
    let mut query_deleted_response = QueryDeletedResponse {
        best_block: block5_header_reorg.hash(),
        height_of_longest_chain: 5,
        deleted_positions: vec![1],
        not_deleted_positions: vec![2, 4, 5, 6],
        heights_deleted_at: vec![5],
        blocks_deleted_in: vec![block5_header_reorg.hash()],
    };

    rpc_service_state.set_query_deleted_response(query_deleted_response.clone());

    // Trigger validation through a base_node_service event
    base_node_service_event_publisher
        .send(Arc::new(BaseNodeEvent::BaseNodeStateChanged(BaseNodeState::default())))
        .unwrap();

    let _ = rpc_service_state
        .wait_pop_get_header_by_height_calls(2, Duration::from_secs(60))
        .await
        .unwrap();

    let _utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    let _query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    // This is needed on a fast computer, otherwise the balance have not been updated correctly yet with the next step
    let mut event_stream = oms.get_event_stream();
    let delay = sleep(Duration::from_secs(10));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = event_stream.recv() => {
                 if let OutputManagerEvent::TxoValidationSuccess(_) = &*event.unwrap(){
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroTari::from(output2_value) + MicroTari::from(output3_value)
    );
    assert_eq!(balance.pending_outgoing_balance, MicroTari::from(output1_value));
    assert_eq!(
        balance.pending_incoming_balance,
        MicroTari::from(output1_value) - MicroTari::from(901_260)
    );
    assert_eq!(MicroTari::from(0), balance.time_locked_balance.unwrap());

    // Now we will update the mined_height in the responses so that the outputs on the reorged chain are confirmed
    // Output 1:    Spent in Block 5 - Confirmed
    // Output 2:    Mined block 1   Confirmed Block 4
    // Output 3:    Imported so will have Unspent
    // Output 4:    Received in Block 5 - Confirmed - Change from spending Output 1
    // Output 5:    Reorged out
    // Output 6:    Reorged out

    utxo_query_responses.height_of_longest_chain = 8;
    utxo_query_responses.best_block = [8u8; 16].to_vec();
    rpc_service_state.set_utxo_query_response(utxo_query_responses);

    query_deleted_response.height_of_longest_chain = 8;
    query_deleted_response.best_block = [8u8; 16].to_vec();
    rpc_service_state.set_query_deleted_response(query_deleted_response);

    let mut event_stream = oms.get_event_stream();

    let validation_id = oms.validate_txos().await.unwrap();

    let _utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    let _query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(30));
    tokio::pin!(delay);
    let mut validation_completed = false;
    loop {
        tokio::select! {
            event = event_stream.recv() => {
                 if let OutputManagerEvent::TxoValidationSuccess(id) = &*event.unwrap(){
                    if id == &validation_id {
                        validation_completed = true;
                        break;
                    }
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(validation_completed, "Validation protocol should complete");

    let balance = oms.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroTari::from(output2_value) + MicroTari::from(output3_value) + MicroTari::from(output1_value) -
            MicroTari::from(901_260)
    );
    assert_eq!(balance.pending_outgoing_balance, MicroTari::from(0));
    assert_eq!(balance.pending_incoming_balance, MicroTari::from(0));
    assert_eq!(MicroTari::from(0), balance.time_locked_balance.unwrap());
}

#[tokio::test]
async fn test_txo_revalidation() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (
        mut oms,
        wallet_connectivity,
        _shutdown,
        _ts,
        mock_rpc_server,
        server_node_identity,
        rpc_service_state,
        _base_node_service_event_publisher,
    ) = setup_output_manager_service(backend, true).await;

    wallet_connectivity.notify_base_node_set(server_node_identity.to_peer());
    // Now we add the connection
    let mut connection = mock_rpc_server
        .create_connection(server_node_identity.to_peer(), "t/bnwallet/1".into())
        .await;
    wallet_connectivity.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let output1_value = 1_000_000;
    let output1 = create_unblinded_output(
        script!(Nop),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(output1_value),
    );
    let output1_tx_output = output1.as_transaction_output(&factories).unwrap();
    oms.add_output_with_tx_id(1.into(), output1.clone(), None)
        .await
        .unwrap();

    let output2_value = 2_000_000;
    let output2 = create_unblinded_output(
        script!(Nop),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(output2_value),
    );
    let output2_tx_output = output2.as_transaction_output(&factories).unwrap();

    oms.add_output_with_tx_id(2.into(), output2.clone(), None)
        .await
        .unwrap();

    let mut block1_header = BlockHeader::new(1);
    block1_header.height = 1;
    let mut block4_header = BlockHeader::new(1);
    block4_header.height = 4;

    let mut block_headers = HashMap::new();
    block_headers.insert(1, block1_header.clone());
    block_headers.insert(4, block4_header.clone());
    rpc_service_state.set_blocks(block_headers.clone());

    // These responses will mark outputs 1 and 2 and mined confirmed
    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().into()),
            mmr_position: 1,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output1_tx_output.hash(),
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().into()),
            mmr_position: 2,
            mined_height: 1,
            mined_in_block: block1_header.hash(),
            output_hash: output2_tx_output.hash(),
        },
    ];

    let utxo_query_responses = UtxoQueryResponses {
        best_block: block4_header.hash(),
        height_of_longest_chain: 4,
        responses,
    };

    rpc_service_state.set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent
    let query_deleted_response = QueryDeletedResponse {
        best_block: block4_header.hash(),
        height_of_longest_chain: 4,
        deleted_positions: vec![],
        not_deleted_positions: vec![1, 2],
        heights_deleted_at: vec![],
        blocks_deleted_in: vec![],
    };

    rpc_service_state.set_query_deleted_response(query_deleted_response.clone());
    oms.validate_txos().await.unwrap();
    let _utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let unspent_txos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent_txos.len(), 2);

    // This response sets output1 as spent
    let query_deleted_response = QueryDeletedResponse {
        best_block: block4_header.hash(),
        height_of_longest_chain: 4,
        deleted_positions: vec![1],
        not_deleted_positions: vec![2],
        heights_deleted_at: vec![4],
        blocks_deleted_in: vec![block4_header.hash()],
    };

    rpc_service_state.set_query_deleted_response(query_deleted_response.clone());
    oms.revalidate_all_outputs().await.unwrap();
    let _utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let unspent_txos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent_txos.len(), 1);

    // This response sets output1 and 2 as spent
    let query_deleted_response = QueryDeletedResponse {
        best_block: block4_header.hash(),
        height_of_longest_chain: 4,
        deleted_positions: vec![1, 2],
        not_deleted_positions: vec![],
        heights_deleted_at: vec![4, 4],
        blocks_deleted_in: vec![block4_header.hash(), block4_header.hash()],
    };

    rpc_service_state.set_query_deleted_response(query_deleted_response.clone());
    oms.revalidate_all_outputs().await.unwrap();
    let _utxo_query_calls = rpc_service_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = rpc_service_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let unspent_txos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent_txos.len(), 0);
}

#[tokio::test]
async fn test_oms_key_manager_discrepancy() {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();
    let (_oms_request_sender, oms_request_receiver) = reply_channel::unbounded();

    let (oms_event_publisher, _) = broadcast::channel(200);
    let constants = ConsensusConstantsBuilder::new(Network::Weatherwax).build();
    let (sender, receiver_bns) = reply_channel::unbounded();
    let (event_publisher_bns, _) = broadcast::channel(100);

    let basenode_service_handle = BaseNodeServiceHandle::new(sender, event_publisher_bns);
    let mut mock_base_node_service = MockBaseNodeService::new(receiver_bns, shutdown.to_signal());
    mock_base_node_service.set_default_base_node_state();
    task::spawn(mock_base_node_service.run());

    let wallet_connectivity = create_wallet_connectivity_mock();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let db = OutputManagerDatabase::new(OutputManagerSqliteDatabase::new(connection, None));

    let master_seed1 = CipherSeed::new();

    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig::default(),
        oms_request_receiver,
        db.clone(),
        oms_event_publisher.clone(),
        factories.clone(),
        constants.clone(),
        shutdown.to_signal(),
        basenode_service_handle.clone(),
        wallet_connectivity.clone(),
        master_seed1.clone(),
        server_node_identity,
    )
    .await
    .unwrap();

    drop(output_manager_service);

    let (_oms_request_sender2, oms_request_receiver2) = reply_channel::unbounded();

    let server_node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let output_manager_service2 = OutputManagerService::new(
        OutputManagerServiceConfig::default(),
        oms_request_receiver2,
        db.clone(),
        oms_event_publisher.clone(),
        factories.clone(),
        constants.clone(),
        shutdown.to_signal(),
        basenode_service_handle.clone(),
        wallet_connectivity.clone(),
        master_seed1,
        server_node_identity2,
    )
    .await
    .expect("Should be able to make a new OMS with same master key");
    drop(output_manager_service2);

    let (_oms_request_sender3, oms_request_receiver3) = reply_channel::unbounded();
    let master_seed2 = CipherSeed::new();
    let server_node_identity3 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let output_manager_service3 = OutputManagerService::new(
        OutputManagerServiceConfig::default(),
        oms_request_receiver3,
        db,
        oms_event_publisher,
        factories,
        constants,
        shutdown.to_signal(),
        basenode_service_handle,
        wallet_connectivity,
        master_seed2,
        server_node_identity3,
    )
    .await;

    assert!(matches!(
        output_manager_service3,
        Err(OutputManagerError::MasterSeedMismatch)
    ));
}

#[tokio::test]
async fn test_get_status_by_tx_id() {
    let factories = CryptoFactories::default();

    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let (mut oms, _, _shutdown, _, _, _, _, _) = setup_output_manager_service(backend, true).await;

    let (_ti, uo1) = make_input(&mut OsRng.clone(), MicroTari::from(10000), &factories.commitment);
    oms.add_unvalidated_output(TxId::from(1u64), uo1, None).await.unwrap();

    let (_ti, uo2) = make_input(&mut OsRng.clone(), MicroTari::from(10000), &factories.commitment);
    oms.add_unvalidated_output(TxId::from(2u64), uo2, None).await.unwrap();

    let (status, _, _) = oms.get_output_statuses_by_tx_id(TxId::from(1u64)).await.unwrap();

    assert_eq!(status.len(), 1);
    assert_eq!(status[0], OutputStatus::EncumberedToBeReceived);
}
