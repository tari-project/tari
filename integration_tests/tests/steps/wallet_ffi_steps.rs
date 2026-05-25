//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{convert::TryFrom, ptr::null, time::Duration};

use cucumber::{given, then, when};
use minotari_app_grpc::tari_rpc::GetBalanceResponse;
use tari_common_types::tari_address::TariAddress;
use tari_integration_tests::{
    DEFAULT_TIMEOUT,
    FfiConnectivityStatus,
    SHORT_TIMEOUT,
    TariWorld,
    wait_for,
    wallet_ffi::{create_seed_words, get_mnemonic_word_list_for_language, spawn_wallet_ffi},
};
use tari_transaction_components::transaction_components::memo_field::{MemoField, TxType};

use crate::steps::{cucumber_steps_log, get_saved_seed_words};

#[when(expr = "I have a ffi wallet {word} connected to base node {word}")]
#[then(expr = "I have a ffi wallet {word} connected to base node {word}")]
#[given(expr = "I have a ffi wallet {word} connected to base node {word}")]
async fn ffi_start_wallet_connected_to_base_node(world: &mut TariWorld, wallet: String, base_node: String) {
    let http_port = world.base_nodes.get(&base_node).unwrap().http_port;
    let address = format!("http://127.0.0.1:{http_port}");
    spawn_wallet_ffi(world, wallet.clone(), null(), address);
}

#[given(expr = "I have a ffi wallet {word} connected to seed node {word}")]
async fn ffi_start_wallet_connected_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    let http_port = world.base_nodes.get(&seed_node).unwrap().http_port;
    let address = format!("http://127.0.0.1:{http_port}");
    spawn_wallet_ffi(world, wallet.clone(), null(), address);
}

#[then(expr = "I want to get public key of ffi wallet {word}")]
async fn ffi_get_public_key(world: &mut TariWorld, wallet: String) {
    let wallet = world.get_ffi_wallet(&wallet).unwrap();
    let public_key = wallet.identify();
    cucumber_steps_log(format!("wallet: {}, public_key: {}", wallet.name, public_key));
}

#[then(expr = "I want to get emoji id of ffi wallet {word}")]
async fn ffi_get_emoji_id(world: &mut TariWorld, wallet: String) {
    let wallet = world.get_ffi_wallet(&wallet).unwrap();
    let emoji_id = wallet.get_emoji_id();
    assert!(TariAddress::from_emoji_string(&emoji_id).is_ok());
}

#[when(expr = "I stop ffi wallet {word}")]
#[then(expr = "I stop ffi wallet {word}")]
async fn ffi_stop_wallet(world: &mut TariWorld, wallet: String) {
    let address = world.get_wallet_address(&wallet).await.unwrap();
    let ffi_wallet = world.ffi_wallets.get_mut(&wallet).unwrap();
    cucumber_steps_log(format!("Adding wallet {wallet}"));
    world.wallet_addresses.insert(wallet, address);
    ffi_wallet.destroy();
}

#[then(expr = "I restart ffi wallet {word}")]
#[when(expr = "I restart ffi wallet {word}")]
async fn ffi_restart_wallet(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_mut_ffi_wallet(&wallet).unwrap();
    ffi_wallet.restart();
}

#[then(expr = "I retrieve the mnemonic word list for {word}")]
async fn ffi_retrieve_mnemonic_words(_world: &mut TariWorld, language: String) {
    cucumber_steps_log(format!("Mnemonic words for language {language}:"));
    let words = get_mnemonic_word_list_for_language(language);
    for i in 0..words.get_length() {
        cucumber_steps_log(format!("{} ", words.get_at(u32::try_from(i).unwrap()).as_string()));
    }
    assert_eq!(words.get_length(), 2048);
}

#[then(expr = "I wait for ffi wallet {word} to have connectivity")]
async fn ffi_wait_wallet_to_connect(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    wait_for!(
        timeout: SHORT_TIMEOUT,
        description: format!("FFI wallet {wallet} to have connectivity"),
        condition: async {
            let status = ffi_wallet.get_connectivity_status();
            if status.0 == FfiConnectivityStatus::Online || status.0 == FfiConnectivityStatus::Degraded {
                Ok(true)
            } else {
                Err(format!("status: {:?}", status.0))
            }
        }
    );
}

#[then(expr = "I wait for ffi wallet {word} to have at least {int} uT")]
#[when(expr = "I wait for ffi wallet {word} to have at least {int} uT")]
async fn ffi_wait_for_balance(world: &mut TariWorld, wallet: String, amount: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let wallet_name = ffi_wallet.name.clone();
    let wallet_id = ffi_wallet.id.clone();
    wait_for!(
        timeout: SHORT_TIMEOUT,
        description: format!("FFI wallet {wallet} to have at least {amount} uT available"),
        condition: async {
            let ffi_balance = ffi_wallet.get_balance();
            if ffi_balance.get_available() >= amount {
                Ok(true)
            } else {
                Err(format!(
                    "wallet {}:{}, needs available {}, has balance: available {} incoming {} time locked {}",
                    wallet_name,
                    wallet_id,
                    amount,
                    ffi_balance.get_available(),
                    ffi_balance.get_pending_incoming(),
                    ffi_balance.get_time_locked()
                ))
            }
        }
    );
}

#[then(expr = "ffi wallet {word} balance is {word}")]
async fn ffi_has_balance(world: &mut TariWorld, wallet: String, balance_key: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let balance = world.balance.get(&balance_key).unwrap().clone();
    let wallet_name = ffi_wallet.name.clone();
    let wallet_id = ffi_wallet.id.clone();
    ffi_wallet.start_txo_validation();
    wait_for!(
        timeout: SHORT_TIMEOUT,
        description: format!("FFI wallet {wallet} balance to match {balance_key}"),
        condition: async {
            let ffi_balance = ffi_wallet.get_balance();
            let ffi_wallet_balance = GetBalanceResponse {
                available_balance: ffi_balance.get_available(),
                pending_incoming_balance: ffi_balance.get_pending_incoming(),
                timelocked_balance: ffi_balance.get_time_locked(),
                pending_outgoing_balance: ffi_balance.get_pending_outgoing(),
            };
            if ffi_wallet_balance == balance {
                cucumber_steps_log(format!(
                    "Wallet {}:{} waiting for balance to be {:?} (DONE), current {:?}",
                    wallet_name, wallet_id, balance, ffi_wallet_balance
                ));
                Ok(true)
            } else {
                Err(format!(
                    "Wallet {}:{} waiting for balance to be {:?}, current {:?}",
                    wallet_name, wallet_id, balance, ffi_wallet_balance
                ))
            }
        }
    );
}

#[when(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}")]
async fn ffi_send_transaction(world: &mut TariWorld, amount: u64, wallet: String, dest: String, fee: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let dest_pub_key = world.get_wallet_address(&dest).await.unwrap();
    let payment_id = MemoField::new_open_from_string(
        &format!("Send from ffi {wallet} to ${dest} at fee ${fee}"),
        TxType::PaymentToOther,
    )
    .unwrap();
    let tx_id = ffi_wallet.send_transaction(dest_pub_key, amount, fee, payment_id);
    assert_ne!(tx_id, 0, "Send transaction was not successful");
}

#[when(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions")]
#[then(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions")]
async fn ffi_send_one_sided_transaction(world: &mut TariWorld, amount: u64, wallet: String, dest: String, fee: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let dest_pub_key = world.get_wallet_one_sided_address(&dest).await.unwrap();
    let payment_id = MemoField::new_open_from_string(
        &format!("Send from ffi {wallet} to ${dest} at fee ${fee}"),
        TxType::PaymentToOther,
    )
    .unwrap();
    let tx_id = ffi_wallet.send_transaction(dest_pub_key, amount, fee, payment_id);
    assert_ne!(tx_id, 0, "Send transaction was not successful");
}

#[when(expr = "I have {int} received and {int} send transaction in ffi wallet {word}")]
#[then(expr = "I have {int} received and {int} send transaction in ffi wallet {word}")]
async fn ffi_check_number_of_transactions(world: &mut TariWorld, received: u32, send: u32, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let inbound_txs = ffi_wallet.get_pending_inbound_transactions();
    let mut inbound_cnt = inbound_txs.get_length();
    let outbound_txs = ffi_wallet.get_pending_outbound_transactions();
    let mut outbound_cnt = outbound_txs.get_length();
    let completed_txs = ffi_wallet.get_completed_transactions();
    for i in 0..completed_txs.get_length() {
        let completed_tx = completed_txs.get_at(i);
        if completed_tx.is_outbound() {
            outbound_cnt += 1;
        } else {
            inbound_cnt += 1;
        }
    }
    assert_eq!(outbound_cnt, send);
    assert_eq!(inbound_cnt, received);
}

#[then(expr = "I wait for ffi wallet {word} to have {int} pending outbound transaction(s)")]
async fn ffi_check_number_of_outbound_transactions(world: &mut TariWorld, wallet: String, cnt: u32) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("FFI wallet {wallet} to have {cnt} pending outbound transaction(s)"),
        condition: async {
            let pending_outbound_transactions = ffi_wallet.get_pending_outbound_transactions();
            let found_cnt = pending_outbound_transactions.get_length();
            if found_cnt >= cnt {
                Ok(true)
            } else {
                Err(format!("found {found_cnt} pending outbound transactions, expected {cnt}"))
            }
        }
    );
}

#[then(expr = "I want to view the transaction information for completed transactions in ffi wallet {word}")]
async fn ffi_view_transaction_kernels_for_completed(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let completed_transactions = ffi_wallet.get_completed_transactions();
    for i in 0..completed_transactions.get_length() {
        let completed_transaction = completed_transactions.get_at(i);
        let is_outbound = completed_transaction.is_outbound();
        // Received one-sided transactions are stored without kernel data (only the scanned output is stored),
        // so kernel checks only apply to outbound transactions.
        if is_outbound {
            let kernel = completed_transaction.get_transaction_kernel();
            cucumber_steps_log(format!("Wallet {wallet}, Transaction kernel info :"));
            assert!(!kernel.get_excess_hex().is_empty());
            cucumber_steps_log(format!("Wallet {wallet}, Excess {}", kernel.get_excess_hex()));
            assert!(!kernel.get_excess_public_nonce_hex().is_empty());
            cucumber_steps_log(format!(
                "Wallet {}, Nonce {}",
                wallet,
                kernel.get_excess_public_nonce_hex()
            ));
            assert!(!kernel.get_excess_signature_hex().is_empty());
            cucumber_steps_log(format!(
                "Wallet {}, Signature {}",
                wallet,
                kernel.get_excess_signature_hex()
            ));
        }
        let address = completed_transaction.get_destination_tari_address();
        assert!(TariAddress::from_hex(&address.address().get_as_hex()).is_ok());
        let address = completed_transaction.get_source_tari_address();
        assert!(TariAddress::from_hex(&address.address().get_as_hex()).is_ok());
        let amount = completed_transaction.get_amount();
        assert!(amount > 0, "Amount '{amount}', expected > 0");
        let fee = completed_transaction.get_fee();
        assert!(fee > 0, "Fee '{fee}', expected > 0");
        let timestamp = completed_transaction.get_timestamp();
        assert!(timestamp > 0, "Timestamp '{timestamp}', expected > 0");
        let payment_id = completed_transaction.get_payment_id();
        assert!(!payment_id.is_empty(), "Payment id '{payment_id}', expected not empty");
        let transaction_type = completed_transaction.get_transaction_type();
        assert_ne!(
            transaction_type, 99,
            "Transaction type '{transaction_type}', expected not 99"
        );
        let status = completed_transaction.get_status();
        assert_ne!(status, -1, "Status '{status}', expected not -1");

        let cancellation_reason = completed_transaction.get_cancellation_reason();
        assert!(
            if status == 6 { cancellation_reason == -1 } else { true },
            "Cancellation reason '{cancellation_reason}' (with status '{status}'), expected -1"
        );
    }
}

#[then(expr = "I cancel all outbound transactions on ffi wallet {word} and it will cancel {int} transaction")]
async fn ffi_cancel_outbound_transactions(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let pending_outbound_transactions = ffi_wallet.get_pending_outbound_transactions();
    let mut cancelled = 0;
    for i in 0..pending_outbound_transactions.get_length() {
        let pending_outbound_transaction = pending_outbound_transactions.get_at(i);
        if ffi_wallet.cancel_pending_transaction(pending_outbound_transaction.get_transaction_id()) {
            cancelled += 1;
        }
    }
    assert_eq!(cancelled, cnt);
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} transaction")]
async fn ffi_wait_for_transaction_received(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("FFI wallet {wallet} to receive {cnt} transaction(s)"),
        condition: async {
            let found_cnt = ffi_wallet.get_counters().get_transaction_received();
            if found_cnt >= cnt {
                Ok(true)
            } else {
                Err(format!("received {found_cnt}, expected {cnt}"))
            }
        }
    );
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} finalization")]
async fn ffi_wait_for_transaction_finalized(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("FFI wallet {wallet} to receive {cnt} finalization(s)"),
        condition: async {
            let found_cnt = ffi_wallet.get_counters().get_transaction_finalized();
            if found_cnt >= cnt {
                Ok(true)
            } else {
                Err(format!("finalized {found_cnt}, expected {cnt}"))
            }
        }
    );
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} broadcast")]
async fn ffi_wait_for_transaction_broadcast(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("FFI wallet {wallet} to receive {cnt} broadcast(s)"),
        condition: async {
            let found_cnt = ffi_wallet.get_counters().get_transaction_broadcast();
            if found_cnt >= cnt {
                Ok(true)
            } else {
                Err(format!("broadcast {found_cnt}, expected {cnt}"))
            }
        }
    );
}

#[then(expr = "I start TXO validation on ffi wallet {word}")]
async fn ffi_start_txo_validation(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    // Reset flags before triggering validation so we don't pick up a stale result
    ffi_wallet.get_counters().reset_txo_validation();
    ffi_wallet.start_txo_validation();
    wait_for!(
        timeout: Duration::from_secs(240),
        description: format!("TXO validation on FFI wallet {wallet} to complete"),
        condition: async {
            if ffi_wallet.get_counters().get_txo_validation_complete() {
                if ffi_wallet.get_counters().get_txo_validation_result() == 0 {
                    // result=0 means success; validation ran to completion
                    Ok(true)
                } else {
                    // result=1 means AlreadyBusy (another validation is running), result>=2 means failure.
                    // Reset and wait for the in-flight validation to complete and fire its own callback.
                    ffi_wallet.get_counters().reset_txo_validation();
                    Err(format!("TXO validation result was {}, resetting", ffi_wallet.get_counters().get_txo_validation_result()))
                }
            } else {
                Err("TXO validation not yet complete".to_string())
            }
        }
    );
}

#[when(expr = "I wait for ffi wallet {word} to have scanned to height {int}")]
async fn wait_for_ffi_wallet_scanned_height(world: &mut TariWorld, wallet: String, height: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    wait_for!(
        timeout: Duration::from_secs(240),
        description: format!("FFI wallet {wallet} to scan to height {height}"),
        condition: async {
            let scanned = ffi_wallet.get_counters().get_scanned_height();
            if scanned >= height {
                Ok(true)
            } else {
                Err(format!("scanned to {scanned}, expected {height}"))
            }
        }
    );
}

#[then(expr = "I start TX validation on ffi wallet {word}")]
async fn ffi_start_tx_validation(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.start_transaction_validation();
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("TX validation on FFI wallet {wallet} to complete"),
        condition: async {
            if ffi_wallet.get_counters().get_tx_validation_complete() {
                Ok(true)
            } else {
                Err("TX validation not yet complete".to_string())
            }
        }
    );
}

#[then(expr = "ffi wallet {word} detects {word} {int} ffi transactions to be {word}")]
async fn ffi_detects_transaction(
    world: &mut TariWorld,
    wallet: String,
    comparison: String,
    count: u64,
    status: String,
) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    assert!(
        [
            "TRANSACTION_STATUS_BROADCAST",
            "TRANSACTION_STATUS_MINED_UNCONFIRMED",
            "TRANSACTION_STATUS_MINED",
            "TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED",
            "TRANSACTION_STATUS_ONE_SIDED_CONFIRMED"
        ]
        .contains(&status.as_str())
    );
    cucumber_steps_log(format!(
        "Waiting for {wallet} to have detected {comparison} {count} {status} transaction(s)"
    ));
    let status_clone = status.clone();
    let comparison_clone = comparison.clone();
    wait_for!(
        timeout: Duration::from_secs(600),
        description: format!("FFI wallet {wallet} to detect {comparison} {count} {status} transaction(s)"),
        condition: async {
            let found_count = match status_clone.as_str() {
                "TRANSACTION_STATUS_BROADCAST" => ffi_wallet.get_counters().get_transaction_broadcast(),
                "TRANSACTION_STATUS_MINED_UNCONFIRMED" => ffi_wallet.get_counters().get_transaction_mined_unconfirmed(),
                "TRANSACTION_STATUS_MINED" => ffi_wallet.get_counters().get_transaction_mined(),
                "TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED" => {
                    let mut c = ffi_wallet.get_counters().get_transaction_faux_unconfirmed();
                    c += ffi_wallet.get_counters().get_transaction_mined_unconfirmed();
                    c
                },
                "TRANSACTION_STATUS_ONE_SIDED_CONFIRMED" => {
                    let mut c = ffi_wallet.get_counters().get_transaction_faux_confirmed();
                    c += ffi_wallet.get_counters().get_transaction_mined();
                    c
                },
                _ => unreachable!(),
            };
            let met = match comparison_clone.as_str() {
                "AT_LEAST" => found_count >= count,
                "EXACTLY" => found_count == count,
                _ => panic!("Unknown comparison method {}", comparison_clone),
            };
            if met {
                cucumber_steps_log(format!("Counters {:?}", ffi_wallet.get_counters()));
                Ok(true)
            } else {
                Err(format!("found_count: {found_count}, expected {comparison_clone} {count}"))
            }
        }
    );
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} mined")]
async fn ffi_wait_for_received_mined(world: &mut TariWorld, wallet: String, count: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    cucumber_steps_log(format!("Waiting for {wallet} to receive {count} transaction(s) mined"));
    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("FFI wallet {wallet} to receive {count} mined transaction(s)"),
        condition: async {
            let found_cnt = ffi_wallet.get_counters().get_transaction_mined();
            if found_cnt >= count {
                Ok(true)
            } else {
                Err(format!("mined {found_cnt}, expected {count}"))
            }
        }
    );
}

#[then(expr = "I recover wallet {word} into ffi wallet {word} from seed words on node {word}")]
async fn recover_wallet_into_ffi_wallet(
    world: &mut TariWorld,
    source_wallet: String,
    ffi_wallet: String,
    node: String,
) {
    let saved_words = get_saved_seed_words(world, &source_wallet);
    let words_ref: Vec<&str> = saved_words.iter().map(String::as_str).collect();
    let seed_words = create_seed_words(words_ref);
    let seed_words_ptr = seed_words.get_ptr() as *const std::ffi::c_void;
    let http_port = world.base_nodes.get(&node).unwrap().http_port;
    let address = format!("http://127.0.0.1:{http_port}");
    spawn_wallet_ffi(world, ffi_wallet, seed_words_ptr, address);
}

#[then(expr = "The fee per gram stats for {word} are {int}, {int}, {int}")]
#[when(expr = "The fee per gram stats for {word} are {int}, {int}, {int}")]
async fn ffi_fee_per_gram_stats(world: &mut TariWorld, wallet: String, min: u64, avg: u64, max: u64) {
    let ffi_wallet = world.get_mut_ffi_wallet(&wallet).unwrap();
    let fee_per_gram_stats = ffi_wallet.get_fee_per_gram_stats(5);
    for i in 0..fee_per_gram_stats.get_length() {
        let fee_per_gram_stat = fee_per_gram_stats.get_at(i);
        cucumber_steps_log(format!("{}: order {}", wallet, fee_per_gram_stat.get_order()));
        cucumber_steps_log(format!("{}: min {}", wallet, fee_per_gram_stat.get_min_fee_per_gram()));
        cucumber_steps_log(format!("{}: avg {}", wallet, fee_per_gram_stat.get_avg_fee_per_gram()));
        cucumber_steps_log(format!("{}: max {}", wallet, fee_per_gram_stat.get_max_fee_per_gram()));
        assert_eq!(fee_per_gram_stat.get_min_fee_per_gram(), min);
        assert_eq!(fee_per_gram_stat.get_avg_fee_per_gram(), avg);
        assert_eq!(fee_per_gram_stat.get_max_fee_per_gram(), max);
    }
}
