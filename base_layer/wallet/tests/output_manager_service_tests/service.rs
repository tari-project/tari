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

#![allow(clippy::indexing_slicing)]
// Overflow in test code panics, which is the desired failure mode for a test.
#![allow(clippy::arithmetic_side_effects)]
use std::sync::Arc;

use chrono::Utc;
use minotari_wallet::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::WalletConnectivityHandle,
    output_manager_service::{
        UtxoSelectionCriteria,
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerStorageError},
        handle::OutputManagerHandle,
        service::OutputManagerService,
        storage::{
            OutputStatus,
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::SpendingPriority,
            sqlite_db::OutputManagerSqliteDatabase,
        },
    },
    test_utils::create_consensus_constants,
    transaction_service::handle::TransactionServiceHandle,
    util::watch::Watch,
    utxo_scanner_service::{handle::UtxoScannerHandle, service::ScannedBlock},
};
use rand::Rng;
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{ComAndPubSignature, CompressedPublicKey, FixedHash, HashOutput, PrivateKey},
};
use tari_crypto::keys::SecretKey as SecretKeyTrait;
use tari_script::{ExecutionStack, TariScript, inputs, push_pubkey_script, script};
use tari_service_framework::reply_channel;
use tari_shutdown::Shutdown;
use tari_transaction_components::{
    TransactionBuilder,
    TransactionBuilderError,
    crypto_factories::CryptoFactories,
    fee::{Fee, addressed_output_memo, recipient_output_features_and_scripts_size},
    helpers::borsh::SerializedSize,
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    tari_amount::{MicroMinotari, T, uT},
    test_helpers::{TestParams, create_wallet_output_with_data},
    transaction_components::{
        MemoField,
        OutputFeatures,
        RangeProofType,
        TransactionOutput,
        WalletOutput,
        WalletOutputBuilder,
        covenants::Covenant,
        memo_field::TxType,
    },
    weight::TransactionWeight,
};
use tari_transaction_key_manager::legacy_key_manager::{
    LegacyTransactionKeyManagerInterface,
    MemoryKeyManager,
    create_new_random_key_manager,
};
use tokio::{
    sync::{broadcast, broadcast::channel},
    task,
};

use crate::support::{
    base_node_http_service_mock::MockHttpClientFactory,
    data::get_temp_sqlite_database_connection,
    utils::{make_input, make_input_with_features},
};

fn default_features_and_scripts_size_byte_size() -> std::io::Result<usize> {
    Ok(TransactionWeight::latest().round_up_features_and_scripts_size(
        OutputFeatures::default().get_serialized_size()? + TariScript::default().get_serialized_size()?,
    ))
}

// Size including the minimum payment_id (PADDING_SIZE = 130 bytes from MemoField::add_sender_address) for
// outputs-to-self (coin split/join). Must match what output_to_self_features_and_scripts_size computes.
fn output_to_self_features_and_scripts_size_byte_size() -> std::io::Result<usize> {
    const PAYMENT_ID_SIZE: usize = 130;
    Ok(TransactionWeight::latest().round_up_features_and_scripts_size(
        OutputFeatures::default().get_serialized_size()? +
            TariScript::default().get_serialized_size()? +
            PAYMENT_ID_SIZE,
    ))
}

/// The fee `OutputManagerService::fee_estimate` quotes when the wallet is short of funds: one input, no change, and
/// `num_outputs` recipient outputs each carrying the same `AddressAndData` memo the funded path quotes for.
///
/// `Fee::calculate` adds the features-and-scripts term once, so the per-output size is multiplied by the number of
/// outputs, exactly as the funded selection above it does.
///
/// `include_memo` exists so a test can show that the memo is actually being paid for: the quote built without it is
/// strictly cheaper, and an estimate that stopped counting the memo would collapse the two together.
fn insufficient_funds_fee_estimate(
    fee_per_gram: MicroMinotari,
    num_outputs: usize,
    include_memo: bool,
) -> MicroMinotari {
    let constants = create_consensus_constants(0);
    let memo = if include_memo {
        addressed_output_memo(
            MemoField::default(),
            TariAddress::default(),
            MicroMinotari::zero(),
            TxType::PaymentToOther,
        )
        .unwrap()
    } else {
        MemoField::default()
    };
    let size = recipient_output_features_and_scripts_size(
        constants.transaction_weight_params(),
        &OutputFeatures::default(),
        &TariScript::default(),
        &Covenant::new(),
        &memo,
    )
    .unwrap();
    Fee::new(*constants.transaction_weight_params()).calculate(
        fee_per_gram,
        1,
        1,
        num_outputs,
        size.saturating_mul(num_outputs),
    )
}

/// Asserts that an insufficient-funds fee quote prices the same output shape as the funded path: every output paid
/// for, memo included. A user who tops up to exactly the quoted amount has to be able to send.
///
/// The two inequalities are the load-bearing part - they are anchored in `Fee::calculate`'s own terms rather than in
/// the estimate being checked, so they still fail if the estimate stops counting the memo or stops counting it once
/// per output. The final equality only pins the exact number.
fn assert_insufficient_funds_quote(quoted: MicroMinotari, fee_per_gram: MicroMinotari, num_outputs: usize) {
    assert!(
        quoted > insufficient_funds_fee_estimate(fee_per_gram, num_outputs, false),
        "the insufficient-funds quote must pay for the recipient memo, got {quoted}"
    );
    if num_outputs > 1 {
        // Every extra output costs its own features, script and memo on top of its weight. If the quote charged
        // the features-and-scripts bytes only once, the gap between it and the single-output quote would be
        // exactly the weight of the extra outputs and nothing more.
        let constants = create_consensus_constants(0);
        let single_output = insufficient_funds_fee_estimate(fee_per_gram, 1, true);
        let extra_output_weight_only = Fee::new(*constants.transaction_weight_params()).calculate(
            fee_per_gram,
            0,
            0,
            num_outputs.saturating_sub(1),
            0,
        );
        assert!(
            quoted > single_output + extra_output_weight_only,
            "the insufficient-funds quote must pay for all {num_outputs} outputs' features and memos, got {quoted}"
        );
    }
    assert_eq!(quoted, insufficient_funds_fee_estimate(fee_per_gram, num_outputs, true));
}

struct TestOmsService {
    pub output_manager_handle: OutputManagerHandle<MemoryKeyManager>,
    pub _wallet_connectivity_mock: WalletConnectivityHandle<MockHttpClientFactory>,
    pub _shutdown: Shutdown,
    pub _transaction_service_handle: TransactionServiceHandle,
    pub _node_event: broadcast::Sender<Arc<BaseNodeEvent>>,
    pub key_manager_handle: MemoryKeyManager,
}

#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_lines)]
async fn setup_output_manager_service<T: OutputManagerBackend + 'static>(
    backend: T,
    _with_connection: bool,
) -> TestOmsService {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let (oms_event_publisher, _) = broadcast::channel(200);

    let (ts_request_sender, _ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher);

    let constants = create_consensus_constants(0);

    let (sender, _receiver_bns) = reply_channel::unbounded();
    let (event_publisher_bns, _) = broadcast::channel(100);
    let basenode_service_handle = BaseNodeServiceHandle::new(sender, event_publisher_bns.clone());

    let wallet_connectivity_mock = WalletConnectivityHandle::new(MockHttpClientFactory::default());

    let key_manager = create_new_random_key_manager().await.unwrap();

    let (event_sender, _) = broadcast::channel(200);
    let recovery_message_watch = Watch::new("unset".to_string());
    let one_sided_message_watch = Watch::new("unset".to_string());

    let scanner_handle = UtxoScannerHandle::new(event_sender.clone(), one_sided_message_watch, recovery_message_watch);

    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig { ..Default::default() },
        oms_request_receiver,
        OutputManagerDatabase::new(backend),
        oms_event_publisher.clone(),
        factories,
        constants,
        shutdown.to_signal(),
        basenode_service_handle,
        Network::LocalNet,
        wallet_connectivity_mock.clone(),
        key_manager.clone(),
        scanner_handle,
    )
    .await
    .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    task::spawn(async move { output_manager_service.start().await.unwrap() });

    TestOmsService {
        output_manager_handle: output_manager_service_handle,
        _wallet_connectivity_mock: wallet_connectivity_mock,
        _shutdown: shutdown,
        _transaction_service_handle: ts_handle,
        _node_event: event_publisher_bns,
        key_manager_handle: key_manager,
    }
}

pub async fn setup_oms_with_bn_state<T: OutputManagerBackend + 'static>(
    backend: T,
) -> (
    OutputManagerHandle<MemoryKeyManager>,
    Shutdown,
    TransactionServiceHandle,
    BaseNodeServiceHandle,
    broadcast::Sender<Arc<BaseNodeEvent>>,
    MemoryKeyManager,
) {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let (oms_event_publisher, _) = broadcast::channel(200);

    let (ts_request_sender, _ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher);

    let constants = create_consensus_constants(0);

    let (sender, _receiver_bns) = reply_channel::unbounded();
    let (event_publisher_bns, _) = broadcast::channel(100);

    let base_node_service_handle = BaseNodeServiceHandle::new(sender, event_publisher_bns.clone());
    let connectivity = WalletConnectivityHandle::new(MockHttpClientFactory::default());
    let key_manager = create_new_random_key_manager().await.unwrap();
    let (event_sender, _) = broadcast::channel(200);
    let recovery_message_watch = Watch::new("unset".to_string());
    let one_sided_message_watch = Watch::new("unset".to_string());
    let scanner_handle = UtxoScannerHandle::new(event_sender.clone(), one_sided_message_watch, recovery_message_watch);

    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig { ..Default::default() },
        oms_request_receiver,
        OutputManagerDatabase::new(backend),
        oms_event_publisher.clone(),
        factories,
        constants,
        shutdown.to_signal(),
        base_node_service_handle.clone(),
        Network::LocalNet,
        connectivity,
        key_manager.clone(),
        scanner_handle,
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

#[tokio::test]
async fn fee_estimate() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let uo = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(3000),
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend.mark_outputs_as_unspent(vec![(uo.output_hash(), true)]).unwrap();

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
        fee.0,
        fee_calc.calculate(
            fee_per_gram,
            1,
            1,
            2,
            2 * output_to_self_features_and_scripts_size_byte_size()
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
            fee.0,
            fee_calc.calculate(
                fee_per_gram,
                1,
                1,
                outputs + 1,
                output_to_self_features_and_scripts_size_byte_size()
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
    assert_insufficient_funds_quote(fee.0, fee_per_gram, 1);
}

#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn test_utxo_selection_no_chain_metadata() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    // no chain metadata
    let (mut oms, _shutdown, _, _, _, key_manager) = setup_oms_with_bn_state(backend.clone()).await;

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
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // create 10 utxos with maturity at heights from 1 to 10
    let mut unspent = Vec::with_capacity(10);
    for i in 1..=10 {
        let uo = make_input_with_features(
            &mut rand::rng().clone(),
            i * amount,
            OutputFeatures {
                maturity: i,
                ..Default::default()
            },
            key_manager.key_manager(),
        );
        oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.output_hash(), true));
    }
    backend.mark_outputs_as_unspent(unspent).unwrap();
    // but we have no chain state so the lowest maturity should be used
    let _tx_builder = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 9);

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
        output_to_self_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size") *
            3,
    );
    assert_eq!(fee.0, expected_fee);

    let spendable_amount = (3..=10).sum::<u64>() * amount;
    let fee = oms
        .fee_estimate(spendable_amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    assert_insufficient_funds_quote(fee.0, fee_per_gram, 2);

    let broke_amount = spendable_amount + MicroMinotari::from(2000);
    let fee = oms
        .fee_estimate(broke_amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    assert_insufficient_funds_quote(fee.0, fee_per_gram, 2);

    // coin split uses the "Largest" selection strategy
    let (_, tx, utxos_total_value) = oms.create_coin_split(vec![], amount, 5, fee_per_gram).await.unwrap();
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        1,
        6,
        output_to_self_features_and_scripts_size_byte_size()
            .expect("Failed to get output_to_self features and scripts size byte size") *
            6,
    );
    assert_eq!(tx.body.get_total_fee().unwrap(), expected_fee);
    assert_eq!(utxos_total_value, MicroMinotari::from(5_000));

    // test that largest utxo was encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 8);
}

#[tokio::test]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_lines)]
async fn test_utxo_selection_with_chain_metadata() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    // setup with chain metadata at a height of 6
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let scanned_block = ScannedBlock {
        header_hash: HashOutput::zero(),
        height: 6,
        timestamp: Utc::now().naive_utc(),
    };
    backend.save_last_scanned_height(scanned_block).unwrap();
    let (mut oms, _shutdown, _, _, _, key_manager) = setup_oms_with_bn_state(backend.clone()).await;
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
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // create 10 utxos with maturity at heights from 1 to 10
    let mut unspent = Vec::with_capacity(10);
    for i in 1..=10 {
        let uo = make_input_with_features(
            &mut rand::rng().clone(),
            i * amount,
            OutputFeatures {
                maturity: i,
                ..Default::default()
            },
            key_manager.key_manager(),
        );
        oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.output_hash(), true));
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
        1,
        3,
        output_to_self_features_and_scripts_size_byte_size()
            .expect("Failed to get default features and scripts size byte size") *
            3,
    );
    assert_eq!(fee.0, expected_fee);

    let spendable_amount = (1..=6).sum::<u64>() * amount;
    let fee = oms
        .fee_estimate(spendable_amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 2)
        .await
        .unwrap();
    assert_insufficient_funds_quote(fee.0, fee_per_gram, 2);

    // test coin split is maturity aware
    let (_, tx, utxos_total_value) = oms.create_coin_split(vec![], amount, 5, fee_per_gram).await.unwrap();
    assert_eq!(utxos_total_value, MicroMinotari::from(5_000));
    let expected_fee = fee_calc.calculate(
        fee_per_gram,
        1,
        1,
        6,
        output_to_self_features_and_scripts_size_byte_size()
            .expect("Failed to get output_to_self features and scripts size byte size") *
            6,
    );
    assert_eq!(tx.body.get_total_fee().unwrap(), expected_fee);

    // test that largest spendable utxo was encumbered
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 9);
    let found = utxos.iter().any(|u| u.wallet_output.value() == 6 * amount);
    assert!(!found, "An unspendable utxo was selected");

    // test transactions
    let _tx_builder = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 8);

    // when the amount is greater than the largest utxo, then "Largest" selection strategy is used
    let _tx_builder = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            6 * amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 6);
}

#[tokio::test]
async fn test_utxo_selection_with_tx_priority() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    // setup with chain metadata at a height of 6
    let backend = OutputManagerSqliteDatabase::new(connection);
    let scanned_block = ScannedBlock {
        header_hash: HashOutput::zero(),
        height: 6,
        timestamp: Utc::now().naive_utc(),
    };
    backend.save_last_scanned_height(scanned_block).unwrap();
    let (mut oms, _shutdown, _, _, _, key_manager) = setup_oms_with_bn_state(backend.clone()).await;

    let amount = MicroMinotari::from(2000);
    let fee_per_gram = MicroMinotari::from(2);

    // Low priority
    let uo_low_1 = make_input_with_features(
        &mut rand::rng().clone(),
        amount,
        OutputFeatures {
            maturity: 1,
            ..Default::default()
        },
        key_manager.key_manager(),
    );
    oms.add_output(uo_low_1.clone(), None).await.unwrap();
    // High priority
    let uo_high = make_input_with_features(
        &mut rand::rng().clone(),
        amount,
        OutputFeatures {
            maturity: 1,
            ..Default::default()
        },
        key_manager.key_manager(),
    );
    oms.add_output(uo_high.clone(), Some(SpendingPriority::HtlcSpendAsap))
        .await
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_high.output_hash(), true)])
        .unwrap();
    // Low priority
    let uo_low_2 = make_input_with_features(
        &mut rand::rng().clone(),
        amount,
        OutputFeatures {
            maturity: 1,
            ..Default::default()
        },
        key_manager.key_manager(),
    );
    oms.add_output(uo_low_2.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_low_2.output_hash(), true)])
        .unwrap();

    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 3);

    assert_eq!(utxos[0].spending_priority, SpendingPriority::Normal);
    assert_eq!(
        utxos[0].wallet_output.commitment_mask_key_id(),
        uo_low_1.commitment_mask_key_id()
    );
    assert_eq!(utxos[1].spending_priority, SpendingPriority::HtlcSpendAsap);
    assert_eq!(
        utxos[1].wallet_output.commitment_mask_key_id(),
        uo_high.commitment_mask_key_id()
    );
    assert_eq!(utxos[2].spending_priority, SpendingPriority::Normal);
    assert_eq!(
        utxos[2].wallet_output.commitment_mask_key_id(),
        uo_low_2.commitment_mask_key_id()
    );

    // test transactions
    let _tx_builder = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();

    // Test that the UTXOs with the lowest priority was left
    let utxos = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(utxos.len(), 2);
    assert_ne!(
        utxos[0].wallet_output.commitment_mask_key_id(),
        uo_high.commitment_mask_key_id()
    );
    assert_ne!(
        utxos[1].wallet_output.commitment_mask_key_id(),
        uo_high.commitment_mask_key_id()
    );
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
            &mut rand::rng().clone(),
            MicroMinotari::from(200 + rand::rng().next_u64() % 1000),
            &OutputFeatures::default(),
            oms.key_manager_handle.key_manager(),
        );
        oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.output_hash(), true));
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
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
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
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle),
        MicroMinotari::from(value1),
        oms.key_manager_handle.key_manager(),
    )
    .unwrap();
    oms.output_manager_handle.add_output(uo_1.clone(), None).await.unwrap();

    backend
        .mark_outputs_as_unspent(vec![(uo_1.output_hash(), true)])
        .unwrap();
    let value2 = 8000;
    let uo_2 = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle),
        MicroMinotari::from(value2),
        &oms.key_manager_handle,
    )
    .unwrap();
    oms.output_manager_handle.add_output(uo_2.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_2.output_hash(), true)])
        .unwrap();

    let _tx_builder = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(value1 + value2) - fee_without_change,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            TariScript::default(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();

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
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle),
        value1,
        &oms.key_manager_handle,
    )
    .unwrap();
    oms.output_manager_handle.add_output(uo_1.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_1.output_hash(), true)])
        .unwrap();
    let value2 = MicroMinotari(800);
    let uo_2 = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&oms.key_manager_handle),
        value2,
        oms.key_manager_handle.key_manager(),
    )
    .unwrap();
    oms.output_manager_handle.add_output(uo_2.clone(), None).await.unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo_2.output_hash(), true)])
        .unwrap();

    match oms
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            value1 + value2 + uT - fee_without_change,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
    {
        Err(OutputManagerError::NotEnoughFunds) => {},
        _ => panic!(),
    }
}

/// Test that the memo field size is accounted for in the UTXO selection process (branch and bound).
/// When a large memo is provided, the fee is higher due to the larger output size. This verifies
/// that BnB correctly accounts for the memo field when selecting UTXOs - a transaction that
/// succeeds with an empty memo should fail with a large memo if funds are tight.
#[tokio::test]
async fn test_memo_field_affects_utxo_selection() {
    let fee_per_gram = MicroMinotari::from(4);
    let amount = MicroMinotari::from(2000);

    // Create a large memo with 100 bytes of user data. This produces a memo > 130 bytes
    // (the PADDING_SIZE), resulting in larger recipient and change outputs and hence higher fees.
    let large_memo =
        MemoField::new_address_and_data(TariAddress::default(), 0.into(), true, TxType::PaymentToOther, vec![
            42u8;
            100
        ])
        .unwrap();
    assert!(large_memo.get_size() > 130, "Large memo should exceed the padding size");

    // First, determine how much fee is needed with an empty memo by doing a successful send.
    // Use a fresh OMS for the empty memo case.
    let (connection1, _tempdir1) = get_temp_sqlite_database_connection();
    let backend1 = OutputManagerSqliteDatabase::new(connection1.clone());
    let mut oms1 = setup_output_manager_service(backend1.clone(), true).await;

    // Use fee_estimate to determine the fee with the default memo (130 bytes AddressAndData).
    // An empty memo has 0 bytes, so the actual fee for an empty-memo transaction will be lower
    // than fee_estimate reports. We set our UTXO value to fee_estimate's result, which is enough
    // for an empty memo but too tight for a large memo (>130 bytes).
    let (fee_with_default_memo, _, _) = oms1
        .output_manager_handle
        .fee_estimate(amount, UtxoSelectionCriteria::default(), fee_per_gram, 1, 1)
        .await
        .unwrap();

    // The tight_value covers amount + fee with default memo. An empty memo (0 bytes) will
    // succeed since it needs less fee. A large memo (>130 bytes) will fail since it needs more fee.
    let tight_value = amount + fee_with_default_memo;

    // Set up OMS with a single tight UTXO — enough for empty memo, not for large memo
    let (connection2, _tempdir2) = get_temp_sqlite_database_connection();
    let backend2 = OutputManagerSqliteDatabase::new(connection2.clone());
    let mut oms2 = setup_output_manager_service(backend2.clone(), true).await;

    let uo = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&oms2.key_manager_handle),
        tight_value,
        &oms2.key_manager_handle,
    )
    .unwrap();
    oms2.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend2
        .mark_outputs_as_unspent(vec![(uo.output_hash(), true)])
        .unwrap();

    // Sending with an empty memo should succeed — the UTXO covers amount + fee
    let result_empty = oms2
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await;
    assert!(result_empty.is_ok(), "Empty memo transaction should succeed");

    // Set up a fresh OMS with the same tight UTXO for the large memo case
    let (connection3, _tempdir3) = get_temp_sqlite_database_connection();
    let backend3 = OutputManagerSqliteDatabase::new(connection3.clone());
    let mut oms3 = setup_output_manager_service(backend3.clone(), true).await;

    let uo2 = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&oms3.key_manager_handle),
        tight_value,
        &oms3.key_manager_handle,
    )
    .unwrap();
    oms3.output_manager_handle.add_output(uo2.clone(), None).await.unwrap();
    backend3
        .mark_outputs_as_unspent(vec![(uo2.output_hash(), true)])
        .unwrap();

    // Sending with a large memo should fail — the large memo increases output sizes
    // and the fee exceeds what the single UTXO can cover.
    let result_large = oms3
        .output_manager_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            script!(Nop).unwrap(),
            Covenant::default(),
            large_memo,
        )
        .await;
    assert!(
        result_large.is_err(),
        "Large memo transaction should fail with insufficient funds, but got: {:?}",
        result_large
    );
    assert!(matches!(result_large.unwrap_err(), OutputManagerError::NotEnoughFunds));
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
            &mut rand::rng().clone(),
            MicroMinotari::from(100 + rand::rng().next_u64() % 1000),
            &OutputFeatures::default(),
            oms.key_manager_handle.key_manager(),
        );
        oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.output_hash(), true));
    }
    backend.mark_outputs_as_unspent(unspent).unwrap();
    let tx_id = TxId::new_random();
    let _tx_builder = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            tx_id,
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();

    match oms.output_manager_handle.cancel_pending_transaction(1u64.into()).await {
        Err(OutputManagerError::OutputManagerStorageError(OutputManagerStorageError::ValueNotFound)) => {},
        _ => panic!("Value should not exist"),
    }

    oms.output_manager_handle
        .cancel_pending_transaction(tx_id)
        .await
        .unwrap();

    assert_eq!(
        oms.output_manager_handle.get_unspent_outputs().await.unwrap().len(),
        num_outputs
    );
}

#[tokio::test]
async fn sending_transaction_persisted_while_offline() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let available_balance = 20_000 * uT;
    let uo = make_input(
        &mut rand::rng().clone(),
        available_balance / 2,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend.mark_outputs_as_unspent(vec![(uo.output_hash(), true)]).unwrap();
    let uo = make_input(
        &mut rand::rng().clone(),
        available_balance / 2,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    oms.output_manager_handle.add_output(uo.clone(), None).await.unwrap();
    backend.mark_outputs_as_unspent(vec![(uo.output_hash(), true)]).unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance);
    assert_eq!(balance.time_locked_balance, None);
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
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance / 2);
    assert_eq!(balance.time_locked_balance, None);
    assert_eq!(balance.pending_outgoing_balance, available_balance / 2);

    // This simulates an offline wallet with a  queued transaction that has not been sent to the receiving wallet
    // This should be cleared as the transaction will be dropped.
    drop(oms.output_manager_handle);
    let mut oms = setup_output_manager_service(backend.clone(), true).await;

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, available_balance);
    assert_eq!(balance.time_locked_balance, None);
    assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(0));

    // Check that is the pending tx is confirmed that the encumberance persists after restart
    let tx_id = TxId::new_random();
    let _tx_builder = oms
        .output_manager_handle
        .prepare_transaction_to_send(
            tx_id,
            MicroMinotari::from(1000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap();
    oms.output_manager_handle
        .confirm_pending_transaction(tx_id, None, None)
        .await
        .unwrap();

    drop(oms.output_manager_handle);
    let mut oms = setup_output_manager_service(backend, true).await;

    let balance = oms.output_manager_handle.get_balance().await.unwrap();
    assert_eq!(balance.available_balance, MicroMinotari::from(10000));
    assert_eq!(balance.time_locked_balance, None);
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
    let uo1 = make_input(
        &mut rand::rng(),
        val1,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    let uo2 = make_input(
        &mut rand::rng(),
        val2,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    let uo3 = make_input(
        &mut rand::rng(),
        val3,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    assert!(oms.output_manager_handle.add_output(uo1.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo2.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo3.clone(), None).await.is_ok());
    // lets mark them as unspent so we can use them
    backend
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo2.output_hash(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo3.output_hash(), true)])
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
            output_to_self_features_and_scripts_size_byte_size()
                .expect("Failed to get output_to_self features and scripts size byte size"),
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
            output_to_self_features_and_scripts_size_byte_size()
                .expect("Failed to get output_to_self features and scripts size byte size"),
    );

    let val1 = 4_000 * uT;
    let val2 = 5_000 * uT;
    let val3 = 6_000 * uT + expected_fee;
    let uo1 = make_input(
        &mut rand::rng(),
        val1,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    let uo2 = make_input(
        &mut rand::rng(),
        val2,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    let uo3 = make_input(
        &mut rand::rng(),
        val3,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    assert!(oms.output_manager_handle.add_output(uo1.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo2.clone(), None).await.is_ok());
    assert!(oms.output_manager_handle.add_output(uo3.clone(), None).await.is_ok());
    // lets mark then as unspent so we can use them
    backend
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo2.output_hash(), true)])
        .unwrap();
    backend
        .mark_outputs_as_unspent(vec![(uo3.output_hash(), true)])
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
    let uo = make_input(
        &mut rand::rng(),
        val,
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    assert!(oms.output_manager_handle.add_output(uo.clone(), None).await.is_ok());
    // lets mark them as unspent so we can use them
    backend.mark_outputs_as_unspent(vec![(uo.output_hash(), true)]).unwrap();

    let fee_per_gram = MicroMinotari::from(1);
    let split_count = 100;

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
    // TODO: reimplement this test with an http mock
    // let (connection, _tempdir) = get_temp_sqlite_database_connection();
    // let backend = OutputManagerSqliteDatabase::new(connection.clone());
    // let oms_db = backend.clone();
    // let mut oms = setup_output_manager_service(backend, true).await;

    // let output1_value = 1_000_000;
    // let output1 = make_input(
    //     &mut rand::rng(),
    //     MicroMinotari::from(output1_value),
    //     &OutputFeatures::default(),
    //     &oms.key_manager_handle,
    // )
    // .await;
    // let output1_tx_output = output1.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    // oms.output_manager_handle
    //     .add_output_with_tx_id(TxId::from(1u64), output1.clone(), None)
    //     .await
    //     .unwrap();
    // oms_db
    //     .mark_outputs_as_unspent(vec![(output1.hash(&oms.key_manager_handle).await.unwrap(), true)])
    //     .unwrap();

    // let output2_value = 2_000_000;
    // let output2 = make_input(
    //     &mut rand::rng(),
    //     MicroMinotari::from(output2_value),
    //     &OutputFeatures::default(),
    //     &oms.key_manager_handle,
    // )
    // .await;
    // let output2_tx_output = output2.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    // oms.output_manager_handle
    //     .add_output_with_tx_id(TxId::from(2u64), output2.clone(), None)
    //     .await
    //     .unwrap();
    // oms_db
    //     .mark_outputs_as_unspent(vec![(output2.hash(&oms.key_manager_handle).await.unwrap(), true)])
    //     .unwrap();

    // let output3_value = 4_000_000;
    // let output3 = make_input(
    //     &mut rand::rng(),
    //     MicroMinotari::from(output3_value),
    //     &OutputFeatures::default(),
    //     &oms.key_manager_handle,
    // )
    // .await;
    // let output3_tx_output = output3.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    // oms.output_manager_handle
    //     .add_output_with_tx_id(TxId::from(3u64), output3.clone(), None)
    //     .await
    //     .unwrap();

    // oms_db
    //     .mark_outputs_as_unspent(vec![(output3.hash(&oms.key_manager_handle).await.unwrap(), true)])
    //     .unwrap();

    // let mut block1_header = BlockHeader::new(1);
    // block1_header.height = 1;
    // let mut block4_header = BlockHeader::new(1);
    // block4_header.height = 4;

    // let mut block_headers = HashMap::new();
    // block_headers.insert(1, block1_header.clone());
    // block_headers.insert(4, block4_header.clone());
    // oms.wallet_connectivity_mock.set_blocks(block_headers.clone());

    // // These responses will mark outputs 1,2,3 and mined confirmed
    // let responses = vec![
    //     UtxoQueryResponse {
    //         output: Some(output1_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output1_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output2_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output2_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output3_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output3_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    // ];

    // let utxo_query_responses = UtxoQueryResponses {
    //     best_block_hash: block4_header.hash().to_vec(),
    //     best_block_height: 4,
    //     responses,
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_utxo_query_response(utxo_query_responses.clone());

    // // This response sets output1 and output2, output3 as mined, not spent
    // let query_deleted_response = QueryDeletedResponse {
    //     best_block_hash: block4_header.hash().to_vec(),
    //     best_block_height: 4,
    //     data: vec![
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //     ],
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response.clone());
    // oms.output_manager_handle.validate_txos().await.unwrap();
    // let _utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();
    // let _query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // oms.output_manager_handle
    //     .prepare_transaction_to_send(
    //         4u64.into(),
    //         MicroMinotari::from(900_000),
    //         UtxoSelectionCriteria::default(),
    //         OutputFeatures::default(),
    //         MicroMinotari::from(10),
    //         TransactionMetadata::default(),
    //         TariScript::default(),
    //         Covenant::default(),
    //         MicroMinotari::zero(),
    //         TariAddress::default(),
    //         PaymentId::Empty,
    //     )
    //     .await
    //     .unwrap();

    // let recv_value = MicroMinotari::from(8_000_000);
    // let (_recv_tx_id, sender_message) = generate_sender_transaction_message(recv_value,
    // &oms.key_manager_handle).await;

    // let _receiver_transaction_protocal = oms
    //     .output_manager_handle
    //     .get_recipient_transaction(sender_message)
    //     .await
    //     .unwrap();

    // let mut outputs = oms_db.fetch_pending_incoming_outputs().unwrap();
    // assert_eq!(outputs.len(), 2);

    // let o5_pos = outputs
    //     .iter()
    //     .position(|o| o.wallet_output.value == MicroMinotari::from(8_000_000))
    //     .unwrap();
    // let output5 = outputs.remove(o5_pos);
    // let output4 = outputs[0].clone();

    // let output4_tx_output = output4
    //     .wallet_output
    //     .to_transaction_output(&oms.key_manager_handle)
    //     .await
    //     .unwrap();
    // let output5_tx_output = output5
    //     .wallet_output
    //     .to_transaction_output(&oms.key_manager_handle)
    //     .await
    //     .unwrap();

    // let balance = oms.output_manager_handle.get_balance().await.unwrap();

    // assert_eq!(
    //     balance.available_balance,
    //     MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value)
    // );
    // assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());
    // assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(output1_value));
    // assert_eq!(
    //     balance.pending_incoming_balance,
    //     MicroMinotari::from(output1_value) -
    //             MicroMinotari::from(900_000) -
    //             MicroMinotari::from(1320) + //Output4 = output 1 -900_000 and 1320 for fees
    //             MicroMinotari::from(8_000_000)
    // );

    // // Output 1:    Spent in Block 5 - Unconfirmed
    // // Output 2:    Mined block 1   Confirmed Block 4
    // // Output 3:    Mined block 1   Confirmed Block 4.
    // // Output 4:    Received in Block 5 - Unconfirmed - Change from spending Output 1
    // // Output 5:    Received in Block 5 - Unconfirmed
    // // Output 6:    Coinbase from Block 5 - Unconfirmed

    // let mut block5_header = BlockHeader::new(1);
    // block5_header.height = 5;
    // block_headers.insert(5, block5_header.clone());
    // oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    // let responses = vec![
    //     UtxoQueryResponse {
    //         output: Some(output1_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output1_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output2_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output2_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output3_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output3_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output4_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 5,
    //         mined_in_block: block5_header.hash().to_vec(),
    //         output_hash: output4_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output5_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 5,
    //         mined_in_block: block5_header.hash().to_vec(),
    //         output_hash: output5_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    // ];

    // let mut utxo_query_responses = UtxoQueryResponses {
    //     best_block_hash: block5_header.hash().to_vec(),
    //     best_block_height: 5,
    //     responses,
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_utxo_query_response(utxo_query_responses.clone());

    // // This response sets output1 as spent in the transaction that produced output4
    // let mut query_deleted_response = QueryDeletedResponse {
    //     best_block_hash: block5_header.hash().to_vec(),
    //     best_block_height: 5,
    //     data: vec![
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 5,
    //             block_deleted_in: block5_header.hash().to_vec(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 5,
    //             block_mined_in: block5_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 5,
    //             block_mined_in: block5_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 5,
    //             block_mined_in: block5_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //     ],
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response.clone());

    // oms.output_manager_handle.validate_txos().await.unwrap();

    // let utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // assert_eq!(utxo_query_calls[0].len(), 2);

    // let query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();
    // assert_eq!(query_deleted_calls[0].hashes.len(), 5);

    // let balance = oms.output_manager_handle.get_balance().await.unwrap();
    // assert_eq!(
    //     balance.available_balance,
    //     MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value)
    // );
    // assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());

    // assert_eq!(oms.output_manager_handle.get_unspent_outputs().await.unwrap().len(), 4);

    // assert!(oms.output_manager_handle.get_spent_outputs().await.unwrap().is_empty());

    // // Now we will update the mined_height in the responses so that the outputs are confirmed
    // // Output 1:    Spent in Block 5 - Confirmed
    // // Output 2:    Mined block 1   Confirmed Block 4
    // // Output 3:    Imported so will have Unspent status
    // // Output 4:    Received in Block 5 - Confirmed - Change from spending Output 1
    // // Output 5:    Received in Block 5 - Confirmed
    // // Output 6:    Coinbase from Block 5 - Confirmed

    // utxo_query_responses.best_block_height = 8;
    // utxo_query_responses.best_block_hash = [8u8; 16].to_vec();
    // oms.base_node_wallet_rpc_mock_state
    //     .set_utxo_query_response(utxo_query_responses);

    // query_deleted_response.best_block_height = 8;
    // query_deleted_response.best_block_hash = [8u8; 16].to_vec();
    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response);

    // oms.output_manager_handle.validate_txos().await.unwrap();

    // let utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // // The spent transaction is not checked during this second validation
    // assert_eq!(utxo_query_calls[0].len(), 2);

    // let query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // assert_eq!(query_deleted_calls[0].hashes.len(), 5);

    // let balance = oms.output_manager_handle.get_balance().await.unwrap();
    // assert_eq!(
    //     balance.available_balance,
    //     MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value) + MicroMinotari::from(output1_value)
    // - MicroMinotari::from(900_000) - MicroMinotari::from(1320) + //spent 900_000 and 1320 for fees
    //   MicroMinotari::from(8_000_000) // output 5
    // );
    // assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(1000000));
    // assert_eq!(balance.pending_incoming_balance, MicroMinotari::from(0));
    // assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());

    // // Now we will create responses that result in a reorg of block 5, keeping block4 the same.
    // // Output 1:    Spent in Block 5 - Unconfirmed
    // // Output 2:    Mined block 1   Confirmed Block 4
    // // Output 3:    Imported so will have Unspent
    // // Output 4:    Received in Block 5 - Unconfirmed - Change from spending Output 1
    // // Output 5:    Reorged out
    // // Output 6:    Reorged out
    // let block5_header_reorg = BlockHeader::new(2);
    // block5_header.height = 5;
    // let mut block_headers = HashMap::new();
    // block_headers.insert(1, block1_header.clone());
    // block_headers.insert(4, block4_header.clone());
    // block_headers.insert(5, block5_header_reorg.clone());
    // oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    // // Update UtxoResponses to not have the received output5 and coinbase output6
    // let responses = vec![
    //     UtxoQueryResponse {
    //         output: Some(output1_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output1_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output2_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output2_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output3_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output3_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output4_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 5,
    //         mined_in_block: block5_header_reorg.hash().to_vec(),
    //         output_hash: output4_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    // ];

    // let mut utxo_query_responses = UtxoQueryResponses {
    //     best_block_hash: block5_header_reorg.hash().to_vec(),
    //     best_block_height: 5,
    //     responses,
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_utxo_query_response(utxo_query_responses.clone());

    // // This response sets output1 as spent in the transaction that produced output4
    // let mut query_deleted_response = QueryDeletedResponse {
    //     best_block_hash: block5_header_reorg.hash().to_vec(),
    //     best_block_height: 5,
    //     data: vec![
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 5,
    //             block_deleted_in: block5_header_reorg.hash().to_vec(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 5,
    //             block_mined_in: block5_header_reorg.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //     ],
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response.clone());

    // oms.output_manager_handle.validate_txos().await.unwrap();

    // // This is needed on a fast computer, otherwise the balance have not been updated correctly yet with the next
    // // step
    // let mut event_stream = oms.output_manager_handle.get_event_stream();
    // let delay = sleep(Duration::from_secs(10));
    // tokio::pin!(delay);
    // loop {
    //     tokio::select! {
    //         event = event_stream.recv() => {
    //              if let OutputManagerEvent::TxoValidationSuccess(_) = &*event.unwrap(){
    //                 break;
    //             }
    //         },
    //         () = &mut delay => {
    //             break;
    //         },
    //     }
    // }

    // let balance = oms.output_manager_handle.get_balance().await.unwrap();
    // assert_eq!(
    //     balance.available_balance,
    //     MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value)
    // );
    // assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(output1_value));
    // assert_eq!(
    //     balance.pending_incoming_balance,
    //     MicroMinotari::from(output1_value) - MicroMinotari::from(901_320)
    // );
    // assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());

    // // Now we will update the mined_height in the responses so that the outputs on the reorged chain are confirmed
    // // Output 1:    Spent in Block 5 - Confirmed
    // // Output 2:    Mined block 1   Confirmed Block 4
    // // Output 3:    Imported so will have Unspent
    // // Output 4:    Received in Block 5 - Confirmed - Change from spending Output 1
    // // Output 5:    Reorged out
    // // Output 6:    Reorged out

    // utxo_query_responses.best_block_height = 8;
    // utxo_query_responses.best_block_hash = [8u8; 16].to_vec();
    // oms.base_node_wallet_rpc_mock_state
    //     .set_utxo_query_response(utxo_query_responses);

    // query_deleted_response.best_block_height = 8;
    // query_deleted_response.best_block_hash = [8u8; 16].to_vec();
    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response);

    // let mut event_stream = oms.output_manager_handle.get_event_stream();

    // let validation_id = oms.output_manager_handle.validate_txos().await.unwrap();

    // let _utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // let _query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // let delay = sleep(Duration::from_secs(30));
    // tokio::pin!(delay);
    // let mut validation_completed = false;
    // loop {
    //     tokio::select! {
    //         event = event_stream.recv() => {
    //              if let OutputManagerEvent::TxoValidationSuccess(id) = &*event.unwrap(){
    //                 if id == &validation_id {
    //                     validation_completed = true;
    //                     break;
    //                 }
    //             }
    //         },
    //         () = &mut delay => {
    //             break;
    //         },
    //     }
    // }
    // assert!(validation_completed, "Validation protocol should complete");

    // let balance = oms.output_manager_handle.get_balance().await.unwrap();
    // assert_eq!(
    //     balance.available_balance,
    //     MicroMinotari::from(output2_value) + MicroMinotari::from(output3_value) + MicroMinotari::from(output1_value)
    // - MicroMinotari::from(901_320)
    // );
    // assert_eq!(balance.pending_outgoing_balance, MicroMinotari::from(0));
    // assert_eq!(balance.pending_incoming_balance, MicroMinotari::from(0));
    // assert_eq!(MicroMinotari::from(0), balance.time_locked_balance.unwrap());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_txo_revalidation() {
    // // env_logger::init(); // Set `$env:RUST_LOG = "trace"`
    // let (connection, _tempdir) = get_temp_sqlite_database_connection();
    // let backend = OutputManagerSqliteDatabase::new(connection.clone());

    // let mut oms = setup_output_manager_service(backend, true).await;

    // // Now we add the connection
    // let mut connection = oms
    //     .mock_rpc_service
    //     .create_connection(oms.node_id.to_peer(), "t/bnwallet/1".into())
    //     .await;
    // oms.wallet_connectivity_mock
    //     .set_base_node_wallet_rpc_client(connect_rpc_client(&mut connection).await);

    // let output1_value = 1_000_000;
    // let key_manager = create_memory_db_key_manager().unwrap();
    // let output1 = create_wallet_output_with_data(
    //     script!(Nop).unwrap(),
    //     OutputFeatures::default(),
    //     &TestParams::new(&key_manager).await,
    //     MicroMinotari::from(output1_value),
    //     &key_manager,
    // )
    // .await
    // .unwrap();
    // let output1_tx_output = output1.to_transaction_output(&oms.key_manager_handle).await.unwrap();
    // oms.output_manager_handle
    //     .add_output_with_tx_id(TxId::from(1u64), output1.clone(), None)
    //     .await
    //     .unwrap();

    // let output2_value = 2_000_000;
    // let output2 = create_wallet_output_with_data(
    //     script!(Nop).unwrap(),
    //     OutputFeatures::default(),
    //     &TestParams::new(&key_manager).await,
    //     MicroMinotari::from(output2_value),
    //     &key_manager,
    // )
    // .await
    // .unwrap();
    // let output2_tx_output = output2.to_transaction_output(&oms.key_manager_handle).await.unwrap();

    // oms.output_manager_handle
    //     .add_output_with_tx_id(TxId::from(2u64), output2.clone(), None)
    //     .await
    //     .unwrap();

    // let mut block1_header = BlockHeader::new(1);
    // block1_header.height = 1;
    // let mut block4_header = BlockHeader::new(1);
    // block4_header.height = 4;

    // let mut block_headers = HashMap::new();
    // block_headers.insert(1, block1_header.clone());
    // block_headers.insert(4, block4_header.clone());
    // oms.base_node_wallet_rpc_mock_state.set_blocks(block_headers.clone());

    // // These responses will mark outputs 1 and 2 and mined confirmed
    // let responses = vec![
    //     UtxoQueryResponse {
    //         output: Some(output1_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output1_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    //     UtxoQueryResponse {
    //         output: Some(output2_tx_output.clone().try_into().unwrap()),
    //         mined_at_height: 1,
    //         mined_in_block: block1_header.hash().to_vec(),
    //         output_hash: output2_tx_output.hash().to_vec(),
    //         mined_timestamp: 0,
    //     },
    // ];

    // let utxo_query_responses = UtxoQueryResponses {
    //     best_block_hash: block4_header.hash().to_vec(),
    //     best_block_height: 4,
    //     responses,
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_utxo_query_response(utxo_query_responses.clone());

    // // This response sets output1 as spent
    // let query_deleted_response = QueryDeletedResponse {
    //     best_block_hash: block4_header.hash().to_vec(),
    //     best_block_height: 4,
    //     data: vec![
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //     ],
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response.clone());
    // oms.output_manager_handle.validate_txos().await.unwrap();
    // let _utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();
    // let _query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // let unspent_txos = oms.output_manager_handle.get_unspent_outputs().await.unwrap();
    // assert_eq!(unspent_txos.len(), 2);

    // // This response sets output1 as spent
    // let query_deleted_response = QueryDeletedResponse {
    //     best_block_hash: block4_header.hash().to_vec(),
    //     best_block_height: 4,
    //     data: vec![
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 4,
    //             block_deleted_in: block4_header.hash().to_vec(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 0,
    //             block_deleted_in: Vec::new(),
    //         },
    //     ],
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response.clone());
    // oms.output_manager_handle.revalidate_all_outputs().await.unwrap();
    // let _utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();
    // let _query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // let unspent_txos = oms.output_manager_handle.get_unspent_outputs().await.unwrap();
    // assert_eq!(unspent_txos.len(), 1);

    // // This response sets output1 and 2 as spent
    // let query_deleted_response = QueryDeletedResponse {
    //     best_block_hash: block4_header.hash().to_vec(),
    //     best_block_height: 4,
    //     data: vec![
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 4,
    //             block_deleted_in: block4_header.hash().to_vec(),
    //         },
    //         QueryDeletedData {
    //             mined_at_height: 1,
    //             block_mined_in: block1_header.hash().to_vec(),
    //             height_deleted_at: 4,
    //             block_deleted_in: block4_header.hash().to_vec(),
    //         },
    //     ],
    // };

    // oms.base_node_wallet_rpc_mock_state
    //     .set_query_deleted_response(query_deleted_response.clone());
    // oms.output_manager_handle.revalidate_all_outputs().await.unwrap();
    // let _utxo_query_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_utxo_query_calls(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();
    // let _query_deleted_calls = oms
    //     .base_node_wallet_rpc_mock_state
    //     .wait_pop_query_deleted(1, Duration::from_secs(60))
    //     .await
    //     .unwrap();

    // let unspent_txos = oms.output_manager_handle.get_unspent_outputs().await.unwrap();
    // assert_eq!(unspent_txos.len(), 0);
}

#[tokio::test]
async fn test_get_status_by_tx_id() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend, true).await;

    let uo1 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
    oms.output_manager_handle
        .add_unvalidated_output(TxId::from(1u64), uo1, None)
        .await
        .unwrap();

    let uo2 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::default(),
        oms.key_manager_handle.key_manager(),
    );
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
        OutputStatus::UnspentMinedUnconfirmed
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
        let commitment_mask_key = oms.key_manager_handle.get_random_key(None, None).unwrap();
        let script_key_id = TariKeyId::Derived {
            key: (&commitment_mask_key.key_id).into(),
        };
        let public_script_key = oms.key_manager_handle.get_public_key_at_key_id(&script_key_id).unwrap();

        let amount = 1_000 * i as u64;
        let features = OutputFeatures::default();
        let encrypted_data = oms
            .key_manager_handle
            .encrypt_data_for_recovery(&commitment_mask_key.key_id, None, amount, MemoField::new_empty())
            .unwrap();

        let uo = WalletOutput::new_current_version(
            MicroMinotari::from(amount),
            commitment_mask_key.key_id,
            features,
            script!(Nop).unwrap(),
            inputs!(public_script_key),
            script_key_id,
            CompressedPublicKey::default(),
            ComAndPubSignature::default(),
            0,
            Covenant::new(),
            encrypted_data,
            MicroMinotari::zero(),
            MemoField::new_empty(),
            &oms.key_manager_handle,
        )
        .unwrap();
        recoverable_wallet_outputs.push(uo);
    }

    let mut non_recoverable_wallet_outputs = Vec::new();
    // we need to create a new key_manager to make the outputs non recoverable
    let key_manager = create_new_random_key_manager().await.unwrap();
    for i in 1..=NUM_NON_RECOVERABLE {
        let uo = make_input(
            &mut rand::rng(),
            MicroMinotari::from(1000 * i as u64),
            &OutputFeatures::default(),
            key_manager.key_manager(),
        );
        non_recoverable_wallet_outputs.push(uo)
    }
    let mut recoverable_outputs = Vec::new();
    for output in &recoverable_wallet_outputs {
        recoverable_outputs.push(output.to_transaction_output().unwrap());
    }

    let mut non_recoverable_outputs = Vec::new();
    for output in non_recoverable_wallet_outputs {
        non_recoverable_outputs.push(output.to_transaction_output().unwrap());
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
    let mut recovered_outputs_keys = Vec::new();
    for o in &recovered_outputs {
        let commitment_branch_public_key = oms
            .key_manager_handle
            .get_public_key_at_key_id(o.output.commitment_mask_key_id())
            .unwrap();
        recovered_outputs_keys.push(commitment_branch_public_key);
    }

    // Check that the non-rewindable outputs are not preset, also check that one rewindable output that was already
    // contained in the OMS database is also not included in the returns outputs.

    assert_eq!(recovered_outputs.len(), NUM_RECOVERABLE - 1);
    for o in recoverable_wallet_outputs.iter().skip(1) {
        let commitment_branch_public_key = oms
            .key_manager_handle
            .get_public_key_at_key_id(o.commitment_mask_key_id())
            .unwrap();
        assert!(recovered_outputs_keys.contains(&commitment_branch_public_key));
    }
}

#[tokio::test]
async fn recovered_output_key_not_in_keychain() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let mut oms = setup_output_manager_service(backend.clone(), true).await;
    // we need to create a new key manager here as we dont want the input be recoverable from oms key chain
    let key_manager = create_new_random_key_manager().await.unwrap();
    let uo = make_input(
        &mut rand::rng(),
        MicroMinotari::from(1000u64),
        &OutputFeatures::default(),
        key_manager.key_manager(),
    );

    let rewindable_output = uo.to_transaction_output().unwrap();

    let result = oms
        .output_manager_handle
        .scan_for_recoverable_outputs(vec![rewindable_output])
        .await;
    assert!(
        matches!(result.as_deref(), Ok([])),
        "It should not reach an error condition or return an output"
    );
}

/// Builds and finalizes the transaction shape shared by the wallet's "spend the whole input to a single recipient"
/// paths: one input, one recipient output holding the whole input minus an up-front fee estimate, and no change
/// output. Returns the fee the transaction builder actually charged.
///
/// With no change output there is nothing to absorb a bad estimate: if `amount` leaves less than the builder
/// charges, the build fails outright.
async fn build_whole_input_spend(
    key_manager: &MemoryKeyManager,
    input: WalletOutput,
    fee_per_gram: MicroMinotari,
    output_features: OutputFeatures,
    amount: MicroMinotari,
    output_memo: MemoField,
) -> Result<MicroMinotari, TransactionBuilderError> {
    let mut builder =
        TransactionBuilder::new(create_consensus_constants(0), key_manager.clone(), Network::LocalNet).unwrap();
    builder
        .with_lock_height(0)
        .with_fee_per_gram(fee_per_gram)
        .with_prevent_fee_gt_amount(false)
        .with_input(input)
        .unwrap();

    let recipient_address = random_dual_address();
    let sender_offset = key_manager.get_random_key(None, None).unwrap();
    let encryption_key = key_manager.get_random_key(None, None).unwrap();
    let (commitment_mask, _script_key) = key_manager.get_next_commitment_mask_and_script_key().unwrap();
    let script = push_pubkey_script(
        &key_manager
            .stealth_address_script_spending_key(&commitment_mask.key_id, recipient_address.public_spend_key())
            .unwrap(),
    );
    let output = WalletOutputBuilder::new(amount, commitment_mask.key_id)
        .with_features(output_features)
        .with_script(script)
        .encrypt_data_for_recovery(key_manager, Some(&encryption_key.key_id), output_memo)
        .unwrap()
        .with_input_data(ExecutionStack::default())
        .with_sender_offset_public_key(sender_offset.pub_key.clone())
        .with_script_key(TariKeyId::Zero)
        .with_minimum_value_promise(MicroMinotari::zero())
        .sign_metadata_signature_user_verified(key_manager, &sender_offset.key_id, &recipient_address)
        .unwrap()
        .try_build(key_manager)
        .unwrap();
    builder
        .add_recipient(
            recipient_address,
            output,
            Some(sender_offset.key_id),
            Some(encryption_key.key_id),
        )
        .unwrap();

    builder.build().map(|finalized| finalized.fee)
}

fn random_dual_address() -> TariAddress {
    let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()));
    let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()));
    TariAddress::new_dual_address_with_default_features(view_key, spend_key, Network::LocalNet).unwrap()
}

/// The production estimate under test: every whole-input spend path routes its recipient output through
/// `recipient_output_features_and_scripts_size`. Deliberately no arithmetic here - re-deriving the size in the
/// test file is what let the original bug through, since the test would then agree with itself.
fn production_estimate(
    output_features: &OutputFeatures,
    memo: &MemoField,
    fee_per_gram: MicroMinotari,
) -> MicroMinotari {
    let constants = create_consensus_constants(0);
    let size = recipient_output_features_and_scripts_size(
        constants.transaction_weight_params(),
        output_features,
        &script!(PushPubKey(Box::default())).unwrap(),
        &Covenant::default(),
        memo,
    )
    .unwrap();
    Fee::new(*constants.transaction_weight_params()).calculate(fee_per_gram, 1, 1, 1, size)
}

async fn spendable_input(key_manager: &MemoryKeyManager, value: MicroMinotari) -> WalletOutput {
    make_input(
        &mut rand::rng(),
        value,
        &OutputFeatures::default(),
        key_manager.key_manager(),
    )
}

/// Drives the production fee estimate for a whole-input spend against a real `TransactionBuilder`:
/// - the estimate for `memo` must be exactly the fee the builder charges, and
/// - an estimate made for an empty memo must be too small to build at all.
///
/// The second half is what makes the first half meaningful: it fails if the estimate ever stops counting the memo.
async fn assert_estimate_matches_and_memo_is_load_bearing(
    key_manager: &MemoryKeyManager,
    value: MicroMinotari,
    fee_per_gram: MicroMinotari,
    output_features: OutputFeatures,
    memo: MemoField,
) {
    let expected_fee = production_estimate(&output_features, &memo, fee_per_gram);
    let fee = build_whole_input_spend(
        key_manager,
        spendable_input(key_manager, value).await,
        fee_per_gram,
        output_features.clone(),
        value - expected_fee,
        memo.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        fee, expected_fee,
        "the production estimate must be exactly what the transaction builder charges"
    );

    // Dropping the memo from the estimate is not a rounding nit: the transaction cannot be balanced at all.
    let short_fee = production_estimate(&output_features, &MemoField::default(), fee_per_gram);
    assert!(
        short_fee < expected_fee,
        "the estimate must count the memo bytes, {short_fee} vs {expected_fee}"
    );
    let err = build_whole_input_spend(
        key_manager,
        spendable_input(key_manager, value).await,
        fee_per_gram,
        output_features,
        value - short_fee,
        memo,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, TransactionBuilderError::SpendingMoreThanAvailable { .. }),
        "expected the build to fail outright, got {err:?}"
    );
}

/// Covers `OutputManagerService::encumber_aggregate_utxo`, whose recipient output carries the caller supplied
/// payment id (`--payment-id`) in its encrypted data.
///
/// The path itself cannot be driven from a unit test - it derives the pre-mine script key from a
/// `TariKeyId::LedgerKey` and needs a Ledger device - so this exercises the estimate the path now calls,
/// `recipient_output_features_and_scripts_size`, against a real transaction of the same shape.
#[tokio::test]
async fn encumber_aggregate_utxo_fee_estimate_counts_the_output_memo() {
    let key_manager = create_new_random_key_manager().await.unwrap();
    let payment_id = MemoField::new_open(vec![7u8; 64], TxType::PaymentToOther).unwrap();
    let output_features = OutputFeatures {
        maturity: 5,
        range_proof_type: RangeProofType::BulletProofPlus,
        ..Default::default()
    };
    assert_estimate_matches_and_memo_is_load_bearing(
        &key_manager,
        MicroMinotari::from(2_000_000),
        MicroMinotari::from(7),
        output_features,
        payment_id,
    )
    .await;
}

/// Covers `OutputManagerService::spend_backup_pre_mine_utxo`, whose recipient output memo is the caller's payment
/// id with the wallet's address attached - an `AddressAndData` memo padded to a 130 byte minimum, so omitting it
/// from the estimate is short by several grams.
///
/// The memo cannot be built before the fee is known because it carries the fee, so the service measures a copy
/// built with a zero fee. This exercises both production pieces: `addressed_output_memo` (the constructor used for
/// the measured copy and for the real output) and the estimate that consumes it. Like the aggregate path, the real
/// path needs a Ledger device, so the transaction shape is reproduced here.
#[tokio::test]
async fn spend_backup_pre_mine_utxo_fee_estimate_counts_the_output_memo() {
    let key_manager = create_new_random_key_manager().await.unwrap();
    let value = MicroMinotari::from(2_000_000);
    let fee_per_gram = MicroMinotari::from(7);
    let wallet_address = random_dual_address();
    // `OutputManagerRequest::SpendBackupPreMineUtxo` builds this from the hash of the output being spent. (The
    // pre-mine index that picks the script key is a different memo, the one decrypted from the input.)
    let payment_id = MemoField::new_open(vec![9u8; 32], TxType::PaymentToOther).unwrap();
    let output_features = OutputFeatures {
        maturity: 0,
        range_proof_type: RangeProofType::BulletProofPlus,
        ..Default::default()
    };

    let measured_memo = addressed_output_memo(
        payment_id.clone(),
        wallet_address.clone(),
        MicroMinotari::zero(),
        TxType::PaymentToOther,
    )
    .unwrap();
    let expected_fee = production_estimate(&output_features, &measured_memo, fee_per_gram);
    let real_memo = addressed_output_memo(payment_id, wallet_address, expected_fee, TxType::PaymentToOther).unwrap();
    assert_eq!(
        measured_memo.get_size(),
        real_memo.get_size(),
        "the fee is a fixed width field, so measuring the memo with a zero fee must be safe"
    );

    assert_estimate_matches_and_memo_is_load_bearing(&key_manager, value, fee_per_gram, output_features, real_memo)
        .await;
}

/// Covers the `PrepareWithdrawMultisigTransaction` estimate in the transaction service, which spends a whole
/// multisig input to a single recipient with no change output. Its output memo is the recipient address attached
/// to an empty payment id, again padded to the 130 byte `AddressAndData` minimum.
///
/// The path needs collected multisig signatures over a real UTXO, so as above this exercises the production
/// estimate and memo constructor it uses against a real transaction of the same shape.
/// `MultisigSession::spend_multisig_utxo` builds a bit-for-bit identical estimate; it is driven end to end by
/// `multisig::session::test::spend_multisig_utxo_fee_estimate_counts_the_output_memo`.
#[tokio::test]
async fn multisig_withdraw_fee_estimate_counts_the_output_memo() {
    let key_manager = create_new_random_key_manager().await.unwrap();
    let fee_per_gram = MicroMinotari::from(1);
    let recipient_address = random_dual_address();
    let output_features = OutputFeatures::default();

    let measured_memo = addressed_output_memo(
        MemoField::default(),
        recipient_address.clone(),
        MicroMinotari::zero(),
        TxType::PaymentToOther,
    )
    .unwrap();
    let expected_fee = production_estimate(&output_features, &measured_memo, fee_per_gram);
    let real_memo = addressed_output_memo(
        MemoField::default(),
        recipient_address,
        expected_fee,
        TxType::PaymentToOther,
    )
    .unwrap();
    assert_eq!(measured_memo.get_size(), real_memo.get_size());

    assert_estimate_matches_and_memo_is_load_bearing(
        &key_manager,
        MicroMinotari::from(2_000_000),
        fee_per_gram,
        output_features,
        real_memo,
    )
    .await;
}

#[tokio::test]
async fn test_maturity_greater_than_i64_max_balance_and_coin_selection() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let scanned_block = ScannedBlock {
        header_hash: HashOutput::zero(),
        height: 100,
        timestamp: Utc::now().naive_utc(),
    };
    backend.save_last_scanned_height(scanned_block).unwrap();
    let (mut oms, _shutdown, _, _, _, key_manager) = setup_oms_with_bn_state(backend.clone()).await;

    let amount = MicroMinotari::from(5000);
    // Maturity greater than i64::MAX (GHSA-f5fr-v7h5-w6q7)
    let overflow_maturity = (i64::MAX as u64) + 1000;
    let uo = make_input_with_features(
        &mut rand::rng().clone(),
        amount,
        OutputFeatures {
            maturity: overflow_maturity,
            ..Default::default()
        },
        key_manager.key_manager(),
    );
    oms.add_output(uo.clone(), None).await.unwrap();
    backend.mark_outputs_as_unspent(vec![(uo.output_hash(), true)]).unwrap();

    let balance = oms.get_balance().await.unwrap();
    // Must NOT appear in available_balance
    assert_eq!(balance.available_balance, MicroMinotari::zero());
    // Must appear in time_locked_balance
    assert_eq!(balance.time_locked_balance, Some(amount));

    // Assert it is never returned by coin selection at tip height 100
    let err = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(2),
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));

    // Update scanned block height to a very high height and assert it still is never selected
    let high_block = ScannedBlock {
        header_hash: HashOutput::zero(),
        height: 10_000_000,
        timestamp: Utc::now().naive_utc(),
    };
    backend.save_last_scanned_height(high_block).unwrap();
    let balance_high = oms.get_balance().await.unwrap();
    assert_eq!(balance_high.available_balance, MicroMinotari::zero());
    assert_eq!(balance_high.time_locked_balance, Some(amount));

    let err_high = oms
        .prepare_transaction_to_send(
            TxId::new_random(),
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(2),
            script!(Nop).unwrap(),
            Covenant::default(),
            MemoField::new_empty(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err_high, OutputManagerError::NotEnoughFunds));
}

#[tokio::test]
async fn test_rescan_with_commitment_blocklisted_produces_wallet_db_without_commitment() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection.clone());
    let (mut oms, _shutdown, _, _, _, key_manager) = setup_oms_with_bn_state(backend.clone()).await;

    let amount = MicroMinotari::from(10_000);
    // Create commitment X (to be blocklisted)
    let uo_x = make_input(
        &mut rand::rng().clone(),
        amount,
        &OutputFeatures::default(),
        key_manager.key_manager(),
    );
    let tx_output_x = uo_x.to_transaction_output().unwrap();
    let commitment_x = tx_output_x.commitment.clone();

    // Create normal commitment Y (not blocklisted)
    let uo_y = make_input(
        &mut rand::rng().clone(),
        amount,
        &OutputFeatures::default(),
        key_manager.key_manager(),
    );
    let tx_output_y = uo_y.to_transaction_output().unwrap();
    let commitment_y = tx_output_y.commitment.clone();

    // Configure WalletConfig with commitment X blocklisted
    let mut wallet_config = minotari_wallet::WalletConfig::default();
    use tari_utilities::hex::Hex;
    wallet_config.excluded_commitments = vec![commitment_x.to_hex()].into();
    let excluded = wallet_config.get_excluded_commitments().unwrap();
    assert_eq!(excluded, vec![commitment_x.clone()]);

    // The candidate wallet outputs to rescan
    let candidate_wallet_outputs = vec![uo_x.clone(), uo_y.clone()];

    // Rescan / recovery filter logic as executed by UtxoScannerTask:
    // Any outputs matching excluded_commitments are skipped from import
    for uo in candidate_wallet_outputs {
        let tx_out = uo.to_transaction_output().unwrap();
        if !excluded.contains(&tx_out.commitment) {
            oms.add_output(uo.clone(), None).await.unwrap();
            backend.mark_outputs_as_unspent(vec![(uo.output_hash(), true)]).unwrap();
        }
    }

    // Verify wallet DB state after rescan:
    // Wallet DB contains commitment Y, and does NOT contain commitment X
    let unspent = oms.get_unspent_outputs().await.unwrap();
    assert_eq!(unspent.len(), 1);
    assert!(unspent.iter().any(|u| u.commitment == commitment_y));
    assert!(!unspent.iter().any(|u| u.commitment == commitment_x));
}


