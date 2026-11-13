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

pub const SAFETY_HEIGHT_MARGIN: u64 = 100;

use std::sync::Arc;

use log::*;
use tari_common_types::types::FixedHash;
use tari_transaction_key_manager::legacy_key_manager::LegacyTransactionKeyManagerInterface;

use crate::{
    OperationId,
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::{TransactionEvent, TransactionEventSender},
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::CompletedTransaction,
        },
    },
};
const LOG_TARGET: &str = "wallet::transaction_service::protocols::validation_protocol";

#[allow(clippy::too_many_lines)]
pub async fn check_detected_transactions<
    TBackend: 'static + TransactionBackend,
    KM: LegacyTransactionKeyManagerInterface,
>(
    mut output_manager: OutputManagerHandle<KM>,
    db: TransactionDatabase<TBackend>,
    event_publisher: TransactionEventSender,
    config: TransactionServiceConfig,
    tip_height: u64,
) {
    debug!(target: LOG_TARGET, "Checking detected (onesided, imported, coinbase unconf) transactions");
    // Reorged faux transactions cannot be detected by excess signature, thus use last known confirmed transaction
    // height or current tip height with safety margin to determine if these should be returned
    let last_mined_transaction = db.fetch_last_mined_transaction().unwrap_or_default();
    let check_height = if let Some(tx) = last_mined_transaction {
        tx.mined_height
            .unwrap_or(tip_height)
            .saturating_sub(SAFETY_HEIGHT_MARGIN)
    } else {
        tip_height.saturating_sub(SAFETY_HEIGHT_MARGIN)
    };

    let mut all_detected_transactions: Vec<CompletedTransaction> = match db.get_imported_transactions() {
        Ok(txs) => txs,
        Err(e) => {
            error!(target: LOG_TARGET, "Problem retrieving imported transactions: {e}");
            Vec::new()
        },
    };
    debug!(
        target: LOG_TARGET,
        "Checking {} imported transactions",
        all_detected_transactions.len()
    );
    let mut unconfirmed_detected = match db.get_unconfirmed_detected_transactions() {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving unconfirmed detected transactions: {e}"
            );
            Vec::new()
        },
    };
    debug!(
        target: LOG_TARGET,
        "Checking {} unconfirmed detected transactions",
        unconfirmed_detected.len()
    );
    all_detected_transactions.append(&mut unconfirmed_detected);

    let mut unmined_coinbases_detected = match db.get_unmined_coinbase_transactions(check_height) {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving unmined coinbase transactions: {e}"
            );
            Vec::new()
        },
    };
    debug!(
        target: LOG_TARGET,
        "Checking {} unmined coinbase detected transactions",
        unmined_coinbases_detected.len()
    );
    all_detected_transactions.append(&mut unmined_coinbases_detected);

    let mut confirmed_dectected = match db.get_confirmed_detected_transactions_from_height(check_height) {
        Ok(txs) => txs,
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "Problem retrieving confirmed detected transactions: {e}"
            );
            Vec::new()
        },
    };
    debug!(
        target: LOG_TARGET,
        "Checking {} confirmed detected transactions",
        confirmed_dectected.len()
    );
    all_detected_transactions.append(&mut confirmed_dectected);

    let mut state_changed = false;
    for tx in all_detected_transactions {
        let output_info_for_tx_id = match output_manager.get_output_info_for_tx_id(tx.tx_id).await {
            Ok(s) => s,
            Err(e) => {
                error!(target: LOG_TARGET, "Problem retrieving output statuses for TxId {}: {e}", tx.tx_id);
                continue;
            },
        };
        debug!(
            target: LOG_TARGET,
            "TxId: {}, {:?} ",
            tx.tx_id, output_info_for_tx_id
        );
        let mined_location_changed = has_mined_location_changed(
            tx.mined_height,
            tx.mined_in_block,
            output_info_for_tx_id.mined_height,
            output_info_for_tx_id.block_hash,
        );
        if mined_location_changed {
            // If the mined height or block hash has changed, we need to update the transaction
            state_changed = true;
        }
        trace!(
            target: LOG_TARGET,
            "TxId: {}, {:?} ",
            tx.tx_id, output_info_for_tx_id
        );
        if output_info_for_tx_id.mined_height.is_none() || output_info_for_tx_id.block_hash.is_none() {
            // this means the transaction is not detected as mined
            if let Err(e) = db.set_transaction_as_unmined(tx.tx_id) {
                error!(
                    target: LOG_TARGET,
                    "Error setting faux transaction to unmined: {e}"
                );
            }
            continue;
        }
        // Its safe to assume that statuses should be the same as they are all in the same transaction and they cannot
        // be different.
        let Some(output_status) = output_info_for_tx_id.statuses.first() else {
            error!(target: LOG_TARGET, "TxId {}: no output statuses", tx.tx_id);
            continue;
        };
        let mined_height = output_info_for_tx_id.mined_height.unwrap_or(0);
        let mined_in_block = output_info_for_tx_id.block_hash.unwrap_or(FixedHash::zero());
        let is_valid = tip_height >= mined_height;
        let previously_confirmed = tx.status.is_confirmed() && tx.mined_height.is_some() && tx.mined_in_block.is_some();
        let must_be_confirmed = tip_height.saturating_sub(mined_height) >= config.num_confirmations_required;
        let previously_locked = tx.status.is_locked();
        let must_be_unlocked = tip_height >= tx.lock_height;

        if should_update_detected_transaction(
            previously_confirmed,
            must_be_confirmed,
            previously_locked,
            must_be_unlocked,
            mined_location_changed,
        ) {
            let log_msg = format!(
                "Updating faux transaction: TxId({}), mined_height({mined_height}), \
                 must_be_confirmed({must_be_confirmed}), output_status({output_status}), is_valid({is_valid})",
                tx.tx_id
            );
            if must_be_confirmed {
                debug!(target: LOG_TARGET, "{log_msg}");
            } else {
                trace!(target: LOG_TARGET, "{log_msg}");
            }

            let result = db.set_transaction_mined_height(
                tx.tx_id,
                mined_height,
                mined_in_block,
                tx.mined_timestamp
                    .map_or(0, |mined_timestamp| mined_timestamp.timestamp() as u64),
                must_be_confirmed,
                tx.status,
                tip_height,
            );
            if let Err(e) = result {
                error!(
                    target: LOG_TARGET,
                    "Error setting faux transaction to mined confirmed: {e}"
                );
                continue;
            }
            let transaction_event = if must_be_confirmed {
                TransactionEvent::DetectedTransactionConfirmed {
                    tx_id: tx.tx_id,
                    is_valid,
                }
            } else {
                TransactionEvent::DetectedTransactionUnconfirmed {
                    tx_id: tx.tx_id,
                    num_confirmations: 0,
                    is_valid,
                }
            };
            let _size = event_publisher.send(Arc::new(transaction_event)).inspect_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {e:?}"
                );
            });
        }
    }
    if state_changed {
        let _size = event_publisher
            .send(Arc::new(TransactionEvent::TransactionValidationStateChanged {
                faux: true,
                id: OperationId::new_random(),
            }))
            .inspect_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {e:?}",
                );
            });
    }
}

fn has_mined_location_changed(
    tx_mined_height: Option<u64>,
    tx_mined_in_block: Option<FixedHash>,
    output_mined_height: Option<u64>,
    output_block_hash: Option<FixedHash>,
) -> bool {
    tx_mined_height != output_mined_height || tx_mined_in_block != output_block_hash
}

fn should_update_detected_transaction(
    previously_confirmed: bool,
    must_be_confirmed: bool,
    previously_locked: bool,
    must_be_unlocked: bool,
    mined_location_changed: bool,
) -> bool {
    !(previously_confirmed && must_be_confirmed) || (previously_locked && must_be_unlocked) || mined_location_changed
}

#[cfg(test)]
mod test {
    use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
    use chrono::Utc;
    use futures::StreamExt;
    use rand::rngs::OsRng;
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::TariAddress,
        transaction::{LegacyTransactionStatus, TransactionDirection, TxId},
        types::{CompressedPublicKey, CompressedSignature, FixedHash, PrivateKey},
    };
    use tari_crypto::keys::SecretKey;
    use tari_service_framework::reply_channel;
    use tari_transaction_components::{
        MicroMinotari,
        transaction_components::{MemoField, Transaction, memo_field::TxType},
    };
    use tari_transaction_key_manager::legacy_key_manager::MemoryDbKeyManager;
    use tokio::sync::broadcast;

    use super::{check_detected_transactions, has_mined_location_changed, should_update_detected_transaction};
    use crate::{
        output_manager_service::{
            handle::{OutputInfoByTxId, OutputManagerHandle, OutputManagerRequest, OutputManagerResponse},
            storage::OutputStatus,
        },
        storage::sqlite_utilities::run_migration_and_create_sqlite_memory_connection,
        transaction_service::{
            config::TransactionServiceConfig,
            storage::{
                database::TransactionDatabase,
                models::CompletedTransaction,
                sqlite_db::TransactionServiceSqliteDatabase,
            },
        },
    };

    #[test]
    fn reorged_confirmed_detected_tx_updates_when_mined_location_changes() {
        let old_hash = FixedHash::from([0x11u8; 32]);
        let new_hash = FixedHash::from([0x22u8; 32]);

        // Re-mined at a different height or different block hash while still meeting confirmations
        assert!(has_mined_location_changed(
            Some(100),
            Some(old_hash),
            Some(101),
            Some(new_hash),
        ));
        assert!(has_mined_location_changed(
            Some(100),
            Some(old_hash),
            Some(100),
            Some(new_hash),
        ));
        assert!(should_update_detected_transaction(true, true, false, true, true));

        // Unchanged confirmed unlocked transaction does not need a redundant DB write
        assert!(!has_mined_location_changed(
            Some(100),
            Some(old_hash),
            Some(100),
            Some(old_hash),
        ));
        assert!(!should_update_detected_transaction(true, true, false, true, false));
    }

    #[tokio::test]
    async fn check_detected_transactions_persists_reorged_mined_location() {
        let connection = run_migration_and_create_sqlite_memory_connection().unwrap();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&[7u8; 32]));
        let db = TransactionDatabase::new(TransactionServiceSqliteDatabase::new(connection, cipher));

        let address = TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        )
        .unwrap();
        let tx_id = TxId::from(42u64);
        let initial_hash = FixedHash::from([0x11u8; 32]);
        let same_height_reorg_hash = FixedHash::from([0x22u8; 32]);
        let new_height_reorg_hash = FixedHash::from([0x33u8; 32]);

        let completed_tx = CompletedTransaction {
            tx_id,
            source_address: address.clone(),
            destination_address: address,
            amount: MicroMinotari::from(1_000),
            fee: MicroMinotari::from(10),
            transaction: Transaction::new(vec![], vec![], vec![], PrivateKey::default(), PrivateKey::default()),
            status: LegacyTransactionStatus::OneSidedConfirmed,
            timestamp: Utc::now(),
            cancelled: None,
            direction: TransactionDirection::Inbound,
            send_count: 0,
            last_send_timestamp: None,
            sent_output_hashes: vec![],
            received_output_hashes: vec![FixedHash::from([0xAAu8; 32])],
            change_output_hashes: vec![],
            transaction_signature: CompressedSignature::default(),
            mined_height: Some(100),
            mined_in_block: Some(initial_hash),
            mined_timestamp: Some(Utc::now()),
            payment_id: MemoField::new_open_from_string("test", TxType::PaymentToOther).unwrap(),
            lock_height: 0,
            rejection_reason: None,
            source_address_claimed: false,
        };
        db.insert_completed_transaction(tx_id, completed_tx).unwrap();

        let (om_sender, mut om_receiver) = reply_channel::unbounded();
        let (om_event_tx, _) = broadcast::channel(10);
        let output_manager = OutputManagerHandle::<MemoryDbKeyManager>::new(om_sender, om_event_tx);

        let responses = vec![(100u64, same_height_reorg_hash), (101u64, new_height_reorg_hash)];
        tokio::spawn(async move {
            for (height, block_hash) in responses {
                if let Some(req_ctx) = om_receiver.next().await {
                    let (req, reply_tx) = req_ctx.split();
                    if let OutputManagerRequest::GetOutputInfoByTxId(id) = req {
                        assert_eq!(id, tx_id);
                        let _ = reply_tx.send(Ok(OutputManagerResponse::OutputInfoByTxId(OutputInfoByTxId {
                            statuses: vec![OutputStatus::Unspent],
                            mined_height: Some(height),
                            block_hash: Some(block_hash),
                        })));
                    }
                }
            }
        });

        let (event_tx, _event_rx) = broadcast::channel(10);
        let config = TransactionServiceConfig::default();

        // 1. Block-hash-only change at height 100 while still confirmed at tip 110
        check_detected_transactions(
            output_manager.clone(),
            db.clone(),
            event_tx.clone(),
            config.clone(),
            110,
        )
        .await;
        let saved = db.get_completed_transaction(tx_id).unwrap();
        assert_eq!(saved.mined_height, Some(100));
        assert_eq!(saved.mined_in_block, Some(same_height_reorg_hash));

        // 2. Both mined_height and mined_in_block change while still confirmed at tip 110
        check_detected_transactions(output_manager, db.clone(), event_tx, config, 110).await;
        let saved = db.get_completed_transaction(tx_id).unwrap();
        assert_eq!(saved.mined_height, Some(101));
        assert_eq!(saved.mined_in_block, Some(new_height_reorg_hash));
    }
}
