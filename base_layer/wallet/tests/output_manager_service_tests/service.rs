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

use std::{collections::HashMap, convert::TryInto, sync::Arc, time::Duration};

use minotari_wallet::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
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
        UtxoSelectionCriteria,
    },
    test_utils::create_consensus_constants,
    transaction_service::handle::TransactionServiceHandle,
    util::wallet_identity::WalletIdentity,
};
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::{
    transaction::TxId,
    types::{ComAndPubSignature, FixedHash, PublicKey},
};
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
    test_utils::node_identity::build_node_identity,
};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcServer,
    blocks::BlockHeader,
    borsh::SerializedSize,
    covenants::Covenant,
    proto::base_node::{QueryDeletedData, QueryDeletedResponse, UtxoQueryResponse, UtxoQueryResponses},
    transactions::{
        fee::Fee,
        key_manager::{
            create_memory_db_key_manager,
            MemoryDbKeyManager,
            TransactionKeyManagerBranch,
            TransactionKeyManagerInterface,
        },
        tari_amount::{uT, MicroMinotari, T},
        test_helpers::{create_wallet_output_with_data, TestParams},
        transaction_components::{OutputFeatures, TransactionOutput, WalletOutput},
        transaction_protocol::{sender::TransactionSenderMessage, TransactionMetadata},
        weight::TransactionWeight,
        CryptoFactories,
        SenderTransactionProtocol,
    },
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{inputs, script, TariScript};
use tari_service_framework::reply_channel;
use tari_shutdown::Shutdown;
use tokio::{
    sync::{broadcast, broadcast::channel},
    task,
    time::sleep,
};

use crate::support::{
    base_node_service_mock::MockBaseNodeService,
    comms_rpc::{connect_rpc_client, BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    data::get_temp_sqlite_database_connection,
    utils::{make_input, make_input_with_features},
};

fn default_features_and_scripts_size_byte_size() -> std::io::Result<usize> {
    Ok(TransactionWeight::latest().round_up_features_and_scripts_size(
        OutputFeatures::default().get_serialized_size()? + TariScript::default().get_serialized_size()?,
    ))
}

struct TestOmsService {
    pub output_manager_handle: OutputManagerHandle,
    pub wallet_connectivity_mock: WalletConnectivityMock,
    pub _shutdown: Shutdown,
    pub _transaction_service_handle: TransactionServiceHandle,
    pub mock_rpc_service: MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    pub node_id: Arc<NodeIdentity>,
    pub base_node_wallet_rpc_mock_state: BaseNodeWalletRpcMockState,
    pub node_event: broadcast::Sender<Arc<BaseNodeEvent>>,
    pub key_manager_handle: MemoryDbKeyManager,
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
async fn setup_output_manager_service<T: OutputManagerBackend + 'static>(
    backend: T,
    with_connection: bool,
) -> TestOmsService {
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

    let mut wallet_connectivity_mock = create_wallet_connectivity_mock();
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    wallet_connectivity_mock.notify_base_node_set(server_node_identity.to_peer());
    wallet_connectivity_mock.base_node_changed().await;

    let service = BaseNodeWalletRpcMockService::new();
    let rpc_service_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();

    let mut mock_server = MockRpcServer::new(server, server_node_identity.clone());
    mock_server.serve();

    if with_connection {
        let mut connection = mock_server
            .create_connection(server_node_identity.to_peer(), protocol_name.into())
            .await;

        wallet_connectivity_mock.set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);
    }

    let key_manager = create_memory_db_key_manager();

    let wallet_identity = WalletIdentity::new(server_node_identity.clone(), Network::LocalNet);
    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig { ..Default::default() },
        oms_request_receiver,
        OutputManagerDatabase::new(backend),
        oms_event_publisher.clone(),
        factories,
        constants,
        shutdown.to_signal(),
        basenode_service_handle,
        wallet_connectivity_mock.clone(),
        wallet_identity,
        key_manager.clone(),
    )
    .await
    .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    task::spawn(async move { output_manager_service.start().await.unwrap() });

    TestOmsService {
        output_manager_handle: output_manager_service_handle,
        wallet_connectivity_mock,
        _shutdown: shutdown,
        _transaction_service_handle: ts_handle,
        mock_rpc_service: mock_server,
        node_id: server_node_identity,
        base_node_wallet_rpc_mock_state: rpc_service_state,
        node_event: event_publisher_bns,
        key_manager_handle: key_manager,
    }
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
    MemoryDbKeyManager,
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
    let key_manager = create_memory_db_key_manager();
    let wallet_identity = WalletIdentity::new(node_identity.clone(), Network::LocalNet);
    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig { ..Default::default() },
        oms_request_receiver,
        OutputManagerDatabase::new(backend),
        oms_event_publisher.clone(),
        factories,
        constants,
        shutdown.to_signal(),
        base_node_service_handle.clone(),
        connectivity,
        wallet_identity,
        key_manager.clone(),
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
        key_manager,
    )
}

async fn generate_sender_transaction_message(
    amount: MicroMinotari,
    key_manager: &MemoryDbKeyManager,
) -> (TxId, TransactionSenderMessage) {
    let input = make_input(&mut OsRng, 2 * amount, &OutputFeatures::default(), key_manager).await;
    let mut builder = SenderTransactionProtocol::builder(create_consensus_constants(0), key_manager.clone());
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari(20))
        .with_input(input)
        .await
        .unwrap()
        .with_recipient_data(
            script!(Nop),
            OutputFeatures::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            amount,
        )
        .await
        .unwrap();

    let change = TestParams::new(key_manager).await;
    builder.with_change_data(
        script!(Nop),
        inputs!(change.script_key_pk),
        change.script_key_id,
        change.spend_key_id,
        Covenant::default(),
    );

    let mut stp = builder.build().await.unwrap();
    let tx_id = stp.get_tx_id().unwrap();
    (
        tx_id,
        TransactionSenderMessage::new_single_round_message(stp.build_single_round_message(key_manager).await.unwrap()),
    )
}

#[tokio::test]
async fn fee_estimate() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let uo = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(3000),
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight_params());
    // minimum fpg
    let fee_per_gram = MicroMinotari::from(1);
    let fee = oms
        .output_manager_handle
        .fee_estimate(
            MicroMinotari::from(100),
            UtxoSelectionCriteria::default(),
            fee_per_gram,
            1,
            1,
        )
        .await
        .unwrap();
    assert_eq!(
        fee,
        fee_calc.calculate(
            fee_per_gram,
            1,
            1,
            2,
            2 * default_features_and_scripts_size_byte_size()
                .expect("Failed to get default features and scripts size byte size")
        )
    );

    let fee_per_gram = MicroMinotari::from(5);
    for outputs in 1..5 {
        let fee = oms
            .output_manager_handle
            .fee_estimate(
                MicroMinotari::from(100),
                UtxoSelectionCriteria::default(),
                fee_per_gram,
                1,
                outputs,
            )
            .await
            .unwrap();

        assert_eq!(
            fee,
            fee_calc.calculate(
                fee_per_gram,
                1,
                1,
                outputs + 1,
                default_features_and_scripts_size_byte_size()
                    .expect("Failed to get default features and scripts size byte size") *
                    (outputs + 1)
            )
        );
    }

    // not enough funds
    let fee = oms
        .output_manager_handle
        .fee_estimate(
            MicroMinotari::from(2750),
            UtxoSelectionCriteria::default(),
            fee_per_gram,
            1,
            1,
        )
        .await
        .unwrap();
    assert_eq!(fee, MicroMinotari::from(375));
}

#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn test_utxo_selection_no_chain_metadata() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    // no chain metadata
    let (mut oms, _shutdown, _, _, _, key_manager) =
        setup_oms_with_bn_state(backend.clone(), None, server_node_identity).await;

    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight_params());
    // no utxos - not enough funds
    let amount = MicroMinotari::from(1000);
    let fee_per_gram = MicroMinotari::from(2);
    let err = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // create 10 utxos with maturity at heights from 1 to 10
    let mut unspent = Vec::with_capacity(10);
    for i in 1..=10 {
        let uo = make_input_with_features(
            &mut OsRng.clone(),
            i * amount,
            OutputFeatures {
                maturity: i,
                ..Default::default()
            },
            &key_manager,
        )
        .await;
        oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&key_manager).await.unwrap(), true));
    }
    backend.mark_outputs_as_unspent(unspent).unwrap();

    // but we have no chain state so the lowest maturity should be used
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            String::new(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that lowest 2 maturities were encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 8);
    for (index, utxo) in utxos.iter().enumerate() {
        let i = index as u64 + 3;
        assert_eq!(utxo.wallet_output.features.maturity, i);
        assert_eq!(utxo.wallet_output.value, i * amount);
    }

    // test that we can get a fee estimate with no chain metadata
    let fee = oms
        .fee_estimate(amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        1,
        3,
        default_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size") *
            3,
    );
    assert_eq!(fee, expected_fee);

    let spendable_amount = (3..=10).sum::<u64>() * amount;
    let fee = oms
        .fee_estimate(spendable_amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    assert_eq!(fee, MicroMinotari::from(256));

    let broke_amount = spendable_amount + MicroMinotari::from(2000);
    let fee = oms
        .fee_estimate(broke_amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    assert_eq!(fee, MicroMinotari::from(256));

    // coin split uses the "Largest" selection strategy
    let (_, tx, utxos_total_value) = oms.create_coin_split(vec![], amount, 5, fee_per_gram).await.unwrap();
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        1,
        6,
        default_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size") *
            6,
    );
    assert_eq!(tx.body.get_total_fee().unwrap(), expected_fee);
    assert_eq!(utxos_total_value, MicroMinotari::from(5_000));

    // test that largest utxo was encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 7);
    for (index, utxo) in utxos.iter().enumerate() {
        let i = index as u64 + 3;
        assert_eq!(utxo.wallet_output.features.maturity, i);
        assert_eq!(utxo.wallet_output.value, i * amount);
    }
}

#[tokio::test]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
async fn test_utxo_selection_with_chain_metadata() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    // setup with chain metadata at a height of 6
    let backend = OutputManagerSqliteDatabase::new(connection);
    let (mut oms, _shutdown, _, _, _, key_manager) =
        setup_oms_with_bn_state(backend.clone(), Some(6), server_node_identity).await;
    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight_params());

    // no utxos - not enough funds
    let amount = MicroMinotari::from(1000);
    let fee_per_gram = MicroMinotari::from(2);
    let err = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // create 10 utxos with maturity at heights from 1 to 10
    let mut unspent = Vec::with_capacity(10);
    for i in 1..=10 {
        let uo = make_input_with_features(
            &mut OsRng.clone(),
            i * amount,
            OutputFeatures {
                maturity: i,
                ..Default::default()
            },
            &key_manager,
        )
        .await;
        oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&key_manager).await.unwrap(), true));
    }
    backend.mark_outputs_as_unspent(unspent).unwrap();

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 10);

    // test fee estimates
    let fee = oms
        .fee_estimate(amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        2,
        3,
        default_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size") *
            3,
    );
    assert_eq!(fee, expected_fee);

    let spendable_amount = (1..=6).sum::<u64>() * amount;
    let fee = oms
        .fee_estimate(spendable_amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    assert_eq!(fee, MicroMinotari::from(256));

    // test coin split is maturity aware
    let (_, tx, utxos_total_value) = oms.create_coin_split(vec![], amount, 5, fee_per_gram).await.unwrap();
    assert_eq!(utxos_total_value, MicroMinotari::from(5_000));
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        1,
        6,
        default_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size") *
            6,
    );
    assert_eq!(tx.body.get_total_fee().unwrap(), expected_fee);

    // test that largest spendable utxo was encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 9);
    let found = utxos.iter().any(|u| u.wallet_output.value == 6 * amount);
    assert!(!found, "An unspendable utxo was selected");

    // test transactions
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that utxos with the lowest 2 maturities were encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 7);
    for utxo in &utxos {
        assert_ne!(utxo.wallet_output.features.maturity, 1);
        assert_ne!(utxo.wallet_output.value, amount);
        assert_ne!(utxo.wallet_output.features.maturity, 2);
        assert_ne!(utxo.wallet_output.value, 2 * amount);
    }

    // when the amount is greater than the largest utxo, then "Largest" selection strategy is used
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            6 * amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // test that utxos with the highest spendable 2 maturities were encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 5);
    for utxo in &utxos {
        assert_ne!(utxo.wallet_output.features.maturity, 4);
        assert_ne!(utxo.wallet_output.value, 4 * amount);
        assert_ne!(utxo.wallet_output.features.maturity, 5);
        assert_ne!(utxo.wallet_output.value, 5 * amount);
    }
}

#[tokio::test]
async fn test_utxo_selection_with_tx_priority() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    let server_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    // setup with chain metadata at a height of 6
    let backend = OutputManagerSqliteDatabase::new(connection);
    let (mut oms, _shutdown, _, _, _, key_manager) =
        setup_oms_with_bn_state(backend.clone(), Some(6), server_node_identity).await;

    let amount = MicroMinotari::from(2000);
    let fee_per_gram = MicroMinotari::from(2);

    // Low priority
    let uo_low_1 = make_input_with_features(
        &mut OsRng.clone(),
        amount,
        OutputFeatures {
            maturity: 1,
            ..Default::default()
        },
        &key_manager,
    )
    .await;
    oms.add_output(uo_low_1.clone(), None).await.unwrap();
    // High priority
    let uo_high = make_input_with_features(
        &mut OsRng.clone(),
        amount,
        OutputFeatures {
            maturity: 1,
            ..Default::default()
        },
        &key_manager,
    )
    .await;
    oms.add_output(uo_high.clone(), Some(SpendingPriority::HtlcSpendAsap))
        .await
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_high.hash(&key_manager).await.unwrap(), true)])
        .unwrap();
    // Low priority
    let uo_low_2 = make_input_with_features(
        &mut OsRng.clone(),
        amount,
        OutputFeatures {
            maturity: 1,
            ..Default::default()
        },
        &key_manager,
    )
    .await;
    oms.add_output(uo_low_2.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_low_2.hash(&key_manager).await.unwrap(), true)])
        .unwrap();

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 3);

    assert_eq!(utxos[0].spending_priority, SpendingPriority::Normal);
    assert_eq!(utxos[0].wallet_output.spending_key_id, uo_low_1.spending_key_id);
    assert_eq!(utxos[1].spending_priority, SpendingPriority::HtlcSpendAsap);
    assert_eq!(utxos[1].wallet_output.spending_key_id, uo_high.spending_key_id);
    assert_eq!(utxos[2].spending_priority, SpendingPriority::Normal);
    assert_eq!(utxos[2].wallet_output.spending_key_id, uo_low_2.spending_key_id);

    // test transactions
    let stp = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    assert!(stp.get_tx_id().is_ok());

    // Test that the UTXOs with the lowest priority was left
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 2);
    assert_ne!(utxos[0].wallet_output.spending_key_id, uo_high.spending_key_id);
    assert_ne!(utxos[1].wallet_output.spending_key_id, uo_high.spending_key_id);
}

#[tokio::test]
async fn send_not_enough_funds() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let num_outputs = 20usize;
    let mut unspent: Vec<(FixedHash, bool)> = Vec::with_capacity(num_outputs);
    for _i in 0..num_outputs {
        let uo = make_input(
            &mut OsRng.clone(),
            MicroMinotari::from(200 + OsRng.next_u64() % 1000),
            &OutputFeatures::default(),
            &oms.key_manager_handle,
        )
        .await;
        oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&oms.key_manager_handle).await.unwrap(), true));
    }
    backend.mark_outputs_as_unspent(unspent).unwrap();

    match oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(num_outputs as u64 * 2000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
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
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let fee_per_gram = MicroMinotari::from(4);
    let constants = create_consensus_constants(0);
    let fee_without_change = Fee::new(*constants.transaction_weight_params()).calculate(
        fee_per_gram,
        1,
        2,
        1,
        default_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size"),
    );
    let value1 = 5000;
    let uo_1 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle).await,
        MicroMinotari::from(value1),
        &oms.key_manager_handle,
    )
    .await
    .unwrap();
    oms.output_manager_handle.add_output(uo_1.clone(), None).await.unwrap();

    backend
        .mark_outputs_as_unspent(vec![(uo_1.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let value2 = 8000;
    let uo_2 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle).await,
        MicroMinotari::from(value2),
        &oms.key_manager_handle,
    )
    .await
    .unwrap();
    oms.output_manager_handle.add_output(uo_2.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_2.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let stp = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(value1 + value2) - fee_without_change,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            TariScript::default(),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();

    assert_eq!(stp.get_amount_to_self().unwrap(), MicroMinotari::from(0));
    assert_eq!(
        oms.output_manager_handle
            .get_balance()
            .await
            .unwrap()
            .pending_incoming_balance,
        MicroMinotari::from(0)
    );
}

#[tokio::test]
async fn send_not_enough_for_change() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let fee_per_gram = MicroMinotari::from(4);
    let constants = create_consensus_constants(0);
    let fee_without_change = Fee::new(*constants.transaction_weight_params()).calculate(fee_per_gram, 1, 2, 1, 0);
    let value1 = MicroMinotari(500);
    let uo_1 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle).await,
        value1,
        &oms.key_manager_handle,
    )
    .await
    .unwrap();
    oms.output_manager_handle.add_output(uo_1.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_1.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let value2 = MicroMinotari(800);
    let uo_2 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle).await,
        value2,
        &oms.key_manager_handle,
    )
    .await
    .unwrap();
    oms.output_manager_handle.add_output(uo_2.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_2.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    match oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            value1 + value2 + uT - fee_without_change,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
    {
        Err(OutputManagerError::NotEnoughFunds) => {},
        _ => panic!(),
    }
}

#[tokio::test]
async fn cancel_transaction() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let num_outputs = 20;
    let mut unspent: Vec<(FixedHash, bool)> = Vec::with_capacity(num_outputs);
    for _i in 0..num_outputs {
        let uo = make_input(
            &mut OsRng.clone(),
            MicroMinotari::from(100 + OsRng.next_u64() % 1000),
            &OutputFeatures::default(),
            &oms.key_manager_handle,
        )
        .await;
        oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&oms.key_manager_handle).await.unwrap(), true));
    }
    backend.mark_outputs_as_unspent(unspent).unwrap();
    let stp = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();

    match oms.output_manager_handle.cancel_transaction(1u64.into()).await {
        Err(OutputManagerError::OutputManagerStorageError(OutputManagerStorageError::ValueNotFound)) => {},
        _ => panic!("Value should not exist"),
    }

    oms.output_manager_handle
        .cancel_transaction(stp.get_tx_id().unwrap())
        .await
        .unwrap();

    assert_eq!(
        oms.output_manager_handle.get_unspent_outputs().await.unwrap().len(),
        num_outputs
    );
}

#[tokio::test]
async fn cancel_transaction_and_reinstate_inbound_tx() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend, true).await;

    let value = MicroMinotari::from(5000);
    let (tx_id, sender_message) = generate_sender_transaction_message(value, &oms.key_manager_handle).await;
    let _rtp = oms
        .output_manager_handle
        .get_recipient_transaction(sender_message)
        .await
        .unwrap();
    assert_eq!(oms.output_manager_handle.get_unspent_outputs().await.unwrap().len(), 0);

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.pending_incoming_balance, value);

    oms.output_manager_handle.cancel_transaction(tx_id).await.unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.pending_incoming_balance, MicroMinotari::from(0));

    oms.output_manager_handle
        .reinstate_cancelled_inbound_transaction_outputs(tx_id)
        .await
        .unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();

    assert_eq!(balance.pending_incoming_balance, value);
}

#[tokio::test]
async fn test_get_balance() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let balance = oms.output_manager_handle.get_balance().await.unwrap();

    assert_eq!(MicroMinotari::from(0), balance.available_balance);

    let mut total = MicroMinotari::from(0);
    let output_val = MicroMinotari::from(2000);
    let uo = make_input(
        &mut OsRng.clone(),
        output_val,
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    total += uo.value;
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let uo = make_input(
        &mut OsRng.clone(),
        output_val,
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    total += uo.value;
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let send_value = MicroMinotari::from(1000);
    let stp = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            send_value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();

    let change_val = stp.get_change_amount().unwrap();

    let recv_value = MicroMinotari::from(1500);
    let (_tx_id, sender_message) = generate_sender_transaction_message(recv_value, &oms.key_manager_handle).await;
    let _rtp = oms
        .output_manager_handle
        .get_recipient_transaction(sender_message)
        .await
        .unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();

    assert_eq!(output_val, balance.available_balance);
    assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());
    assert_eq!(recv_value + change_val, balance.pending_incoming_balance);
    assert_eq!(output_val, balance.pending_outgoing_balance);
}

#[tokio::test]
async fn sending_transaction_persisted_while_offline() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let available_balance = 20_000 * uT;
    let uo = make_input(
        &mut OsRng.clone(),
        available_balance / 2,
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let uo = make_input(
        &mut OsRng.clone(),
        available_balance / 2,
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance);
    assert_eq!(balance.time_locked_balance.unwrap(), MicroMinotari::from(0));
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(0));

    // Check that funds are encumbered and stay encumbered if the pending tx is not confirmed before restart
    let _stp = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance / 2);
    assert_eq!(balance.time_locked_balance.unwrap(), MicroMinotari::from(0));
    assert_eq!(balance.pending_outgoing_balance, available_balance / 2);

    // This simulates an offline wallet with a  queued transaction that has not been sent to the receiving wallet
    // This should be cleared as the transaction will be dropped.
    drop(oms.output_manager_handle);
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance);
    assert_eq!(balance.time_locked_balance.unwrap(), MicroMinotari::from(0));
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(0));

    // Check that is the pending tx is confirmed that the encumberance persists after restart
    let stp = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    let sender_tx_id = stp.get_tx_id().unwrap();
    oms.output_manager_handle
        .confirm_pending_transaction(sender_tx_id)
        .await
        .unwrap();

    drop(oms.output_manager_handle);
    let mut oms = setup_output_manager_service(backend, true).await;

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, MicroMinotari::from(10000));
    assert_eq!(balance.time_locked_balance.unwrap(), MicroMinotari::from(0));
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(10000));
}

#[tokio::test]
async fn coin_split_with_change() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let val1 = 6_000 * uT;
    let val2 = 7_000 * uT;
    let val3 = 8_000 * uT;
    let uo1 = make_input(&mut OsRng, val1, &OutputFeatures::default(), &oms.key_manager_handle).await;
    let uo2 = make_input(&mut OsRng, val2, &OutputFeatures::default(), &oms.key_manager_handle).await;
    let uo3 = make_input(&mut OsRng, val3, &OutputFeatures::default(), &oms.key_manager_handle).await;
    assert!(oms.output_manager_handle.add_output(uo1.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo2.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo3.clone(), None).await.is_ok());
    // lets mark them as unspent so we can use them
    backend
        .mark_outputs_as_unspent(vec![(uo1.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo2.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo3.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let fee_per_gram = MicroMinotari::from(5);
    let split_count = 8;
    let (_tx_id, coin_split_tx, amount) = oms
        .output_manager_handle
        .create_coin_split(vec![], 1000.into(), split_count, fee_per_gram)
        .await
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 2);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count + 1);
    let fee_calc = Fee::new(*create_consensus_constants(0).transaction_weight_params());
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        2,
        split_count + 1,
        (split_count + 1) *
            default_features_and_scripts_size_byte_size()
                .expect("Failed to get default features and scripts size byte size"),
    );
    assert_eq!(coin_split_tx.body.get_total_fee().unwrap(), expected_fee);
    // NOTE: assuming the LargestFirst strategy is used
    assert_eq!(amount, val3);
}

#[tokio::test]
async fn coin_split_no_change() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let fee_per_gram = MicroMinotari::from(5);
    let split_count = 15;
    let constants = create_consensus_constants(0);
    let fee_calc = Fee::new(*constants.transaction_weight_params());
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        3,
        split_count,
        split_count *
            default_features_and_scripts_size_byte_size()
                .expect("Failed to get default features and scripts size byte size"),
    );

    let val1 = 4_000 * uT;
    let val2 = 5_000 * uT;
    let val3 = 6_000 * uT + expected_fee;
    let uo1 = make_input(&mut OsRng, val1, &OutputFeatures::default(), &oms.key_manager_handle).await;
    let uo2 = make_input(&mut OsRng, val2, &OutputFeatures::default(), &oms.key_manager_handle).await;
    let uo3 = make_input(&mut OsRng, val3, &OutputFeatures::default(), &oms.key_manager_handle).await;
    assert!(oms.output_manager_handle.add_output(uo1.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo2.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo3.clone(), None).await.is_ok());
    // lets mark then as unspent so we can use them
    backend
        .mark_outputs_as_unspent(vec![(uo1.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo2.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo3.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let (_tx_id, coin_split_tx, amount) = oms
        .output_manager_handle
        .create_coin_split(vec![], 1000.into(), split_count, fee_per_gram)
        .await
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 3);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count);
    assert_eq!(coin_split_tx.body.get_total_fee().unwrap(), expected_fee);
    assert_eq!(amount, val1 + val2 + val3);
}

#[tokio::test]
async fn it_handles_large_coin_splits() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let val = 20 * T;
    let uo = make_input(&mut OsRng, val, &OutputFeatures::default(), &oms.key_manager_handle).await;
    assert!(oms.output_manager_handle.add_output(uo.clone(), None).await.is_ok());
    // lets mark them as unspent so we can use them
    backend
        .mark_outputs_as_unspent(vec![(uo.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let fee_per_gram = MicroMinotari::from(1);
    let split_count = 499;

    let (_tx_id, coin_split_tx, _amount) = oms
        .output_manager_handle
        .create_coin_split(vec![], 10000.into(), split_count, fee_per_gram)
        .await
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 1);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count + 1);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_txo_validation() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let oms_db = backend.clone();
    let mut oms = setup_output_manager_service(backend, true).await;

    // Now we add the connection
    let mut connection = oms
        .mock_rpc_service
        .create_connection(oms.node_id.to_peer(), "t/bnwallet/1".into())
        .await;
    oms.wallet_connectivity_mock
        .set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let output1_value = 1_000_000;
    let output1 = make_input(
        &mut OsRng,
        MicroMinotari::from(output1_value),
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    let output1_tx_output = output1.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    oms.output_manager_handle
        .add_output_with_tx_id(TxId::from(1u64), output1.clone(), None)
        .await
        .unwrap();
    oms_db
        .mark_outputs_as_unspent(vec![(output1.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let output2_value = 2_000_000;
    let output2 = make_input(
        &mut OsRng,
        MicroMinotari::from(output2_value),
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    let output2_tx_output = output2.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    oms.output_manager_handle
        .add_output_with_tx_id(TxId::from(2u64), output2.clone(), None)
        .await
        .unwrap();
    oms_db
        .mark_outputs_as_unspent(vec![(output2.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let output3_value = 4_000_000;
    let output3 = make_input(
        &mut OsRng,
        MicroMinotari::from(output3_value),
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;

    oms.output_manager_handle
        .add_output_with_tx_id(TxId::from(3u64), output3.clone(), None)
        .await
        .unwrap();

    oms_db
        .mark_outputs_as_unspent(vec![(output3.hash(&oms.key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let mut block1_header = BlockHeader::new(1);
    block1_header.height = 1;
    let mut block4_header = BlockHeader::new(1);
    block4_header.height = 4;

    let mut block_headers = HashMap::new();
    block_headers.insert(1, block1_header.clone());
    block_headers.insert(4, block4_header.clone());
    oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    // These responses will mark outputs 1 and 2 and mined confirmed
    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output1_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output2_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
    ];

    let utxo_query_responses = UtxoQueryResponses {
        best_block_hash: block4_header.hash().to_vec(),
        best_block_height: 4,
        responses,
    };

    oms.base_node_wallet_rpc_mock_state
        .set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 and output2 as mined, not spent
    let query_deleted_response = QueryDeletedResponse {
        best_block_hash: block4_header.hash().to_vec(),
        best_block_height: 4,
        data: vec![
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
        ],
    };

    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response.clone());
    oms.output_manager_handle.validate_txos().await.unwrap();
    let _utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    oms.output_manager_handle
        .prepare_transaction_to_send(
            4u64.into(),
            MicroMinotari::from(900_000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(10),
            TransactionMetadata::default(),
            "".to_string(),
            TariScript::default(),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();

    let recv_value = MicroMinotari::from(8_000_000);
    let (_recv_tx_id, sender_message) = generate_sender_transaction_message(recv_value, &oms.key_manager_handle).await;

    let _receiver_transaction_protocal = oms
        .output_manager_handle
        .get_recipient_transaction(sender_message)
        .await
        .unwrap();

    let mut outputs = oms_db.fetch_pending_incoming_outputs().unwrap();
    assert_eq!(outputs.len(), 2);

    let o5_pos = outputs
        .iter()
        .position(|o| o.wallet_output.value == MicroMinotari::from(8_000_000))
        .unwrap();
    let output5 = outputs.remove(o5_pos);
    let output4 = outputs[0].clone();

    let output4_tx_output = output4
        .wallet_output
        .to_transaction_output(&oms.key_manager_handle)
        .await
        .unwrap();
    let output5_tx_output = output5
        .wallet_output
        .to_transaction_output(&oms.key_manager_handle)
        .await
        .unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();

    assert_eq!(
        balance.available_balance,
        MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value)
    );
    assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(output1_value));
    assert_eq!(
        balance.pending_incoming_balance,
        MicroMinotari::from(output1_value) -
                MicroMinotari::from(900_000) -
                MicroMinotari::from(1320) + //Output4 = output 1 -900_000 and 1320 for fees
                MicroMinotari::from(8_000_000)
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
    oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output1_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output2_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output4_tx_output.clone().try_into().unwrap()),
            mined_at_height: 5,
            mined_in_block: block5_header.hash().to_vec(),
            output_hash: output4_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output5_tx_output.clone().try_into().unwrap()),
            mined_at_height: 5,
            mined_in_block: block5_header.hash().to_vec(),
            output_hash: output5_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
    ];

    let mut utxo_query_responses = UtxoQueryResponses {
        best_block_hash: block5_header.hash().to_vec(),
        best_block_height: 5,
        responses,
    };

    oms.base_node_wallet_rpc_mock_state
        .set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent in the transaction that produced output4
    let mut query_deleted_response = QueryDeletedResponse {
        best_block_hash: block5_header.hash().to_vec(),
        best_block_height: 5,
        data: vec![
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 5,
                block_deleted_in: block5_header.hash().to_vec(),
            },
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            QueryDeletedData {
                mined_at_height: 5,
                block_mined_in: block5_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            QueryDeletedData {
                mined_at_height: 5,
                block_mined_in: block5_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            QueryDeletedData {
                mined_at_height: 5,
                block_mined_in: block5_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
        ],
    };

    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response.clone());

    oms.output_manager_handle.validate_txos().await.unwrap();

    let utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    assert_eq!(utxo_query_calls[0].len(), 3);

    let query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(query_deleted_calls[0].hashes.len(), 4);

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value)
    );
    assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());

    assert_eq!(oms.output_manager_handle.get_unspent_outputs().await.unwrap().len(), 4);

    assert!(oms.output_manager_handle.get_spent_outputs().await.unwrap().is_empty());

    // Now we will update the mined_height in the responses so that the outputs are confirmed
    // Output 1:    Spent in Block 5 - Confirmed
    // Output 2:    Mined block 1   Confirmed Block 4
    // Output 3:    Imported so will have Unspent status
    // Output 4:    Received in Block 5 - Confirmed - Change from spending Output 1
    // Output 5:    Received in Block 5 - Confirmed
    // Output 6:    Coinbase from Block 5 - Confirmed

    utxo_query_responses.best_block_height = 8;
    utxo_query_responses.best_block_hash = [8u8; 16].to_vec();
    oms.base_node_wallet_rpc_mock_state
        .set_utxo_query_response(utxo_query_responses);

    query_deleted_response.best_block_height = 8;
    query_deleted_response.best_block_hash = [8u8; 16].to_vec();
    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response);

    oms.output_manager_handle.validate_txos().await.unwrap();

    let utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    // The spent transaction is not checked during this second validation
    assert_eq!(utxo_query_calls[0].len(), 3);

    let query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    assert_eq!(query_deleted_calls[0].hashes.len(), 4);

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value) + MicroMinotari::from(output1_value) -
                MicroMinotari::from(900_000) -
                MicroMinotari::from(1320) + //spent 900_000 and 1320 for fees
                MicroMinotari::from(8_000_000) // output 5
    );
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(1000000));
    assert_eq!(balance.pending_incoming_balance, MicroMinotari::from(0));
    assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());

    // Trigger another validation and only Output3 should be checked
    oms.output_manager_handle.validate_txos().await.unwrap();

    let utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(utxo_query_calls.len(), 1);
    assert_eq!(utxo_query_calls[0].len(), 1);
    assert_eq!(
        utxo_query_calls[0][0],
        output3
            .to_transaction_output(&oms.key_manager_handle)
            .await
            .unwrap()
            .hash()
            .to_vec()
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
    oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    // Update UtxoResponses to not have the received output5 and coinbase output6
    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output1_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output2_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output4_tx_output.clone().try_into().unwrap()),
            mined_at_height: 5,
            mined_in_block: block5_header_reorg.hash().to_vec(),
            output_hash: output4_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
    ];

    let mut utxo_query_responses = UtxoQueryResponses {
        best_block_hash: block5_header_reorg.hash().to_vec(),
        best_block_height: 5,
        responses,
    };

    oms.base_node_wallet_rpc_mock_state
        .set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent in the transaction that produced output4
    let mut query_deleted_response = QueryDeletedResponse {
        best_block_hash: block5_header_reorg.hash().to_vec(),
        best_block_height: 5,
        data: vec![
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 5,
                block_deleted_in: block5_header_reorg.hash().to_vec(),
            },
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            QueryDeletedData {
                mined_at_height: 5,
                block_mined_in: block5_header_reorg.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
        ],
    };

    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response.clone());

    // Trigger validation through a base_node_service event
    oms.node_event
        .send(Arc::new(BaseNodeEvent::NewBlockDetected(
            (*block5_header_reorg.hash()).into(),
            5,
        )))
        .unwrap();

    let _result = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_get_header_by_height_calls(2, Duration::from_secs(60))
        .await
        .unwrap();

    let _utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    let _query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    // This is needed on a fast computer, otherwise the balance have not been updated correctly yet with the next
    // step
    let mut event_stream = oms.output_manager_handle.get_event_stream();
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

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value)
    );
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(output1_value));
    assert_eq!(
        balance.pending_incoming_balance,
        MicroMinotari::from(output1_value) - MicroMinotari::from(901_320)
    );
    assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());

    // Now we will update the mined_height in the responses so that the outputs on the reorged chain are confirmed
    // Output 1:    Spent in Block 5 - Confirmed
    // Output 2:    Mined block 1   Confirmed Block 4
    // Output 3:    Imported so will have Unspent
    // Output 4:    Received in Block 5 - Confirmed - Change from spending Output 1
    // Output 5:    Reorged out
    // Output 6:    Reorged out

    utxo_query_responses.best_block_height = 8;
    utxo_query_responses.best_block_hash = [8u8; 16].to_vec();
    oms.base_node_wallet_rpc_mock_state
        .set_utxo_query_response(utxo_query_responses);

    query_deleted_response.best_block_height = 8;
    query_deleted_response.best_block_hash = [8u8; 16].to_vec();
    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response);

    let mut event_stream = oms.output_manager_handle.get_event_stream();

    let validation_id = oms.output_manager_handle.validate_txos().await.unwrap();

    let _utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();

    let _query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
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

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value) + MicroMinotari::from(output1_value) -
            MicroMinotari::from(901_320)
    );
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(0));
    assert_eq!(balance.pending_incoming_balance, MicroMinotari::from(0));
    assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_txo_revalidation() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());

    let mut oms = setup_output_manager_service(backend, true).await;

    // Now we add the connection
    let mut connection = oms
        .mock_rpc_service
        .create_connection(oms.node_id.to_peer(), "t/bnwallet/1".into())
        .await;
    oms.wallet_connectivity_mock
        .set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    let output1_value = 1_000_000;
    let key_manager = create_memory_db_key_manager();
    let output1 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&key_manager).await,
        MicroMinotari::from(output1_value),
        &key_manager,
    )
    .await
    .unwrap();
    let output1_tx_output = output1.to_transaction_output(&oms.key_manager_handle).await.unwrap();
    oms.output_manager_handle
        .add_output_with_tx_id(TxId::from(1u64), output1.clone(), None)
        .await
        .unwrap();

    let output2_value = 2_000_000;
    let output2 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&key_manager).await,
        MicroMinotari::from(output2_value),
        &key_manager,
    )
    .await
    .unwrap();
    let output2_tx_output = output2.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    oms.output_manager_handle
        .add_output_with_tx_id(TxId::from(2u64), output2.clone(), None)
        .await
        .unwrap();

    let mut block1_header = BlockHeader::new(1);
    block1_header.height = 1;
    let mut block4_header = BlockHeader::new(1);
    block4_header.height = 4;

    let mut block_headers = HashMap::new();
    block_headers.insert(1, block1_header.clone());
    block_headers.insert(4, block4_header.clone());
    oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    // These responses will mark outputs 1 and 2 and mined confirmed
    let responses = vec![
        UtxoQueryResponse {
            output: Some(output1_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output1_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
        UtxoQueryResponse {
            output: Some(output2_tx_output.clone().try_into().unwrap()),
            mined_at_height: 1,
            mined_in_block: block1_header.hash().to_vec(),
            output_hash: output2_tx_output.hash().to_vec(),
            mined_timestamp: 0,
        },
    ];

    let utxo_query_responses = UtxoQueryResponses {
        best_block_hash: block4_header.hash().to_vec(),
        best_block_height: 4,
        responses,
    };

    oms.base_node_wallet_rpc_mock_state
        .set_utxo_query_response(utxo_query_responses.clone());

    // This response sets output1 as spent
    let query_deleted_response = QueryDeletedResponse {
        best_block_hash: block4_header.hash().to_vec(),
        best_block_height: 4,
        data: vec![
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
        ],
    };

    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response.clone());
    oms.output_manager_handle.validate_txos().await.unwrap();
    let _utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let unspent_txos = oms.output_manager_handle.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent_txos.len(), 2);

    // This response sets output1 as spent
    let query_deleted_response = QueryDeletedResponse {
        best_block_hash: block4_header.hash().to_vec(),
        best_block_height: 4,
        data: vec![
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 4,
                block_deleted_in: block4_header.hash().to_vec(),
            },
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 0,
                block_deleted_in: Vec::new(),
            },
        ],
    };

    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response.clone());
    oms.output_manager_handle.revalidate_all_outputs().await.unwrap();
    let _utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let unspent_txos = oms.output_manager_handle.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent_txos.len(), 1);

    // This response sets output1 and 2 as spent
    let query_deleted_response = QueryDeletedResponse {
        best_block_hash: block4_header.hash().to_vec(),
        best_block_height: 4,
        data: vec![
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 4,
                block_deleted_in: block4_header.hash().to_vec(),
            },
            QueryDeletedData {
                mined_at_height: 1,
                block_mined_in: block1_header.hash().to_vec(),
                height_deleted_at: 4,
                block_deleted_in: block4_header.hash().to_vec(),
            },
        ],
    };

    oms.base_node_wallet_rpc_mock_state
        .set_query_deleted_response(query_deleted_response.clone());
    oms.output_manager_handle.revalidate_all_outputs().await.unwrap();
    let _utxo_query_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
        .await
        .unwrap();
    let _query_deleted_calls = oms
        .base_node_wallet_rpc_mock_state
        .wait_pop_query_deleted(1, Duration::from_secs(60))
        .await
        .unwrap();

    let unspent_txos = oms.output_manager_handle.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent_txos.len(), 0);
}

#[tokio::test]
async fn test_get_status_by_tx_id() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend, true).await;

    let uo1 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    oms.output_manager_handle
        .add_unvalidated_output(TxId::from(1u64), uo1, None)
        .await
        .unwrap();

    let uo2 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::default(),
        &oms.key_manager_handle,
    )
    .await;
    oms.output_manager_handle
        .add_unvalidated_output(TxId::from(2u64), uo2, None)
        .await
        .unwrap();

    let output_statuses_by_tx_id = oms
        .output_manager_handle
        .get_output_info_for_tx_id(TxId::from(1u64))
        .await
        .unwrap();

    assert_eq!(output_statuses_by_tx_id.statuses.len(), 1);
    assert_eq!(
        output_statuses_by_tx_id.statuses[0],
        OutputStatus::EncumberedToBeReceived
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn scan_for_recovery_test() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    const NUM_RECOVERABLE: usize = 5;
    const NUM_NON_RECOVERABLE: usize = 3;

    let mut recoverable_wallet_outputs = Vec::new();

    for i in 1..=NUM_RECOVERABLE {
        let (spending_key_result, _) = oms
            .key_manager_handle
            .get_next_key(TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
            .await
            .unwrap();
        let (script_key, public_script_key) = oms
            .key_manager_handle
            .get_next_key(TransactionKeyManagerBranch::ScriptKey.get_branch_key())
            .await
            .unwrap();
        let amount = 1_000 * i as u64;
        let features = OutputFeatures::default();
        let encrypted_data = oms
            .key_manager_handle
            .encrypt_data_for_recovery(&spending_key_result, None, amount)
            .await
            .unwrap();

        let uo = WalletOutput::new_current_version(
            MicroMinotari::from(amount),
            spending_key_result,
            features,
            script!(Nop),
            inputs!(public_script_key),
            script_key,
            PublicKey::default(),
            ComAndPubSignature::default(),
            0,
            Covenant::new(),
            encrypted_data,
            MicroMinotari::zero(),
            &oms.key_manager_handle,
        )
        .await
        .unwrap();
        recoverable_wallet_outputs.push(uo);
    }

    let mut non_recoverable_wallet_outputs = Vec::new();
    // we need to create a new key_manager to make the outputs non recoverable
    let key_manager = create_memory_db_key_manager();
    for i in 1..=NUM_NON_RECOVERABLE {
        let uo = make_input(
            &mut OsRng,
            MicroMinotari::from(1000 * i as u64),
            &OutputFeatures::default(),
            &key_manager,
        )
        .await;
        non_recoverable_wallet_outputs.push(uo)
    }
    let mut recoverable_outputs = Vec::new();
    for output in &recoverable_wallet_outputs {
        recoverable_outputs.push(output.to_transaction_output(&oms.key_manager_handle).await.unwrap());
    }

    let mut non_recoverable_outputs = Vec::new();
    for output in non_recoverable_wallet_outputs {
        non_recoverable_outputs.push(output.to_transaction_output(&oms.key_manager_handle).await.unwrap());
    }

    oms.output_manager_handle
        .add_output(recoverable_wallet_outputs[0].clone(), None)
        .await
        .unwrap();

    let recovered_outputs = oms
        .output_manager_handle
        .scan_for_recoverable_outputs(
            recoverable_outputs
                .clone()
                .into_iter()
                .chain(non_recoverable_outputs.clone().into_iter())
                .collect::<Vec<TransactionOutput>>(),
        )
        .await
        .unwrap();

    // Check that the non-rewindable outputs are not preset, also check that one rewindable output that was already
    // contained in the OMS database is also not included in the returns outputs.

    assert_eq!(recovered_outputs.len(), NUM_RECOVERABLE - 1);
    for o in recoverable_wallet_outputs.iter().skip(1) {
        assert!(recovered_outputs
            .iter()
            .any(|ro| ro.output.spending_key_id == o.spending_key_id));
    }
}

#[tokio::test]
async fn recovered_output_key_not_in_keychain() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;
    // we need to create a new key manager here as we dont want the input be recoverable from oms key chain
    let key_manager = create_memory_db_key_manager();
    let uo = make_input(
        &mut OsRng,
        MicroMinotari::from(1000u64),
        &OutputFeatures::default(),
        &key_manager,
    )
    .await;

    let rewindable_output = uo.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    let result = oms
        .output_manager_handle
        .scan_for_recoverable_outputs(vec![rewindable_output])
        .await;
    assert!(
        matches!(result.as_deref(), Ok([])),
        "It should not reach an error condition or return an output"
    );
}
