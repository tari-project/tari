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
use tari_core::transactions::transaction_components::encrypted_data::{PaymentId, TxType};
use tari_integration_tests::{
    wallet_ffi::{create_contact, create_seed_words, get_mnemonic_word_list_for_language, spawn_wallet_ffi},
    TariWorld,
};
use tari_utilities::hex::Hex;

use crate::steps::{cucumber_steps_log, get_saved_seed_words};

#[when(expr = "I have a ffi wallet {word} connected to base node {word}")]
#[then(expr = "I have a ffi wallet {word} connected to base node {word}")]
#[given(expr = "I have a ffi wallet {word} connected to base node {word}")]
async fn ffi_start_wallet_connected_to_base_node(world: &mut TariWorld, wallet: String, base_node: String) {
    spawn_wallet_ffi(world, wallet.clone(), null());
    let base_node = world.get_node(&base_node).unwrap();
    world.get_ffi_wallet(&wallet).unwrap().add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().unwrap().to_string(),
    );
}

#[given(expr = "I have a ffi wallet {word} connected to seed node {word}")]
async fn ffi_start_wallet_connected_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    spawn_wallet_ffi(world, wallet.clone(), null());
    assert!(world.all_seed_nodes().contains(&seed_node), "Seed node not found.");
    let seed_node = world.get_node(&seed_node).unwrap();
    world.get_ffi_wallet(&wallet).unwrap().add_base_node(
        seed_node.identity.public_key().to_hex(),
        seed_node.identity.first_public_address().unwrap().to_string(),
    );
}

#[given(expr = "I set base node {word} for ffi wallet {word}")]
async fn ffi_set_base_node(world: &mut TariWorld, base_node: String, wallet: String) {
    let base_node = world.get_node(&base_node).unwrap();
    world.get_ffi_wallet(&wallet).unwrap().add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().unwrap().to_string(),
    );
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
    cucumber_steps_log(format!("Adding wallet {}", wallet));
    world.wallet_addresses.insert(wallet, address);
    ffi_wallet.destroy();
}

#[then(expr = "I retrieve the mnemonic word list for {word}")]
async fn ffi_retrieve_mnemonic_words(_world: &mut TariWorld, language: String) {
    cucumber_steps_log(format!("Mnemonic words for language {}:", language));
    let words = get_mnemonic_word_list_for_language(language);
    for i in 0..words.get_length() {
        cucumber_steps_log(format!("{} ", words.get_at(u32::try_from(i).unwrap()).as_string()));
    }
    assert_eq!(words.get_length(), 2048);
}

#[then(expr = "I wait for ffi wallet {word} to connect to {word}")]
async fn ffi_wait_wallet_to_connect(world: &mut TariWorld, wallet: String, node: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let node = world.get_node(&node).unwrap().identity.public_key();
    for _ in 0..10 {
        let public_keys = ffi_wallet.connected_public_keys();
        for i in 0..public_keys.get_length() {
            let public_key = public_keys.get_public_key_at(u32::try_from(i).unwrap());
            if public_key.get_bytes().get_as_hex() == node.to_hex() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    panic!("Wallet not connected");
}

#[then(expr = "I wait for ffi wallet {word} to have at least {int} uT")]
#[when(expr = "I wait for ffi wallet {word} to have at least {int} uT")]
async fn ffi_wait_for_balance(world: &mut TariWorld, wallet: String, amount: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let mut ffi_balance = ffi_wallet.get_balance();
    let mut cnt = 0;
    while ffi_balance.get_available() < amount && cnt < 10 {
        if cnt % 3 == 0 {
            cucumber_steps_log(format!(
                "wallet {}, port {}, needs available {}, has balance: available {} incoming {} time locked {}",
                ffi_wallet.name,
                ffi_wallet.port,
                amount,
                ffi_balance.get_available(),
                ffi_balance.get_pending_incoming(),
                ffi_balance.get_time_locked()
            ));
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        ffi_balance = ffi_wallet.get_balance();
        cnt += 1;
    }
    assert!(
        ffi_balance.get_available() >= amount,
        "Wallet {}:{} doesn't have enough available funds: available {} incoming {} time locked {}",
        ffi_wallet.name,
        ffi_wallet.port,
        ffi_balance.get_available(),
        ffi_balance.get_pending_incoming(),
        ffi_balance.get_time_locked()
    );
}

#[then(expr = "ffi wallet {word} balance is {word}")]
async fn ffi_has_balance(world: &mut TariWorld, wallet: String, balance_key: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let balance = world.balance.get(&balance_key).unwrap();
    let num_retries = 15;
    let mut ffi_wallet_balance = GetBalanceResponse::default();

    for i in 0..num_retries {
        ffi_wallet.start_transaction_validation();
        let ffi_balance = ffi_wallet.get_balance();
        ffi_wallet_balance = GetBalanceResponse {
            available_balance: ffi_balance.get_available(),
            pending_incoming_balance: ffi_balance.get_pending_incoming(),
            timelocked_balance: ffi_balance.get_time_locked(),
            pending_outgoing_balance: ffi_balance.get_pending_outgoing(),
        };
        if &ffi_wallet_balance == balance {
            cucumber_steps_log(format!(
                "Wallet {}:{} waiting for balance to be {:?} (DONE), current {:?}",
                ffi_wallet.name, ffi_wallet.port, balance, ffi_wallet_balance
            ));
            return;
        } else if i % 3 == 0 {
            cucumber_steps_log(format!(
                "Wallet {}:{} waiting for balance to be {:?}, current {:?}",
                ffi_wallet.name, ffi_wallet.port, balance, ffi_wallet_balance
            ))
        } else {
            // Nothing here
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    panic!(
        "Wallet {}:{} doesn't have the correct balance: expected {:?} current {:?}",
        ffi_wallet.name, ffi_wallet.port, balance, ffi_wallet_balance
    );
}

#[when(expr = "I add contact with alias {word} and address of {word} to ffi wallet {word}")]
async fn ffi_add_contact(world: &mut TariWorld, alias: String, pubkey: String, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();

    let address = world.get_wallet_address(&pubkey).await.unwrap();
    let contact = create_contact(alias, address);

    assert!(ffi_wallet.upsert_contact(contact));
}

async fn check_contact(world: &mut TariWorld, alias: String, pubkey: Option<String>, wallet: String) -> bool {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let address: Option<String> = match pubkey {
        Some(pubkey) => Some(world.get_wallet_address(&pubkey).await.unwrap()),
        None => None,
    };
    let contacts = ffi_wallet.get_contacts();
    let mut found = false;
    for i in 0..contacts.get_length() {
        let contact = contacts.get_at(i);
        let contact_address = TariAddress::from_bytes(&contact.get_address().address().get_vec()).unwrap();
        if (address.is_none() || &contact_address.to_base58() == address.as_ref().unwrap()) &&
            contact.get_alias() == alias
        {
            found = true;
            break;
        }
    }
    found
}

#[then(expr = "I have contact with alias {word} and address of {word} in ffi wallet {word}")]
async fn ffi_check_contact(world: &mut TariWorld, alias: String, pubkey: String, wallet: String) {
    assert!(check_contact(world, alias, Some(pubkey), wallet).await);
}

#[when(expr = "I remove contact with alias {word} from ffi wallet {word}")]
async fn ffi_remove_contact(world: &mut TariWorld, alias: String, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let contacts = ffi_wallet.get_contacts();
    let mut contact_to_remove = None;
    for i in 0..contacts.get_length() {
        let contact = contacts.get_at(i);
        if contact.get_alias() == alias {
            contact_to_remove = Some(contact);
            break;
        }
    }
    assert!(contact_to_remove.is_some());
    assert!(ffi_wallet.remove_contact(contact_to_remove.unwrap()));
}

#[then(expr = "I don't have contact with alias {word} in ffi wallet {word}")]
async fn ffi_check_no_contact(world: &mut TariWorld, alias: String, wallet: String) {
    assert!(!check_contact(world, alias, None, wallet).await);
}

#[when(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int}")]
async fn ffi_send_transaction(world: &mut TariWorld, amount: u64, wallet: String, dest: String, fee: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let dest_pub_key = world.get_wallet_address(&dest).await.unwrap();
    let payment_id = PaymentId::open_from_string(
        &format!("Send from ffi {} to ${} at fee ${}", wallet, dest, fee),
        TxType::PaymentToOther,
    );
    let tx_id = ffi_wallet.send_transaction(dest_pub_key, amount, fee, payment_id, false);
    assert_ne!(tx_id, 0, "Send transaction was not successful");
}

#[when(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions")]
#[then(expr = "I send {int} uT from ffi wallet {word} to wallet {word} at fee {int} via one-sided transactions")]
async fn ffi_send_one_sided_transaction(world: &mut TariWorld, amount: u64, wallet: String, dest: String, fee: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let dest_pub_key = world.get_wallet_address(&dest).await.unwrap();
    let payment_id = PaymentId::open_from_string(
        &format!("Send from ffi {} to ${} at fee ${}", wallet, dest, fee),
        TxType::PaymentToOther,
    );
    let tx_id = ffi_wallet.send_transaction(dest_pub_key, amount, fee, payment_id, true);
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
    let mut found_cnt = 0;
    let num_retries = 120;
    for _ in 0..num_retries {
        let pending_outbound_transactions = ffi_wallet.get_pending_outbound_transactions();
        found_cnt = pending_outbound_transactions.get_length();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "The number of pending outbound transaction is lower.");
}

#[then(expr = "I wait for ffi wallet {word} to have at least {int} contacts to be {word}")]
#[when(expr = "I wait for ffi wallet {word} to have at least {int} contacts to be {word}")]
async fn ffi_check_contacts(world: &mut TariWorld, wallet: String, cnt: u64, status: String) {
    assert!(
        ["Online", "Offline", "NeverSeen"].contains(&status.as_str()),
        "Unknown status: {}",
        status
    );
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    cucumber_steps_log(format!(
        "Waiting for {} to have at least {} contacts with status '{}'",
        wallet, cnt, status
    ));
    let mut found_cnt = 0;

    let liveness_data = ffi_wallet.get_liveness_data();
    for i in 0..120 {
        if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Waiting for {} to have at least {} contacts with status '{}', current count: {}",
                wallet, cnt, status, found_cnt
            ));
        }
        found_cnt = 0;
        for (_alias, data) in liveness_data.lock().unwrap().iter() {
            if data.get_online_status() == status {
                found_cnt += 1;
            }
        }
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(
        found_cnt >= cnt,
        "{} doesn't have at least {} contacts with status {}!",
        wallet,
        cnt,
        status
    );
}

#[then(expr = "I want to view the transaction information for completed transactions in ffi wallet {word}")]
async fn ffi_view_transaction_kernels_for_completed(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let completed_transactions = ffi_wallet.get_completed_transactions();
    for i in 0..completed_transactions.get_length() {
        let completed_transaction = completed_transactions.get_at(i);
        let kernel = completed_transaction.get_transaction_kernel();
        cucumber_steps_log(format!("Wallet {}, Transaction kernel info :", wallet));
        assert!(!kernel.get_excess_hex().is_empty());
        cucumber_steps_log(format!("Wallet {}, Excess {}", wallet, kernel.get_excess_hex()));
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
        let address = completed_transaction.get_destination_tari_address();
        assert!(TariAddress::from_hex(&address.address().get_as_hex()).is_ok());
        let address = completed_transaction.get_source_tari_address();
        assert!(TariAddress::from_hex(&address.address().get_as_hex()).is_ok());
        let amount = completed_transaction.get_amount();
        assert!(amount > 0, "Amount '{}', expected > 0", amount);
        let fee = completed_transaction.get_fee();
        assert!(fee > 0, "Fee '{}', expected > 0", fee);
        let timestamp = completed_transaction.get_timestamp();
        assert!(timestamp > 0, "Timestamp '{}', expected > 0", timestamp);
        let payment_id = completed_transaction.get_payment_id();
        assert!(
            !payment_id.is_empty(),
            "Payment id '{}', expected not empty",
            payment_id
        );
        let transaction_type = completed_transaction.get_transaction_type();
        assert_ne!(
            transaction_type, 99,
            "Transaction type '{}', expected not 99",
            transaction_type
        );
        let status = completed_transaction.get_status();
        assert_ne!(status, -1, "Status '{}', expected not -1", status);
        let confirmations = completed_transaction.get_confirmations();
        assert!(
            if status == 6 { confirmations >= 1 } else { true },
            "Confirmations '{}' (with status '{}'), expected >= 1",
            confirmations,
            status
        );
        let cancellation_reason = completed_transaction.get_cancellation_reason();
        assert!(
            if status == 6 { cancellation_reason == -1 } else { true },
            "Cancellation reason '{}' (with status '{}'), expected -1",
            cancellation_reason,
            status
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
    let num_retries = 120;
    let mut found_cnt = 0;
    for _ in 0..num_retries {
        found_cnt = ffi_wallet.get_counters().get_transaction_received();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "Expected {}, but got only {}", cnt, found_cnt);
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} finalization")]
async fn ffi_wait_for_transaction_finalized(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let num_retries = 120;
    let mut found_cnt = 0;
    for _ in 0..num_retries {
        found_cnt = ffi_wallet.get_counters().get_transaction_finalized();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "Expected {}, but got only {}", cnt, found_cnt);
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} broadcast")]
async fn ffi_wait_for_transaction_broadcast(world: &mut TariWorld, wallet: String, cnt: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    let num_retries = 120;
    let mut found_cnt = 0;
    for _ in 0..num_retries {
        found_cnt = ffi_wallet.get_counters().get_transaction_broadcast();
        if found_cnt >= cnt {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= cnt, "Expected {}, but got only {}", cnt, found_cnt);
}

#[then(expr = "I start TXO validation on ffi wallet {word}")]
async fn ffi_start_txo_validation(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.start_txo_validation();
    let num_retries = 120;
    let mut validation_complete = false;
    for _ in 0..num_retries {
        validation_complete = ffi_wallet.get_counters().get_txo_validation_complete();
        if validation_complete {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(validation_complete);
}

#[then(expr = "I start TX validation on ffi wallet {word}")]
async fn ffi_start_tx_validation(world: &mut TariWorld, wallet: String) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.start_transaction_validation();
    let num_retries = 120;
    let mut validation_complete = false;
    for _ in 0..num_retries {
        validation_complete = ffi_wallet.get_counters().get_tx_validation_complete();
        if validation_complete {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(validation_complete);
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
    assert!([
        "TRANSACTION_STATUS_BROADCAST",
        "TRANSACTION_STATUS_MINED_UNCONFIRMED",
        "TRANSACTION_STATUS_MINED",
        "TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED",
        "TRANSACTION_STATUS_ONE_SIDED_CONFIRMED"
    ]
    .contains(&status.as_str()));
    cucumber_steps_log(format!(
        "Waiting for {} to have detected {} {} {} transaction(s)",
        wallet, comparison, count, status
    ));
    let mut found_count = 0;
    for i in 0..120 {
        if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Waiting for {} to have detected {} {} {} transaction(s), current count: {}",
                wallet, comparison, count, status, found_count
            ));
        }
        found_count = match status.as_str() {
            "TRANSACTION_STATUS_BROADCAST" => ffi_wallet.get_counters().get_transaction_broadcast(),
            "TRANSACTION_STATUS_MINED_UNCONFIRMED" => ffi_wallet.get_counters().get_transaction_mined_unconfirmed(),
            "TRANSACTION_STATUS_MINED" => ffi_wallet.get_counters().get_transaction_mined(),
            "TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED" => {
                let mut count = ffi_wallet.get_counters().get_transaction_faux_unconfirmed();
                count += ffi_wallet.get_counters().get_transaction_mined_unconfirmed();
                count
            },
            "TRANSACTION_STATUS_ONE_SIDED_CONFIRMED" => {
                let mut count = ffi_wallet.get_counters().get_transaction_faux_confirmed();
                count += ffi_wallet.get_counters().get_transaction_mined();
                count
            },
            _ => unreachable!(),
        };
        if found_count >= count {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    cucumber_steps_log(format!("Counters {:?}", ffi_wallet.get_counters()));
    match comparison.as_str() {
        "AT_LEAST" => assert!(
            found_count >= count,
            "Counter not adequate! Counter is {}.",
            found_count
        ),
        "EXACTLY" => assert!(
            found_count == count,
            "Counter not adequate! Counter is {}.",
            found_count
        ),
        _ => panic!("Unknown comparison method {}", comparison),
    };
}

#[then(expr = "I wait for ffi wallet {word} to receive {int} mined")]
async fn ffi_wait_for_received_mined(world: &mut TariWorld, wallet: String, count: u64) {
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    cucumber_steps_log(format!(
        "Waiting for {} to receive {} transaction(s) mined",
        wallet, count
    ));

    let mut found_cnt = 0;
    for i in 0..120 {
        if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Waiting for {} to receive {} transaction(s) mined, current count: {}",
                wallet, count, found_cnt
            ));
        }
        found_cnt = ffi_wallet.get_counters().get_transaction_mined();
        if found_cnt >= count {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(found_cnt >= count);
}

#[then(expr = "I recover wallet {word} into ffi wallet {word} from seed words on node {word}")]
async fn ffi_recover_wallet(world: &mut TariWorld, wallet_name: String, ffi_wallet_name: String, base_node: String) {
    let saved_seed_words = get_saved_seed_words(world, &wallet_name);
    let saved_seed_words = saved_seed_words.iter().map(|word| word.as_str()).collect();
    let seed_words = create_seed_words(saved_seed_words);

    spawn_wallet_ffi(world, ffi_wallet_name.clone(), seed_words.get_ptr());

    let base_node = world.get_node(&base_node).unwrap();
    world.get_ffi_wallet(&ffi_wallet_name).unwrap().add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().unwrap().to_string(),
    );
}

#[then(expr = "I restart ffi wallet {word} connected to base node {word}")]
async fn ffi_restart_wallet(world: &mut TariWorld, wallet: String, base_node: String) {
    {
        let ffi_wallet = world.get_mut_ffi_wallet(&wallet).unwrap();
        ffi_wallet.restart(ffi_wallet.port);
    }
    let base_node = world.get_node(&base_node).unwrap();
    let ffi_wallet = world.get_ffi_wallet(&wallet).unwrap();
    ffi_wallet.add_base_node(
        base_node.identity.public_key().to_hex(),
        base_node.identity.first_public_address().unwrap().to_string(),
    );
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
