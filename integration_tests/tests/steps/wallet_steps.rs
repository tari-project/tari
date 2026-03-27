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

#![allow(clippy::indexing_slicing)]
use std::{convert::TryFrom, panic, path::PathBuf, time::Duration};

use cucumber::{given, then, when};
use futures::StreamExt;
use grpc::{
    CancelTransactionRequest,
    ClaimHtlcRefundRequest,
    ClaimShaAtomicSwapRequest,
    Empty,
    GetBalanceRequest,
    GetCompletedTransactionsRequest,
    GetIdentityRequest,
    GetTransactionInfoRequest,
    ImportUtxosRequest,
    PaymentRecipient,
    ReplaceByFeeRequest,
    SendShaAtomicSwapRequest,
    TransferRequest,
    UserPayForFeeRequest,
    ValidateRequest,
};
use minotari_app_grpc::{
    tari_rpc,
    tari_rpc::{self as grpc, GetStateRequest, TransactionStatus, TxOutputsToSpendTransfer},
};
use minotari_console_wallet::{CliCommands, ExportUtxosArgs};
use minotari_wallet::transaction_service::config::TransactionRoutingMechanism;
use tari_common_types::{
    transaction::LegacyTransactionStatus,
    types::{ComAndPubSignature, CompressedPublicKey, PrivateKey, RangeProof},
};
use tari_crypto::ristretto::pedersen::CompressedPedersenCommitment;
use tari_integration_tests::{
    TariWorld,
    transaction::{
        build_transaction_with_output,
        build_transaction_with_output_and_fee_per_gram,
        build_transaction_with_output_and_lockheight,
    },
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet},
};
use tari_script::{ExecutionStack, TariScript};
use tari_transaction_components::{
    MicroMinotari,
    transaction_components::{
        CoinBaseExtra,
        EncryptedData,
        OutputFeatures,
        OutputType,
        RangeProofType,
        TransactionOutputVersion,
        UnblindedOutput,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
    },
};
use tari_utilities::hex::Hex;

use tari_integration_tests::{DEFAULT_TIMEOUT, SHORT_TIMEOUT, wait_for};

use crate::steps::{
    CONFIRMATION_PERIOD,
    cucumber_steps_log,
    mining_steps::create_miner,
};

pub const LOG_TARGET: &str = "cucumber::wallet_steps";

#[given(expr = "a wallet {word} connected to base node {word}")]
async fn start_wallet(world: &mut TariWorld, wallet_name: String, node_name: String) {
    let seeds = world.base_nodes.get(&node_name).unwrap().seed_nodes.clone();
    world
        .wallet_connected_to_base_node
        .insert(wallet_name.clone(), node_name.clone());
    spawn_wallet(world, wallet_name, Some(node_name), seeds, None, None).await;
}

#[when(expr = "I have wallet {word} connected to all seed nodes")]
async fn start_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    // assuming we have deployed at least a base node, we take the first one as base node for wallet to connect to
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    world.wallet_connected_to_base_node.insert(name.clone(), node.clone());
    let mut cli = get_default_cli();
    cli.seed_words_file_name = Some(PathBuf::new().join("seed_words.txt"));
    spawn_wallet(
        world,
        name,
        Some(node.clone()),
        world.all_seed_nodes().to_vec(),
        None,
        Some(cli),
    )
    .await;
}

#[when(expr = "I wait for wallet {word} to have at least {int} uT")]
#[then(expr = "I wait for wallet {word} to have at least {int} uT")]
async fn wait_for_wallet_to_have_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet_ps = world.wallets.get(&wallet).unwrap();
    let mut client = wallet_ps.get_grpc_client().await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("wallet {wallet} to have at least {amount} uT"),
        condition: async {
            let _result = client.validate_all_transactions(ValidateRequest {}).await;
            let balance = client
                .get_balance(GetBalanceRequest { payment_id: None })
                .await
                .unwrap()
                .into_inner();

            if balance.available_balance >= amount {
                cucumber_steps_log(format!(
                    "Wallet {wallet} needs at least available {amount} uT (DONE), has {balance:?}"
                ));
                Ok(true)
            } else {
                Err(format!("available balance: {}", balance.available_balance))
            }
        }
    );
}

#[when(expr = "I remember wallet {word} balance {word}")]
#[then(expr = "I remember wallet {word} balance {word}")]
async fn remember_wallet_balance(world: &mut TariWorld, wallet: String, balance_key: String) {
    let wallet_ps = world.wallets.get(&wallet).unwrap();

    let mut client = wallet_ps.get_grpc_client().await.unwrap();

    let _result = client.validate_all_transactions(ValidateRequest {}).await;
    let balance = client
        .get_balance(GetBalanceRequest { payment_id: None })
        .await
        .unwrap()
        .into_inner();
    cucumber_steps_log(format!(
        "Wallet: {wallet}, balance key: {balance_key}, balance: {balance:?}"
    ));
    world.balance.insert(balance_key, balance);
}

#[when(expr = "I have wallet {word} connected to base node {word}")]
async fn wallet_connected_to_base_node(world: &mut TariWorld, wallet: String, base_node: String) {
    let bn = world.base_nodes.get(&base_node).unwrap();
    let peer_seeds = bn.seed_nodes.clone();
    world
        .wallet_connected_to_base_node
        .insert(wallet.clone(), base_node.clone());

    let mut cli = get_default_cli();
    cli.seed_words_file_name = Some(PathBuf::new().join("seed_words.txt"));
    spawn_wallet(world, wallet, Some(base_node), peer_seeds, None, Some(cli)).await;
}

#[when(expr = "I have wallet {word} connected to seed node {word}")]
async fn have_wallet_connect_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    world
        .wallet_connected_to_base_node
        .insert(wallet.clone(), seed_node.clone());
    spawn_wallet(world, wallet, Some(seed_node.clone()), vec![seed_node], None, None).await;
}

#[when(expr = "wallet {word} detects all transactions as {word}")]
#[then(expr = "wallet {word} detects all transactions as {word}")]
async fn wallet_detects_all_txs_as_mined_status(world: &mut TariWorld, wallet_name: String, status: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();

    let mut completed_tx_stream = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();

    // Collect all tx_ids first, then wait for each
    let mut tx_ids = Vec::new();
    while let Some(tx_info) = completed_tx_stream.next().await {
        let tx_info = tx_info.unwrap();
        tx_ids.push(tx_info.transaction.unwrap().tx_id);
    }

    for tx_id in tx_ids {
        cucumber_steps_log(format!("waiting for tx with tx_id = {tx_id} to be {status}"));
        tari_integration_tests::tx_event_stream::wait_for_tx_status(
            &mut client,
            tx_id,
            &status,
            DEFAULT_TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("Wallet {wallet_name}: {e}"));
    }
}

#[when(expr = "wallet {word} detects all transactions are at least {word}")]
#[then(expr = "wallet {word} detects all transactions are at least {word}")]
async fn wallet_detects_all_txs_are_at_least_in_some_status(
    world: &mut TariWorld,
    wallet_name: String,
    status: String,
) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet_name).await.unwrap();
    let tx_ids = match world.wallet_tx_ids.get(&wallet_address) {
        Some(ids) => ids.clone(),
        None => {
            // Receiver wallet has no sent tx_ids tracked; vacuously satisfied
            cucumber_steps_log(format!("Wallet {wallet_name} has no tracked tx_ids, skipping check"));
            return;
        },
    };

    for tx_id in &tx_ids {
        cucumber_steps_log(format!("waiting for tx with tx_id = {tx_id} to be at least {status}"));
        tari_integration_tests::tx_event_stream::wait_for_tx_status(
            &mut client,
            *tx_id,
            &status,
            DEFAULT_TIMEOUT,
        )
        .await
        .unwrap_or_else(|e| panic!("Wallet {wallet_name}: {e}"));
    }
}

#[then(expr = "wallet {word} detects all transactions are Broadcast")]
async fn wallet_detects_all_txs_as_broadcast(world: &mut TariWorld, wallet_name: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet_name).await.unwrap();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;

    for tx_id in tx_ids {
        cucumber_steps_log(format!("waiting for tx with tx_id = {tx_id} to be mined_confirmed"));
        for retry in 0..=num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();

            if retry == num_retries {
                panic!(
                    "Wallet {} failed to detect tx with tx_id = {} to be mined_confirmed",
                    wallet_name.as_str(),
                    tx_id
                );
            }
            match tx_info.status() {
                grpc::TransactionStatus::Broadcast => {
                    cucumber_steps_log(format!(
                        "Transaction with tx_id = {} has been detected as mined_confirmed by wallet {}",
                        tx_id,
                        wallet_name.as_str()
                    ));
                    return;
                },
                _ => {
                    cucumber_steps_log(format!(
                        "Transaction with tx_id = {} has been detected with status = {:?}",
                        tx_id,
                        tx_info.status()
                    ));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                },
            }
        }
    }
}

#[when(expr = "wallet {word} detects last transaction is Pending")]
async fn wallet_detects_last_tx_as_pending(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();
    let tx_id = tx_ids.last().unwrap(); // get last transaction
    let num_retries = 100;

    cucumber_steps_log(format!("waiting for tx with tx_id = {tx_id} to be pending"));
    for retry in 0..=num_retries {
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![*tx_id],
        };
        let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
        let tx_info = tx_info.transactions.first().unwrap();

        if retry == num_retries {
            panic!(
                "Wallet {} failed to detect tx with tx_id = {} to be pending",
                wallet.as_str(),
                tx_id
            );
        }
        match tx_info.status() {
            grpc::TransactionStatus::Pending => {
                cucumber_steps_log(format!(
                    "Transaction with tx_id = {} has been detected as pending by wallet {}",
                    tx_id,
                    wallet.as_str()
                ));
                return;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            },
        }
    }
}

#[when(expr = "wallet {word} detects last transaction is Cancelled")]
async fn wallet_detects_last_tx_as_cancelled(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();
    let tx_id = tx_ids.last().unwrap(); // get last transaction
    let num_retries = 100;

    cucumber_steps_log(format!("waiting for tx with tx_id = {tx_id} to be Cancelled"));
    for retry in 0..=num_retries {
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![*tx_id],
        };
        let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
        let tx_info = tx_info.transactions.first().unwrap();

        if retry == num_retries {
            panic!(
                "Wallet {} failed to detect tx with tx_id = {} to be cancelled, current status is {:?}",
                wallet.as_str(),
                tx_id,
                tx_info.status(),
            );
        }
        match tx_info.status() {
            grpc::TransactionStatus::Rejected => {
                cucumber_steps_log(format!(
                    "Transaction with tx_id = {} has status {:?}",
                    tx_id,
                    tx_info.status()
                ));
                return;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            },
        }
    }
}

#[when(expr = "I list all {word} transactions for wallet {word}")]
#[then(expr = "I list all {word} transactions for wallet {word}")]
async fn list_all_txs_for_wallet(world: &mut TariWorld, transaction_type: String, wallet: String) {
    if transaction_type.as_str() != "COINBASE" && transaction_type.as_str() != "NORMAL" {
        panic!("Invalid transaction type. Values should be COINBASE or NORMAL, value passed is {transaction_type}");
    }
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();

    let request = GetCompletedTransactionsRequest {
        payment_id: None,
        block_hash: None,
        block_height: None,
    };
    let mut completed_txs = client.get_completed_transactions(request).await.unwrap().into_inner();

    while let Some(tx) = completed_txs.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        let is_coinbase = tx_info.status == TransactionStatus::Coinbase as i32 ||
            tx_info.status == TransactionStatus::CoinbaseConfirmed as i32 ||
            tx_info.status == TransactionStatus::CoinbaseUnconfirmed as i32 ||
            tx_info.status == TransactionStatus::CoinbaseNotInBlockChain as i32;
        if transaction_type == "COINBASE" && !is_coinbase || transaction_type == "NORMAL" && is_coinbase {
            continue;
        }
        cucumber_steps_log(format!(
            "TxId: {}, Status: {}, IsCancelled: {}, {}",
            tx_info.tx_id, tx_info.status, tx_info.is_cancelled, transaction_type
        ));
    }
}

#[when(expr = "wallet {word} has at least {int} transactions that are all {word} and not cancelled")]
#[then(expr = "wallet {word} has at least {int} transactions that are all {word} and not cancelled")]
async fn wallet_has_at_least_num_txs(world: &mut TariWorld, wallet: String, num_txs: u64, transaction_status: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let transaction_status = match transaction_status.as_str() {
        "TRANSACTION_STATUS_COMPLETED" => 0,
        "TRANSACTION_STATUS_BROADCAST" => 1,
        "TRANSACTION_STATUS_MINED_UNCONFIRMED" => 2,
        "TRANSACTION_STATUS_IMPORTED" => 3,
        "TRANSACTION_STATUS_PENDING" => 4,
        "TRANSACTION_STATUS_COINBASE" => 5,
        "TRANSACTION_STATUS_MINED_CONFIRMED" => 6,
        "TRANSACTION_STATUS_REJECTED" => 7,
        "TRANSACTION_STATUS_ONE_SIDED_UNCONFIRMED" => 8,
        "TRANSACTION_STATUS_ONE_SIDED_CONFIRMED" => 9,
        "TRANSACTION_STATUS_QUEUED" => 10,
        "TRANSACTION_STATUS_NOT_FOUND" => 11,
        "TRANSACTION_STATUS_COINBASE_UNCONFIRMED" => 12,
        "TRANSACTION_STATUS_COINBASE_CONFIRMED" => 13,
        "TRANSACTION_STATUS_COINBASE_NOT_IN_BLOCK_CHAIN" => 14,
        _ => panic!("Invalid transaction status {transaction_status}"),
    };

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("wallet {wallet} to have at least {num_txs} txs with status {transaction_status}"),
        condition: async {
            let mut txs = client
                .get_completed_transactions(grpc::GetCompletedTransactionsRequest {
                    payment_id: None,
                    block_hash: None,
                    block_height: None,
                })
                .await
                .unwrap()
                .into_inner();
            let mut found_tx = 0u64;
            while let Some(tx) = txs.next().await {
                let tx_info = tx.unwrap().transaction.unwrap();
                if tx_info.status == transaction_status {
                    found_tx += 1;
                }
            }
            if found_tx >= num_txs {
                Ok(true)
            } else {
                Err(format!("found {found_tx} matching txs"))
            }
        }
    );
}

#[when(expr = "I create a transaction {word} spending {word} to {word}")]
pub fn create_tx_spending_coinbase(world: &mut TariWorld, transaction: String, inputs: String, output: String) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output(utxos, &world.key_manager);
    world.utxos.insert(output, utxo);
    world.transactions.insert(transaction, tx);
}

#[when(expr = "I create a custom fee transaction {word} spending {word} to {word} with fee per gram {word}")]
async fn create_tx_custom_fee_per_gram(
    world: &mut TariWorld,
    transaction: String,
    inputs: String,
    output: String,
    fee: u64,
) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output_and_fee_per_gram(utxos, fee, &world.key_manager);
    world.utxos.insert(output, utxo);
    world.transactions.insert(transaction, tx);
}

#[when(expr = "I create a custom locked transaction {word} spending {word} to {word} with lockheight {word}")]
fn create_tx_custom_lock(world: &mut TariWorld, transaction: String, inputs: String, output: String, lockheight: u64) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output_and_lockheight(utxos, lockheight, &world.key_manager);
    world.utxos.insert(output, utxo);
    world.transactions.insert(transaction, tx);
}

#[then(expr = "I wait for wallet {word} to have less than {int} uT")]
#[when(expr = "I wait for wallet {word} to have less than {int} uT")]
async fn wait_for_wallet_to_have_less_than_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    cucumber_steps_log(format!("Waiting for wallet {wallet} to have less than {amount} uT"));

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("wallet {wallet} to have less than {amount} uT"),
        condition: async {
            let _result = client.validate_all_transactions(ValidateRequest {}).await;
            let balance_res = client
                .get_balance(GetBalanceRequest { payment_id: None })
                .await
                .unwrap()
                .into_inner();
            if balance_res.available_balance < amount {
                cucumber_steps_log(format!(
                    "Wallet {wallet} needs less than available {amount} uT (DONE), has {balance_res:?}"
                ));
                Ok(true)
            } else {
                Err(format!("available balance: {}", balance_res.available_balance))
            }
        }
    );
}

#[then(expr = "I wait for wallet {word} to have scanned to height {int}")]
#[when(expr = "I wait for wallet {word} to have scanned to height {int}")]
async fn wait_for_wallet_to_have_scanned_to_height(world: &mut TariWorld, wallet: String, height: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    cucumber_steps_log(format!(
        "Waiting for wallet {wallet} to have scanned to height {height}"
    ));

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("wallet {wallet} to scan to height {height}"),
        condition: async {
            let _result = client.validate_all_transactions(ValidateRequest {}).await;
            let state_res = client.get_state(GetStateRequest {}).await.unwrap().into_inner();
            if state_res.scanned_height == height {
                cucumber_steps_log(format!(
                    "Wallet {wallet} needs to scan to height {height} (DONE), current {state_res:?}"
                ));
                Ok(true)
            } else {
                Err(format!("scanned height: {}", state_res.scanned_height))
            }
        }
    );
}

#[then(expr = "all wallets validate their transactions")]
#[when(expr = "all wallets validate their transactions")]
async fn all_wallets_validate_their_transactions(world: &mut TariWorld) {
    let wallets = world.wallets.keys().cloned().collect::<Vec<_>>();
    for wallet in &wallets {
        let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
        let result = client.validate_all_transactions(ValidateRequest {}).await;
        if let Err(e) = result {
            cucumber_steps_log(format!(
                "Error! Wallet {wallet} failed to validate transactions, error: {e:?}"
            ));
        }
    }
}

#[when(expr = "I have non-default wallet {word} connected to all seed nodes using {word}")]
#[given(expr = "I have non-default wallet {word} connected to all seed nodes using {word}")]
async fn non_default_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, wallet: String, mechanism: String) {
    let routing_mechanism = TransactionRoutingMechanism::from(mechanism);
    // assuming we have at least one base node as seed node, we use the first to connect wallet to
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    world.wallet_connected_to_base_node.insert(wallet.clone(), node.clone());
    spawn_wallet(
        world,
        wallet,
        Some(node.clone()),
        world.all_seed_nodes().to_vec(),
        Some(routing_mechanism),
        None,
    )
    .await;
}

#[given(expr = "I have {int} non-default wallets connected to all seed nodes using {word}")]
#[when(expr = "I have {int} non-default wallets connected to all seed nodes using {word}")]
async fn non_default_wallets_connected_to_all_seed_nodes(world: &mut TariWorld, num: u64, mechanism: String) {
    let routing_mechanism = TransactionRoutingMechanism::from(mechanism);
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    for ind in 0..num {
        let wallet_name = format!("Wallet_{ind}");
        world
            .wallet_connected_to_base_node
            .insert(wallet_name.clone(), node.clone());
        let mut cli = get_default_cli();
        cli.seed_words_file_name = Some(PathBuf::new().join("seed_words.txt"));
        spawn_wallet(
            world,
            wallet_name,
            Some(node.clone()),
            world.all_seed_nodes().to_vec(),
            Some(routing_mechanism),
            Some(cli),
        )
        .await;
    }
}

#[when(
    expr = "I send {int} uT one-sided without waiting for broadcast from wallet {word} to wallet {word} at fee {int}"
)]
#[then(
    expr = "I send {int} uT one-sided without waiting for broadcast from wallet {word} to wallet {word} at fee {int}"
)]
async fn send_amount_from_source_wallet_to_dest_wallet_without_broadcast(
    world: &mut TariWorld,
    amount: u64,
    source_wallet: String,
    dest_wallet: String,
    fee: u64,
) {
    let mut source_client = create_wallet_client(world, source_wallet.clone()).await.unwrap();
    let source_wallet_address = world.get_wallet_address(&source_wallet).await.unwrap();

    let dest_wallet_address = world.get_wallet_address(&dest_wallet).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: dest_wallet_address.clone(),
        amount,
        fee_per_gram: fee,
        payment_type: 1, // one sided transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "transfer amount {} from {} to {}",
                amount,
                source_wallet.as_str(),
                dest_wallet.as_str()
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let tx_res = source_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transacting amount {} uT from wallet {} to {} at fee {} failed",
        amount,
        source_wallet.as_str(),
        dest_wallet.as_str(),
        fee
    );

    let tx_id = tx_res.transaction_id;

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(source_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "Transfer amount {amount} from {source_wallet} to {dest_wallet} at fee {fee} succeeded"
    ));
}

#[when(expr = "I send a one-sided transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send a one-sided transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn send_one_sided_transaction_from_source_wallet_to_dest_wallt(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee: u64,
) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram: fee,
        payment_type: 1, // one sided transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "One sided transfer amount {} from {} to {}",
                amount,
                sender.as_str(),
                receiver.as_str()
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let tx_res = sender_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "One sided transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee
    );

    // we wait for transaction to be broadcast
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = sender_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for one sided transaction from {} to {} (DONE) with amount {} at fee {} to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for one sided transaction from {} to {} with amount {} at fee {} to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee,
            ));
        } else {
            // Nothing here
        }

        if i == num_retries - 1 {
            panic!(
                "One sided transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "One sided transaction with amount {amount} from {sender} to {receiver} at fee {fee} succeeded"
    ));
}

#[then(expr = "I send an interactive transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[when(expr = "I send an interactive transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn send_interactive_amount_from_wallet_to_wallet_at_fee(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 2, // one-sided stealth transaction (MW interactive not supported)
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "Transfer amount {} from {} to {} as fee {}",
                amount,
                sender.as_str(),
                receiver.as_str(),
                fee_per_gram
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;
    cucumber_steps_log(format!("Transaction results: {tx_res:?}"));

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    cucumber_steps_log(format!("Transaction 1 result: {tx_res:?}"));
    assert!(
        tx_res.is_success,
        "Transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee_per_gram
    );

    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for transaction from {} to {} with amount {} at fee {} (DONE) to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for transaction from {} to {} with amount {} at fee {} to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries - 1 {
            panic!(
                "Transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "Transaction with amount {amount} from {sender} to {receiver} at fee {fee_per_gram} succeeded"
    ));
}

#[then(expr = "I send {} interactive transactions of {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[when(expr = "I send {} interactive transactions of {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[allow(clippy::too_many_lines)]
async fn send_many_interactive_amount_from_wallet_to_wallet_at_fee(
    world: &mut TariWorld,
    number_of_transactions: u64,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 0, // mimblewimble transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "Transfer amount {} from {} to {} as fee {}",
                amount,
                sender.as_str(),
                receiver.as_str(),
                fee_per_gram
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let mut tx_ids = Vec::with_capacity(usize::try_from(number_of_transactions).unwrap());
    for i in 0..number_of_transactions {
        cucumber_steps_log(format!(
            "Sending transaction {} of {} with amount {} from {} to {} at fee {}",
            i + 1,
            number_of_transactions,
            amount,
            sender,
            receiver,
            fee_per_gram
        ));
        let tx_res = sender_wallet_client
            .transfer(transfer_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_res = tx_res.results;
        cucumber_steps_log(format!("Transaction results: {tx_res:?}"));

        assert_eq!(tx_res.len(), 1usize);

        let tx_res = tx_res.first().unwrap();
        cucumber_steps_log(format!("Transaction 1 result: {tx_res:?}"));
        assert!(
            tx_res.is_success,
            "Transaction with amount {} from wallet {} to {} at fee {} failed",
            amount,
            sender.as_str(),
            receiver.as_str(),
            fee_per_gram
        );
        tx_ids.push(tx_res.transaction_id);
    }

    for tx_id in &tx_ids {
        let tx_info_req = GetTransactionInfoRequest {
            transaction_ids: vec![*tx_id],
        };

        let num_retries = 300; // 30s total wait with 100ms intervals
        for i in 0..num_retries {
            let tx_info_res = sender_wallet_client
                .get_transaction_info(tx_info_req.clone())
                .await
                .unwrap()
                .into_inner();
            let tx_info = tx_info_res.transactions.first().unwrap();

            // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
            if tx_info.status == 1_i32 {
                cucumber_steps_log(format!(
                    "Wait for transaction from {} to {} with amount {} at fee {} (DONE) to be broadcast",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                ));
                break;
            } else if i % 5 == 0 {
                cucumber_steps_log(format!(
                    "Wait for transaction from {} to {} with amount {} at fee {} to be broadcast",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                ));
            } else {
                // Nothing here
            }

            if i == num_retries - 1 {
                panic!(
                    "Transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                )
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // Insert tx_id's to the corresponding world mapping
    world
        .wallet_tx_ids
        .insert(sender_wallet_address.clone(), tx_ids.clone());

    cucumber_steps_log(format!(
        "{number_of_transactions} consecutive interactive transactions with amount {amount} from {sender} to \
         {receiver} at fee {fee_per_gram} succeeded"
    ));
}

#[then(expr = "wallet {word} detects at least {int} coinbase transactions as CoinbaseConfirmed")]
async fn wallet_detects_at_least_coinbase_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut completed_tx_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;
    let mut total_mined_confirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        cucumber_steps_log(format!("{wallet_name}: Detecting coinbase confirmed transactions"));
        'inner: while let Some(tx_info) = completed_tx_res.next().await {
            let tx_id = tx_info.unwrap().transaction.unwrap().tx_id;
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::CoinbaseConfirmed => {
                    total_mined_confirmed_coinbases += 1;
                    if total_mined_confirmed_coinbases >= coinbases {
                        break 'outer;
                    }
                },
                _ => continue 'inner,
            }
        }

        if total_mined_confirmed_coinbases < coinbases {
            total_mined_confirmed_coinbases = 0;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    if total_mined_confirmed_coinbases >= coinbases {
        cucumber_steps_log(format!(
            "Wallet {wallet_name} detected at least {coinbases} coinbase transactions as CoinbaseConfirmed"
        ));
    } else {
        panic!("Wallet {wallet_name} failed to detect at least {coinbases} coinbase transactions as CoinbaseConfirmed");
    }
}

#[then(expr = "wallet {word} detects at least {int} coinbase transactions as CoinbaseUnconfirmed")]
async fn wallet_detects_at_least_coinbase_unconfirmed_transactions(
    world: &mut TariWorld,
    wallet_name: String,
    coinbases: u64,
) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut completed_tx_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;
    let mut total_mined_unconfirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        cucumber_steps_log(format!("{wallet_name}, Detecting coinbase unconfirmed transactions"));
        'inner: while let Some(tx_info) = completed_tx_res.next().await {
            let tx_id = tx_info.unwrap().transaction.unwrap().tx_id;
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::CoinbaseUnconfirmed | grpc::TransactionStatus::CoinbaseNotInBlockChain => {
                    total_mined_unconfirmed_coinbases += 1;
                    if total_mined_unconfirmed_coinbases >= coinbases {
                        break 'outer;
                    }
                },
                _ => continue 'inner,
            }
        }

        if total_mined_unconfirmed_coinbases < coinbases {
            total_mined_unconfirmed_coinbases = 0;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    if total_mined_unconfirmed_coinbases >= coinbases {
        cucumber_steps_log(format!(
            "Wallet {wallet_name} detected at least {coinbases} coinbase transactions as CoinbaseConfirmed"
        ));
    } else {
        panic!("Wallet {wallet_name} failed to detect at least {coinbases} coinbase transactions as CoinbaseConfirmed");
    }
}

#[then(expr = "wallet {word} detects only {int} transaction as unconfirmed")]
async fn wallet_detects_only_transactions_as_unconfirmed(
    world: &mut TariWorld,
    wallet_name: String,
    expected_count: u64,
) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut completed_tx_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();

    let num_retries = 40;
    let mut total_unconfirmed_transactions = 0u64;
    let mut all_transactions_status = Vec::new();

    'outer: for _ in 0..num_retries {
        cucumber_steps_log(format!("{wallet_name}, Detecting unconfirmed transactions"));
        total_unconfirmed_transactions = 0;
        all_transactions_status.clear();

        'inner: while let Some(tx_info) = completed_tx_res.next().await {
            let tx_id = tx_info.unwrap().transaction.unwrap().tx_id;
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            all_transactions_status.push((tx_id, tx_info.status()));
            match tx_info.status() {
                grpc::TransactionStatus::MinedUnconfirmed => {
                    total_unconfirmed_transactions += 1;
                },
                _ => continue 'inner,
            }
        }

        // Debug: Print all transaction statuses
        cucumber_steps_log(format!(
            "{}, Found {} completed transactions with statuses: {:?}",
            wallet_name,
            all_transactions_status.len(),
            all_transactions_status
        ));

        if total_unconfirmed_transactions == expected_count {
            break 'outer;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        completed_tx_res = client
            .get_completed_transactions(GetCompletedTransactionsRequest {
                payment_id: None,
                block_hash: None,
                block_height: None,
            })
            .await
            .unwrap()
            .into_inner();
    }

    assert_eq!(
        total_unconfirmed_transactions, expected_count,
        "Expected {expected_count} unconfirmed transactions, but found {total_unconfirmed_transactions}. All \
         transaction statuses: {all_transactions_status:?}"
    );
}

#[then(expr = "wallet {word} detects exactly {int} coinbase transactions as CoinbaseConfirmed")]
async fn wallet_detects_exactly_coinbase_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();

    let num_retries = 100;
    let mut total_mined_confirmed_coinbases = 0;

    for _ in 0..num_retries {
        cucumber_steps_log("Detecting coinbase confirmed transactions");
        total_mined_confirmed_coinbases = 0;
        let mut completed_tx_res = client
            .get_completed_transactions(GetCompletedTransactionsRequest {
                payment_id: None,
                block_hash: None,
                block_height: None,
            })
            .await
            .unwrap()
            .into_inner();

        while let Some(tx_info) = completed_tx_res.next().await {
            let tx_id = tx_info.unwrap().transaction.unwrap().tx_id;
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            if tx_info.status() == grpc::TransactionStatus::CoinbaseConfirmed {
                total_mined_confirmed_coinbases += 1;
            }
        }

        if total_mined_confirmed_coinbases == coinbases {
            break;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    if total_mined_confirmed_coinbases == coinbases {
        cucumber_steps_log(format!(
            "Wallet {wallet_name} detected exactly {coinbases} coinbase transactions as CoinbaseConfirmed"
        ));
    } else {
        panic!(
            "Wallet {wallet_name} failed to detect exactly {coinbases} coinbase transactions as CoinbaseConfirmed \
             (found {total_mined_confirmed_coinbases})"
        );
    }
}

#[then(expr = "I stop all wallets")]
async fn stop_all_wallets(world: &mut TariWorld) {
    for (wallet, wallet_ps) in &mut world.wallets {
        cucumber_steps_log(format!("Stopping wallet {wallet}"));
        wallet_ps.kill();
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
}

#[then(expr = "I stop wallet {word}")]
#[when(expr = "I stop wallet {word}")]
async fn stop_wallet(world: &mut TariWorld, wallet: String) {
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    world.wallet_addresses.insert(wallet.clone(), wallet_address);
    cucumber_steps_log(format!("Stopping wallet {}", wallet.as_str()));
    wallet_ps.kill();
    tokio::time::sleep(Duration::from_secs(5)).await;
}

#[when(expr = "I start wallet {word}")]
#[then(expr = "I start wallet {word}")]
async fn start_wallet_without_node(world: &mut TariWorld, wallet: String) {
    match world.wallet_connected_to_base_node.get(&wallet) {
        None => spawn_wallet(world, wallet, None, vec![], None, None).await,
        Some(base_node) => {
            // start wallet
            let base_node_ps = world.base_nodes.get(base_node).unwrap();
            let seed_nodes = base_node_ps.seed_nodes.clone();
            spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, None).await;
        },
    }
}

#[then(expr = "I stop-start wallet {word}")]
async fn restart_wallet(world: &mut TariWorld, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    // stop wallet
    wallet_ps.kill();
    tokio::time::sleep(Duration::from_secs(5)).await;
    // start wallet
    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap().clone();
    let base_node_ps = world.base_nodes.get(&base_node).unwrap();
    let seed_nodes = base_node_ps.seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node), seed_nodes, None, None).await;
}

#[then(expr = "all wallets detect all transactions as Mined_or_OneSidedConfirmed")]
async fn all_wallets_detect_all_txs_as_mined_confirmed(world: &mut TariWorld) {
    for wallet in world.wallets.keys() {
        let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
        let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
        let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address);

        let wallet_tx_ids = if let Some(wallet_tx_ids) = wallet_tx_ids {
            if wallet_tx_ids.is_empty() {
                panic!("Wallet {} should have available transaction ids", wallet.as_str());
            }
            wallet_tx_ids.clone()
        } else {
            cucumber_steps_log(format!("Wallet {wallet} has no available transactions"));
            vec![]
        };

        let num_retries = 100;

        for tx_id in wallet_tx_ids {
            'inner: for retry in 0..=num_retries {
                let req = GetTransactionInfoRequest {
                    transaction_ids: vec![tx_id],
                };
                let res = wallet_client.get_transaction_info(req).await.unwrap().into_inner();
                let tx_status = res.transactions.first().unwrap().status;

                if tx_status == LegacyTransactionStatus::MinedConfirmed as i32 ||
                    tx_status == LegacyTransactionStatus::OneSidedConfirmed as i32
                {
                    cucumber_steps_log(format!(
                        "Wallet {wallet} has detected transaction with id {tx_id} as Mined_or_OneSidedConfirmed"
                    ));
                    break 'inner;
                }

                if retry == num_retries {
                    panic!(
                        "Transaction with id {tx_id} does not have status as Mined_or_OneSidedConfirmed, on wallet \
                         {wallet}, status is {tx_status}"
                    );
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

#[then(expr = "wallets {word} should have {word} {int} spendable coinbase outputs")]
async fn wallets_should_have_at_least_num_spendable_coinbase_outs(
    world: &mut TariWorld,
    wallets: String,
    comparison: String,
    amount_of_coinbases: u64,
) {
    let at_least = "AT_LEAST";
    let exactly = "EXACTLY";

    if comparison.as_str() != at_least && comparison.as_str() != exactly {
        panic!("Invalid comparison value provided: {comparison}");
    }

    let wallets = wallets.split(',').collect::<Vec<_>>();
    let mut wallets_clients: Vec<_> = vec![];
    for w in &wallets {
        wallets_clients.push(create_wallet_client(world, w.to_string()).await.unwrap());
    }

    let num_retries = 100;
    let mut unspendable_coinbase_count = 0;
    let mut spendable_coinbase_count = 0;

    for ind in 0..wallets_clients.len() {
        let wallet = wallets[ind];
        let mut client = wallets_clients[ind].clone();

        'inner: for _ in 0..num_retries {
            let mut stream = client
                .get_completed_transactions(GetCompletedTransactionsRequest {
                    payment_id: None,
                    block_hash: None,
                    block_height: None,
                })
                .await
                .unwrap()
                .into_inner();
            while let Some(completed_tx) = stream.next().await {
                let tx_info = completed_tx.unwrap().transaction.unwrap();
                if tx_info.status == grpc::TransactionStatus::CoinbaseUnconfirmed as i32 {
                    unspendable_coinbase_count += 1;
                    cucumber_steps_log(format!(
                        "Found coinbase transaction with id {} for wallet '{}' as 'CoinbaseUnconfirmed'",
                        tx_info.tx_id, &wallet
                    ));
                }
                if tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32 {
                    unspendable_coinbase_count += 1;
                    cucumber_steps_log(format!(
                        "Found coinbase transaction with id {} for wallet '{}' as 'CoinbaseNotInBlockChain'",
                        tx_info.tx_id, &wallet
                    ));
                }
                if tx_info.status == grpc::TransactionStatus::CoinbaseConfirmed as i32 {
                    spendable_coinbase_count += 1;
                    cucumber_steps_log(format!(
                        "Found coinbase transaction with id {} for wallet '{}' as 'CoinbaseConfirmed'",
                        tx_info.tx_id, &wallet
                    ));
                }
            }

            if spendable_coinbase_count >= amount_of_coinbases {
                cucumber_steps_log(format!(
                    "Wallet '{}' has found at least {} spendable coinbases within a total of {} coinbase transactions",
                    &wallet,
                    amount_of_coinbases,
                    spendable_coinbase_count + unspendable_coinbase_count
                ));
                break 'inner;
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        if comparison == at_least && spendable_coinbase_count >= amount_of_coinbases {
            cucumber_steps_log(format!("Wallet {wallet} has found at least {amount_of_coinbases}"));
        } else if comparison == exactly && spendable_coinbase_count == amount_of_coinbases {
            cucumber_steps_log(format!("Wallet {wallet} has found exactly {amount_of_coinbases}"));
        } else {
            panic!(
                "Wallet {wallet} hasn't found {comparison} {amount_of_coinbases} spendable outputs, instead got \
                 {spendable_coinbase_count}"
            );
        }
    }
}

#[when(
    expr = "I send {int} one-sided transactions of {int} uT each from wallet {word} to wallet {word} at fee_per_gram \
            {int}"
)]
async fn send_num_one_sided_transactions_to_wallets_at_fee(
    world: &mut TariWorld,
    num_txs: u64,
    amount: u64,
    sender_wallet: String,
    receiver_wallet: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender_wallet.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender_wallet).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver_wallet).await.unwrap();
    let mut tx_ids = vec![];

    for _ in 0..num_txs {
        let payment_recipient = PaymentRecipient {
            address: receiver_wallet_address.clone(),
            amount,
            fee_per_gram,
            payment_type: 1, // one sided transaction
            raw_payment_id: MemoField::new_open_from_string(
                &format!(
                    "transfer amount {} from {} to {}",
                    amount,
                    sender_wallet.as_str(),
                    receiver_wallet.as_str()
                ),
                TxType::PaymentToOther,
            )
            .unwrap()
            .to_bytes(),
            user_payment_id: None,
        };
        let transfer_req = TransferRequest {
            recipients: vec![payment_recipient],
            single_tx: false,
        };
        let transfer_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
        let transfer_res = transfer_res.results.first().unwrap();

        if !transfer_res.is_success {
            panic!(
                "Failed to send transaction from wallet {} to wallet {}, with message \n {}",
                &sender_wallet, &receiver_wallet, &transfer_res.failure_message
            );
        }
        tx_ids.push(transfer_res.transaction_id);

        // insert tx_id's to the corresponding world mapping
        let source_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

        source_tx_ids.append(&mut tx_ids);

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let num_retries = 100;
    cucumber_steps_log(format!(
        "Waiting for transactions from wallet {sender_wallet} to wallet {receiver_wallet} to be broadcast"
    ));

    for tx_id in tx_ids {
        cucumber_steps_log(format!("Waiting for transaction with id {tx_id} to be broadcast"));
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![tx_id],
        };

        let mut is_broadcast = false;

        'inner: for i in 0..num_retries {
            let txs_info = sender_wallet_client
                .get_transaction_info(request.clone())
                .await
                .unwrap()
                .into_inner();
            let txs_info = txs_info.transactions.first().unwrap();

            if txs_info.status == 1 {
                cucumber_steps_log(format!(
                    "Wait for Transaction from wallet {sender_wallet} to wallet {receiver_wallet} (DONE) with id \
                     {tx_id} broadcast to the network"
                ));
                is_broadcast = true;
                break 'inner;
            } else if i % 5 == 0 {
                cucumber_steps_log(format!(
                    "Wait for Transaction from wallet {sender_wallet} to wallet {receiver_wallet} with id {tx_id} \
                     broadcast to the network"
                ));
            } else {
                // Nothing here
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        if !is_broadcast {
            panic!(
                "Transaction from wallet {sender_wallet} to wallet {receiver_wallet} with id {tx_id} was not \
                 broacasted to the network"
            );
        }
    }
}

#[then(expr = "I wait for {word} to have a node connection")]
async fn wait_for_wallet_to_have_num_connections(world: &mut TariWorld, wallet: String) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    cucumber_steps_log(format!("Waiting for wallet {wallet} to have a connection"));
    let mut connections = 0_u32;

    for _ in 0..num_retries {
        let network_status_res = wallet_client.get_network_status(Empty {}).await.unwrap().into_inner();
        connections = network_status_res.num_node_connections;
        if u64::from(connections) >= 1 {
            cucumber_steps_log(format!("Wallet {wallet} has a connection"));
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    if u64::from(connections) < 1 {
        panic!("Wallet {wallet} does not have a connection");
    }
}

#[when(expr = "I wait for wallet {word} to have connectivity")]
#[then(expr = "I wait for wallet {word} to have connectivity")]
async fn wallet_pending_connection(world: &mut TariWorld, wallet: String) {
    let mut wallet_client = world.get_wallet_client(&wallet).await.unwrap();

    for _i in 0..30 {
        let res: tonic::Response<tari_rpc::GetConnectedHttpPeerResponse> =
            wallet_client.get_connected_http_peer(Empty {}).await.unwrap();
        let res = res.into_inner();
        if let Some(peer) = res.connected_peer &&
            peer.is_online
        {
            return;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    panic!("Peer was not connected in time");
}

#[then(expr = "I wait for {word} to have {word} connectivity")]
async fn wait_for_wallet_to_have_specific_connectivity(world: &mut TariWorld, wallet: String, connectivity: String) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    cucumber_steps_log(format!(
        "Waiting for wallet {wallet} to have connectivity {connectivity}"
    ));
    let connectivity = connectivity.to_uppercase();

    // applications/minotari_app_grpc/proto/network.proto ->
    // enum ConnectivityStatus {
    //     Initializing = 0;
    //     Online = 1;
    //     Degraded = 2;
    //     Offline = 3;
    // }
    let connectivity_index = match connectivity.as_str() {
        "INITIALIZING" => 0,
        "ONLINE" => 1,
        "DEGRADED" => 2,
        "OFFLINE" => 3,
        _ => panic!("Invalid connectivity value {connectivity}"),
    };

    for _ in 0..=num_retries {
        let network_status_res = wallet_client.get_network_status(Empty {}).await.unwrap().into_inner();
        let connectivity_status = network_status_res.status;
        if connectivity_status == connectivity_index {
            cucumber_steps_log(format!("Wallet {wallet} has {connectivity} connectivity"));
            return;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    panic!("Wallet {wallet} did not get correct connectivity status {connectivity}");
}

#[when(expr = "I transfer {int}T one-sided from {word} to {word}")]
async fn transfer_tari_from_wallet_to_receiver(world: &mut TariWorld, amount: u64, sender: String, receiver: String) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount: amount * 1_000_000_u64, // 1T = 1_000_000uT
        fee_per_gram: 10,               // as in the js cucumber tests
        payment_type: 1,                // one sided transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "transfer amount {} from {} to {}",
                amount,
                sender.as_str(),
                receiver.as_str()
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transacting amount {}T from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        10
    );

    // we wait for transaction to be broadcast
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for Transaction from {} to {} with amount {} at fee {} (DONE) to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for Transaction from {} to {} with amount {} at fee {} to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            ));
        } else {
            // Nothing here
        }

        if i == num_retries {
            panic!(
                "Transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "Transfer amount {amount} from {sender} to {receiver} at fee 10 succeeded"
    ));
}

#[when(expr = "wallet {word} has {int}T")]
#[then(expr = "wallet {word} has {int}T")]
async fn wallet_has_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    let mut available_balance = 0;

    for _ in 0..num_retries {
        let _result = wallet_client.validate_all_transactions(ValidateRequest {}).await;
        let balance_res = wallet_client
            .get_balance(GetBalanceRequest { payment_id: None })
            .await
            .unwrap()
            .into_inner();

        available_balance = balance_res.available_balance;
        if available_balance >= amount * 1_000_000 {
            cucumber_steps_log(format!("Wallet {} has at least {}T", wallet.as_str(), amount));
            return;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    panic!("Wallet {wallet} failed to have at least {amount}T, it ended with {available_balance}T");
}

#[when(expr = "I have wallet {word} with {int}T connected to base node {word}")]
async fn wallet_with_tari_connected_to_base_node(
    world: &mut TariWorld,
    wallet: String,
    amount: u64,
    base_node: String,
) {
    let peer_seeds = world.base_nodes.get(&base_node).unwrap().seed_nodes.clone();
    cucumber_steps_log(format!(
        "Start a new wallet {} connected to base node {}",
        wallet.as_str(),
        base_node.as_str()
    ));
    world
        .wallet_connected_to_base_node
        .insert(wallet.clone(), base_node.clone());
    spawn_wallet(world, wallet.clone(), Some(base_node.clone()), peer_seeds, None, None).await;

    let mut base_node_client = world.get_node_client(&base_node).await.unwrap();
    let tip_info_res = base_node_client.get_tip_info(Empty {}).await.unwrap().into_inner();
    let mut current_height = tip_info_res.metadata.unwrap().best_block_height;

    let mut num_blocks = 0;
    let mut reward = 0;

    while reward < amount {
        current_height += 1;
        num_blocks += 1;
        reward += world.consensus_manager.get_block_reward_at(current_height).as_u64() / 1_000_000;
        // 1 T = 1_000_000
        // uT
    }

    cucumber_steps_log("Creating miner...");
    create_miner(
        world,
        "SHA3X".to_string(),
        "temp_miner".to_string(),
        base_node.clone(),
        wallet.clone(),
    )
    .await;

    cucumber_steps_log(format!("Mining {} blocks", num_blocks + CONFIRMATION_PERIOD));
    let miner = world.miners.get(&"temp_miner".to_string()).unwrap();
    miner
        .mine(world, Some(num_blocks + CONFIRMATION_PERIOD), None, None)
        .await; // mine some additional blocks to confirm txs

    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    for _ in 0..num_retries {
        let _result = wallet_client.validate_all_transactions(ValidateRequest {}).await;
        let balance_res = wallet_client
            .get_balance(GetBalanceRequest { payment_id: None })
            .await
            .unwrap()
            .into_inner();

        if balance_res.available_balance >= amount * 1_000_000 {
            cucumber_steps_log(format!("Wallet {} has at least {}T", wallet.as_str(), amount));
            return;
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    panic!("Wallet {wallet} failed to have at least {amount}T");
}

#[when(expr = "I transfer {int} uT one-sided from {word} to {word} and {word} at fee {int}")]
#[allow(clippy::too_many_lines)]
async fn transfer_one_sided_from_wallet_to_two_recipients_at_fee(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver1: String,
    receiver2: String,
    fee_per_gram: u64,
) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver1_address = world.get_wallet_address(&receiver1).await.unwrap();
    let receiver2_address = world.get_wallet_address(&receiver2).await.unwrap();

    let payment_recipient1 = PaymentRecipient {
        address: receiver1_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 1, // one sided transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "transfer amount {} from {} to {}",
                amount,
                sender.as_str(),
                receiver1.as_str()
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };

    let payment_recipient2 = PaymentRecipient {
        address: receiver2_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 1, // one sided transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "transfer amount {} from {} to {}",
                amount,
                sender.as_str(),
                receiver2.as_str()
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient1, payment_recipient2],
        single_tx: true,
    };
    let tx_res = sender_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 2_usize);

    let tx_res1 = tx_res.first().unwrap();
    let tx_res2 = tx_res.last().unwrap();

    assert!(
        tx_res1.is_success,
        "Transacting amount {} uT from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver1.as_str(),
        fee_per_gram
    );
    assert!(
        tx_res2.is_success,
        "Transacting amount {} uT from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver2.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcast
    let tx_id1 = tx_res1.transaction_id;
    let tx_id2 = tx_res2.transaction_id;

    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id1, tx_id2],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info1 = tx_info_res.transactions.first().unwrap();
        let tx_info2 = tx_info_res.transactions.last().unwrap();

        cucumber_steps_log(format!(
            "Tx_info for first recipient {} is {}, for tx_id = {}",
            receiver1, tx_info1.status, tx_id1
        ));
        cucumber_steps_log(format!(
            "Tx_info for second recipient {} is {}, for tx_id = {}",
            receiver2, tx_info2.status, tx_id2
        ));
        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info1.status == 1_i32 && tx_info2.status == 1_i32 {
            cucumber_steps_log(format!(
                "Transaction from {} to {} and {} with amount {} at fee {} (DONE) to be broadcast",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Transaction from {} to {} and {} with amount {} at fee {} to be broadcast",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries {
            panic!(
                "Transaction from {} to {} and {} with amount {} at fee {} failed to be broadcast",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                10
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id1);
    sender_tx_ids.push(tx_id2);

    cucumber_steps_log(format!(
        "Transfer amount {amount} from {sender} to {receiver1} and {receiver2} at fee {fee_per_gram} succeeded"
    ));
}

#[when(expr = "I transfer {int} uT to self from wallet {word} at fee {int}")]
async fn transfer_tari_to_self(world: &mut TariWorld, amount: u64, sender: String, fee_per_gram: u64) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: sender_wallet_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 0, // normal mimblewimble payment type
        raw_payment_id: MemoField::new_open_from_string(
            &format!("transfer amount {} from {} to self", amount, sender.as_str()),
            TxType::PaymentToSelf,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Transacting amount {} to self from wallet {} at fee {} failed",
        amount,
        sender.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcast
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for Transaction to self from {} with amount {} at fee {} (DONE) to be broadcast",
                sender.clone(),
                amount,
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for Transaction to self from {} with amount {} at fee {} to be broadcast",
                sender.clone(),
                amount,
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries {
            panic!(
                "Transaction to self from {} with amount {} at fee {} failed to be broadcast",
                sender.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "Transfer amount {amount} to self from {sender} at fee {fee_per_gram} succeeded"
    ));
}

#[when(expr = "I broadcast HTLC transaction with {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn htlc_transaction(world: &mut TariWorld, amount: u64, sender: String, receiver: String, fee_per_gram: u64) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 0, // normal mimblewimble transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "Atomic Swap from {} to {} with amount {} at fee {}",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };

    let atomic_swap_request = SendShaAtomicSwapRequest {
        recipient: Some(payment_recipient),
    };
    let sha_atomic_swap_tx_res = sender_wallet_client
        .send_sha_atomic_swap_transaction(atomic_swap_request)
        .await
        .unwrap()
        .into_inner();

    assert!(
        sha_atomic_swap_tx_res.is_success,
        "Atomic swap transacting amount uT {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcast
    let tx_id = sha_atomic_swap_tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = sender_wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for Atomic swap transaction from {} to {} (DONE) with amount {} at fee {} to be broadcast",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for Atomic swap transaction from {} to {} with amount {} at fee {} to be broadcast",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries {
            panic!(
                "Atomic swap transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    world.output_hash = Some(sha_atomic_swap_tx_res.output_hash);
    world.pre_image = Some(sha_atomic_swap_tx_res.pre_image);

    cucumber_steps_log(format!(
        "Atomic swap transfer amount {amount} from {sender} to {receiver} at fee {fee_per_gram} succeeded"
    ));
}

#[when(expr = "I claim an HTLC refund transaction with wallet {word} at fee {int}")]
async fn claim_htlc_refund_transaction_with_wallet_at_fee(world: &mut TariWorld, wallet: String, fee_per_gram: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let output_hash = world.output_hash.clone().unwrap();

    let claim_htlc_req = ClaimHtlcRefundRequest {
        output_hash,
        fee_per_gram,
    };

    let claim_htlc_refund_res = wallet_client
        .claim_htlc_refund_transaction(claim_htlc_req)
        .await
        .unwrap()
        .into_inner();

    assert!(
        claim_htlc_refund_res.clone().results.unwrap().is_success,
        "Claim HTLC refund transaction with wallet {} at fee {} failed",
        wallet.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcast
    let tx_id = claim_htlc_refund_res.results.unwrap().transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for Claim HTLC refund transaction with wallet {} (DONE) at fee {} to be broadcast",
                wallet.as_str(),
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for Claim HTLC refund transaction with wallet {} at fee {} to be broadcast",
                wallet.as_str(),
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries {
            panic!(
                "Claim HTLC refund transaction with wallet {} at fee {} failed to be broadcast",
                wallet.as_str(),
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let wallet_tx_ids = world.wallet_tx_ids.entry(wallet_address.clone()).or_default();
    wallet_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "Claim HTLC refund transaction with wallet {wallet} at fee {fee_per_gram} succeeded"
    ));
}

#[when(expr = "I claim an HTLC transaction with wallet {word} at fee {int}")]
async fn wallet_claims_htlc_transaction_at_fee(world: &mut TariWorld, wallet: String, fee_per_gram: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
    let output_hash = world.output_hash.clone().unwrap();
    let pre_image = world.pre_image.clone().unwrap();

    let claim_htlc_req = ClaimShaAtomicSwapRequest {
        output: output_hash,
        pre_image,
        fee_per_gram,
    };

    let claim_htlc_res = wallet_client
        .claim_sha_atomic_swap_transaction(claim_htlc_req)
        .await
        .unwrap()
        .into_inner();

    assert!(
        claim_htlc_res.clone().results.unwrap().is_success,
        "Claim HTLC transaction with wallet {} at fee {} failed",
        wallet.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcast
    let tx_id = claim_htlc_res.results.unwrap().transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..=num_retries {
        let tx_info_res = wallet_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for Claim HTLC transaction with wallet {} (DONE) at fee {} to be broadcast",
                wallet.as_str(),
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for Claim HTLC transaction with wallet {} at fee {} to be broadcast",
                wallet.as_str(),
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries {
            panic!(
                "Claim HTLC transaction with wallet {} at fee {} failed to be broadcast",
                wallet.as_str(),
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let wallet_tx_ids = world.wallet_tx_ids.entry(wallet_address.clone()).or_default();
    wallet_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "Claim HTLC transaction with wallet {wallet} at fee {fee_per_gram} succeeded"
    ));
}

#[when(expr = "I send a one-sided stealth transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send a one-sided stealth transaction of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn send_one_sided_stealth_transaction(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        payment_type: 2, // one sided stealth transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!(
                "One sided stealth transfer amount {} from {} to {}",
                amount,
                sender.as_str(),
                receiver.as_str()
            ),
            TxType::PaymentToOther,
        )
        .unwrap()
        .to_bytes(),
        user_payment_id: None,
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
        single_tx: false,
    };
    let tx_res = sender_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "One sided stealth transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        sender.as_str(),
        receiver.as_str(),
        fee_per_gram
    );

    // we wait for transaction to be broadcast
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = sender_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            cucumber_steps_log(format!(
                "Wait for one sided stealth transaction from {} to {} (DONE) with amount {} at fee {} to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            ));
            break;
        } else if i % 5 == 0 {
            cucumber_steps_log(format!(
                "Wait for one sided stealth transaction from {} to {} with amount {} at fee {} to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            ));
        } else {
            // Nothing here
        }

        if i == num_retries - 1 {
            panic!(
                "One sided stealth transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    cucumber_steps_log(format!(
        "One sided stealth transaction with amount {amount} from {sender} to {receiver} at fee {fee_per_gram} \
         succeeded"
    ));
}

#[then(expr = "I import {word} unspent outputs to {word}")]
#[allow(clippy::too_many_lines)]
async fn import_wallet_unspent_outputs(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();
    if wallet_a_ps.is_running() {
        cucumber_steps_log(format!("Stopping wallet {wallet_a}"));
        wallet_a_ps.kill();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
        with_private_keys: true,
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();

    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(
        world,
        wallet_a.clone(),
        Some(base_node.clone()),
        seed_nodes,
        None,
        Some(cli),
    )
    .await;

    let exported_outputs = std::fs::File::open(path_buf).unwrap();
    let mut reader = csv::Reader::from_reader(exported_outputs);

    let mut outputs: Vec<UnblindedOutput> = vec![];

    for output in reader.records() {
        let output = output.unwrap();
        let version = match &output[1] {
            "V0" => TransactionOutputVersion::V0,
            "V1" => TransactionOutputVersion::V1,
            _ => panic!("Invalid output version"),
        };
        let value = MicroMinotari(output[2].parse::<u64>().unwrap());
        let spending_key = PrivateKey::from_hex(&output[3]).unwrap();
        let flags = match &output[5] {
            "Standard" => OutputType::Standard,
            "Coinbase" => OutputType::Coinbase,
            "Burn" => OutputType::Burn,
            "ValidatorNodeRegistration" => OutputType::ValidatorNodeRegistration,
            "CodeTemplateRegistration" => OutputType::CodeTemplateRegistration,
            _ => panic!("Invalid output type"),
        };
        let maturity = output[6].parse::<u64>().unwrap();
        let coinbase_extra = CoinBaseExtra::try_from(Vec::from_hex(&output[7]).unwrap()).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = CompressedPublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment = CompressedPedersenCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = CompressedPublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_data = EncryptedData::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroMinotari(output[20].parse::<u64>().unwrap());
        let proof = if output[21].is_empty() {
            None
        } else {
            Some(RangeProof::from_hex(&output[21]).unwrap())
        };

        let features =
            OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None, RangeProofType::BulletProofPlus);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_a,
            signature_u_x,
            signature_u_y,
        );
        let utxo = UnblindedOutput::new(
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            proof,
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversion"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
        payment_id: MemoField::new_open_from_string(
            &format!("I import {wallet_a} unspent outputs to {wallet_b}"),
            TxType::ImportedUtxoNoneRewindable,
        )
        .unwrap()
        .to_bytes(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I import {word} spent outputs to {word}")]
#[allow(clippy::too_many_lines)]
async fn import_wallet_spent_outputs(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();
    if wallet_a_ps.is_running() {
        cucumber_steps_log(format!("Stopping wallet {wallet_a}"));
        wallet_a_ps.kill();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
        with_private_keys: true,
    };
    cli.command2 = Some(CliCommands::ExportSpentUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(
        world,
        wallet_a.clone(),
        Some(base_node.clone()),
        seed_nodes,
        None,
        Some(cli),
    )
    .await;

    let exported_outputs = std::fs::File::open(path_buf).unwrap();
    let mut reader = csv::Reader::from_reader(exported_outputs);

    let mut outputs: Vec<UnblindedOutput> = vec![];

    for output in reader.records() {
        let output = output.unwrap();
        let version = match &output[1] {
            "V0" => TransactionOutputVersion::V0,
            "V1" => TransactionOutputVersion::V1,
            _ => panic!("Invalid output version"),
        };
        let value = MicroMinotari(output[2].parse::<u64>().unwrap());
        let spending_key = PrivateKey::from_hex(&output[3]).unwrap();
        let flags = match &output[5] {
            "Standard" => OutputType::Standard,
            "Coinbase" => OutputType::Coinbase,
            "Burn" => OutputType::Burn,
            "ValidatorNodeRegistration" => OutputType::ValidatorNodeRegistration,
            "CodeTemplateRegistration" => OutputType::CodeTemplateRegistration,
            _ => panic!("Invalid output type"),
        };
        let maturity = output[6].parse::<u64>().unwrap();
        let coinbase_extra = CoinBaseExtra::try_from(Vec::from_hex(&output[7]).unwrap()).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = CompressedPublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment = CompressedPedersenCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = CompressedPublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_data = EncryptedData::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroMinotari(output[20].parse::<u64>().unwrap());
        let proof = if output[21].is_empty() {
            None
        } else {
            Some(RangeProof::from_hex(&output[21]).unwrap())
        };

        let features =
            OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None, RangeProofType::BulletProofPlus);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_a,
            signature_u_x,
            signature_u_y,
        );
        let utxo = UnblindedOutput::new(
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            proof,
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversion"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
        payment_id: MemoField::new_open_from_string(
            &format!("I import {wallet_a} spent outputs to {wallet_b}"),
            TxType::ImportedUtxoNoneRewindable,
        )
        .unwrap()
        .to_bytes(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}
#[allow(clippy::too_many_lines)]
#[then(expr = "I import {word} unspent outputs as pre_mine outputs to {word}")]
async fn import_unspent_outputs_as_pre_mine(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();
    if wallet_a_ps.is_running() {
        cucumber_steps_log(format!("Stopping wallet {wallet_a}"));
        wallet_a_ps.kill();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
        with_private_keys: true,
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(
        world,
        wallet_a.clone(),
        Some(base_node.clone()),
        seed_nodes,
        None,
        Some(cli),
    )
    .await;

    let exported_outputs = std::fs::File::open(path_buf).unwrap();
    let mut reader = csv::Reader::from_reader(exported_outputs);

    let mut outputs: Vec<UnblindedOutput> = vec![];

    for output in reader.records() {
        let output = output.unwrap();
        let version = match &output[1] {
            "V0" => TransactionOutputVersion::V0,
            "V1" => TransactionOutputVersion::V1,
            _ => panic!("Invalid output version"),
        };
        let value = MicroMinotari(output[2].parse::<u64>().unwrap());
        let spending_key = PrivateKey::from_hex(&output[3]).unwrap();
        let flags = match &output[5] {
            "Standard" => OutputType::Standard,
            "Coinbase" => OutputType::Coinbase,
            "Burn" => OutputType::Burn,
            "ValidatorNodeRegistration" => OutputType::ValidatorNodeRegistration,
            "CodeTemplateRegistration" => OutputType::CodeTemplateRegistration,
            _ => panic!("Invalid output type"),
        };
        let maturity = output[6].parse::<u64>().unwrap();
        let coinbase_extra = CoinBaseExtra::try_from(Vec::from_hex(&output[7]).unwrap()).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = CompressedPublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment = CompressedPedersenCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = CompressedPublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_data = EncryptedData::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroMinotari(output[20].parse::<u64>().unwrap());
        let proof = if output[21].is_empty() {
            None
        } else {
            Some(RangeProof::from_hex(&output[21]).unwrap())
        };

        let features =
            OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None, RangeProofType::BulletProofPlus);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_a,
            signature_u_x,
            signature_u_y,
        );
        let utxo = UnblindedOutput::new(
            version,
            value,
            spending_key,
            features,
            script,
            input_data,
            script_private_key,
            sender_offset_public_key,
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            proof,
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversion"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
        payment_id: MemoField::new_open_from_string(
            &format!("I import {wallet_a} unspent outputs as pre_mine outputs to {wallet_b}"),
            TxType::ImportedUtxoNoneRewindable,
        )
        .unwrap()
        .to_bytes(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I check if wallet {word} has {int} transactions")]
async fn check_if_wallet_has_num_transactions(world: &mut TariWorld, wallet: String, num_txs: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();

    let mut count = 0;
    while let Some(tx) = get_completed_txs_res.next().await {
        let _tx = tx.unwrap(); // make sure we get the actual response
        count += 1;
    }

    assert_eq!(
        num_txs,
        count,
        "Wallet {} did not get {} transactions, instead it got {}",
        wallet.as_str(),
        num_txs,
        count
    );
}

#[when(expr = "I multi-send {int} one-sided transactions of {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I multi-send {int} one-sided transactions of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn multi_send_txs_from_wallet(
    world: &mut TariWorld,
    num_txs: u64,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let mut transfer_res = vec![];

    for _ in 0..num_txs {
        let payment_recipient = PaymentRecipient {
            address: receiver_wallet_address.clone(),
            amount,
            fee_per_gram,
            payment_type: 1, // one sided transaction
            raw_payment_id: MemoField::new_open_from_string(
                &format!(
                    "I send multi-transfers with amount {} from {} to {} with fee per gram {}",
                    amount,
                    sender.as_str(),
                    receiver.as_str(),
                    fee_per_gram
                ),
                TxType::PaymentToOther,
            )
            .unwrap()
            .to_bytes(),
            user_payment_id: None,
        };

        let transfer_req = TransferRequest {
            recipients: vec![payment_recipient],
            single_tx: false,
        };
        let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
        let tx_res = tx_res.results;

        assert_eq!(tx_res.len(), 1usize);

        let tx_res = tx_res.first().unwrap();
        assert!(
            tx_res.is_success,
            "Multi-Transaction with amount {} from wallet {} to {} at fee {} failed",
            amount,
            sender.as_str(),
            receiver.as_str(),
            fee_per_gram
        );

        transfer_res.push(tx_res.clone());
    }

    let num_retries = 100;

    for tx_res in transfer_res {
        let tx_id = tx_res.transaction_id;
        let tx_info_req = GetTransactionInfoRequest {
            transaction_ids: vec![tx_id],
        };

        for i in 0..num_retries {
            let tx_info_res = sender_wallet_client
                .get_transaction_info(tx_info_req.clone())
                .await
                .unwrap()
                .into_inner();
            let tx_info = tx_info_res.transactions.first().unwrap();

            // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
            if tx_info.status == 1_i32 {
                cucumber_steps_log(format!(
                    "Wait for Multi-transaction from {} to {} with amount {} at fee {} has been broadcast",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                ));
                break;
            }

            if i == num_retries - 1 {
                panic!(
                    "Multi-transaction from {} to {} with amount {} at fee {} failed to be broadcast",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                )
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // insert tx_id's to the corresponding world mapping
        let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

        sender_tx_ids.push(tx_id);

        cucumber_steps_log(format!(
            "Multi-transaction with amount {amount} from {sender} to {receiver} at fee {fee_per_gram} succeeded"
        ));
    }
}

#[then(expr = "I cancel last transaction in wallet {word}")]
async fn cancel_last_transaction_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet).await.unwrap();

    let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    // get the last tx id for wallet
    let tx_id = *wallet_tx_ids.last().unwrap();
    let cancel_tx_req = CancelTransactionRequest {
        tx_id,
        force_if_completed: false,
    };
    let cancel_tx_res = client.cancel_transaction(cancel_tx_req).await.unwrap().into_inner();
    assert!(
        cancel_tx_res.is_success,
        "Unable to cancel transaction with id = {tx_id}"
    );
}

#[when(
    expr = "I send a replace by fee of {int} uT from wallet {word} to wallet {word} at fee higher by {int} then before"
)]
async fn send_replace_by_fee_transaction(
    world: &mut TariWorld,
    _amount: u64,
    sender: String,
    _receiver: String,
    fee_increase: u64,
) {
    let mut client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let wallet_tx_ids = world.wallet_tx_ids.get(&sender_wallet_address).unwrap();
    let tx_id = *wallet_tx_ids.last().unwrap();

    let replace_by_fee_req = ReplaceByFeeRequest {
        transaction_id: tx_id,
        fee_increase,
    };

    let replace_by_fee_res = client.replace_by_fee(replace_by_fee_req).await.unwrap().into_inner();
    let new_tx_id = replace_by_fee_res.transaction_id;

    let wallet_tx_ids = world.wallet_tx_ids.get_mut(&sender_wallet_address).unwrap();
    wallet_tx_ids.push(new_tx_id);
}

#[allow(clippy::too_many_lines)]
#[when(expr = "I send a user_pay_for_fee from wallet {word} to wallet {word} at fee {int}")]
async fn send_user_pay_for_fee_transaction(world: &mut TariWorld, sender: String, receiver: String, fee: u64) {
    let mut client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let wallet_tx_ids = world.wallet_tx_ids.get(&sender_wallet_address).unwrap();
    let tx_id = *wallet_tx_ids.last().unwrap();

    let transfer_with_tx_id = TxOutputsToSpendTransfer {
        tx_id,
        fee,
        destination: receiver_wallet_address.clone(),
    };
    let user_pay_for_fee_req = UserPayForFeeRequest {
        recipients: vec![transfer_with_tx_id],
    };

    let response = client.user_pay_for_fee(user_pay_for_fee_req).await;
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let tx_results = response
        .expect("UserPayForFee response should succeed")
        .into_inner()
        .results;
    assert!(
        !tx_results.is_empty(),
        "UserPayForFee response should contain at least one result"
    );
    let tx_result = tx_results.first().unwrap();
    assert!(
        tx_result.is_success,
        "UserPayForFee should be successful. Failure: {}",
        tx_result.failure_message
    );
    let new_tx_id = tx_result.transaction_id;

    let wallet_tx_ids = world.wallet_tx_ids.get_mut(&sender_wallet_address).unwrap();
    wallet_tx_ids.push(new_tx_id);
}

#[when(expr = "I create a burn transaction of {int} uT from {word} at fee {int}")]
async fn burn_transaction(world: &mut TariWorld, amount: u64, wallet: String, fee: u64) {
    let mut client = world.get_wallet_client(&wallet).await.unwrap();
    let identity = client.identify(GetIdentityRequest {}).await.unwrap().into_inner();

    let req = grpc::CreateBurnTransactionRequest {
        amount,
        fee_per_gram: fee,
        claim_public_key: identity.public_key,
        sidechain_deployment_key: vec![],
        payment_id: MemoField::new_open_from_string("Burning some tari", TxType::Burn)
            .unwrap()
            .to_bytes(),
    };

    let result = client.create_burn_transaction(req).await.unwrap();
    let tx_id = result.into_inner().transaction_id;

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("burn transaction from {wallet} to be broadcast/confirmed"),
        condition: async {
            let result = client
                .get_transaction_info(grpc::GetTransactionInfoRequest {
                    transaction_ids: vec![tx_id],
                })
                .await
                .unwrap();

            let status = result.into_inner().transactions.last().unwrap().status;
            if let 1 | 2 | 6 = status {
                Ok(true)
            } else {
                Err(format!("status: {status}"))
            }
        }
    );
}

#[then(expr = "wallet {word} balance is {word}")]
async fn wallet_has_balance(world: &mut TariWorld, wallet_name: String, balance_key: String) {
    let mut client = world.get_wallet_client(&wallet_name).await.unwrap();
    let balance = world.balance.get(&balance_key).unwrap().clone();

    wait_for!(
        timeout: SHORT_TIMEOUT,
        description: format!("wallet {wallet_name} to match balance {balance_key}"),
        condition: async {
            let _result = client.validate_all_transactions(ValidateRequest {}).await;
            let balance_res = client
                .get_balance(GetBalanceRequest { payment_id: None })
                .await
                .unwrap()
                .into_inner();
            if balance_res == balance {
                cucumber_steps_log(format!(
                    "Wallet {wallet_name} needs balance {balance:?} (DONE), has {balance_res:?}"
                ));
                Ok(true)
            } else {
                Err(format!("current: {balance_res:?}"))
            }
        }
    );
}

#[then(expr = "wallet {word} has {int} coinbase transactions")]
async fn wallet_has_num_coinbase_transactions(world: &mut TariWorld, wallet_name: String, expected: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();

    wait_for!(
        timeout: DEFAULT_TIMEOUT,
        description: format!("wallet {wallet_name} to have {expected} coinbase transactions"),
        condition: async {
            let mut txs = client
                .get_completed_transactions(GetCompletedTransactionsRequest {
                    payment_id: None,
                    block_hash: None,
                    block_height: None,
                })
                .await
                .unwrap()
                .into_inner();
            let mut found = 0u64;
            while let Some(tx) = txs.next().await {
                let tx_info = tx.unwrap().transaction.unwrap();
                let is_coinbase = tx_info.status == grpc::TransactionStatus::Coinbase as i32 ||
                    tx_info.status == grpc::TransactionStatus::CoinbaseConfirmed as i32 ||
                    tx_info.status == grpc::TransactionStatus::CoinbaseUnconfirmed as i32 ||
                    tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32;
                if is_coinbase {
                    found += 1;
                }
            }
            if found >= expected {
                Ok(true)
            } else {
                Err(format!("found {found} coinbase txs"))
            }
        }
    );
}

#[then(expr = "all COINBASE transactions for wallet {word} are valid")]
async fn all_coinbase_transactions_for_wallet_are_valid(world: &mut TariWorld, wallet_name: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut txs = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();
    while let Some(tx) = txs.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        let is_coinbase = tx_info.status == grpc::TransactionStatus::Coinbase as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseConfirmed as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseUnconfirmed as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32;
        if is_coinbase {
            assert!(
                !tx_info.is_cancelled,
                "Wallet {wallet_name} has a cancelled coinbase transaction (tx_id: {})",
                tx_info.tx_id
            );
        }
    }
}

#[then(expr = "all NORMAL transactions for wallet {word} are valid")]
async fn all_normal_transactions_for_wallet_are_valid(world: &mut TariWorld, wallet_name: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut txs = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();
    while let Some(tx) = txs.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        let is_coinbase = tx_info.status == grpc::TransactionStatus::Coinbase as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseConfirmed as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseUnconfirmed as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32;
        if !is_coinbase {
            assert!(
                !tx_info.is_cancelled,
                "Wallet {wallet_name} has a cancelled normal transaction (tx_id: {})",
                tx_info.tx_id
            );
        }
    }
}

#[then(
    expr = "all COINBASE transactions for wallet {word} and wallet {word} have consistent but opposing cancellation"
)]
async fn coinbase_transactions_have_opposing_cancellation(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    // Retry for up to 120 seconds to allow wallets time to detect and process the reorg.
    // The UTXO scanner runs every 60 seconds, so we need at least one cycle to complete.
    let num_retries = 60;
    for i in 0..num_retries {
        let cancelled_a = get_coinbase_cancellation_status(world, &wallet_a).await;
        let cancelled_b = get_coinbase_cancellation_status(world, &wallet_b).await;
        if cancelled_a != cancelled_b {
            return;
        }
        if i < num_retries - 1 {
            cucumber_steps_log(format!(
                "Wallets {wallet_a} and {wallet_b} both show cancelled={cancelled_a}, waiting for reorg to \
                 propagate..."
            ));
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    let cancelled_a = get_coinbase_cancellation_status(world, &wallet_a).await;
    let cancelled_b = get_coinbase_cancellation_status(world, &wallet_b).await;
    assert_ne!(
        cancelled_a,
        cancelled_b,
        "Wallets {wallet_a} and {wallet_b} should have opposing coinbase cancellation status, but both are {}",
        if cancelled_a { "cancelled" } else { "not cancelled" }
    );
}

async fn get_coinbase_cancellation_status(world: &mut TariWorld, wallet_name: &str) -> bool {
    let mut client = create_wallet_client(world, wallet_name.to_string()).await.unwrap();
    let mut txs = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        })
        .await
        .unwrap()
        .into_inner();
    let mut cancelled_count = 0u64;
    let mut total_coinbase = 0u64;
    while let Some(tx) = txs.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        let is_coinbase = tx_info.status == grpc::TransactionStatus::Coinbase as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseConfirmed as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseUnconfirmed as i32 ||
            tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32;
        if is_coinbase {
            total_coinbase += 1;
            // A coinbase is considered cancelled/reorged if it is explicitly cancelled OR
            // if it has CoinbaseNotInBlockChain status (set when the block it was mined in is reorged out).
            if tx_info.is_cancelled || tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32 {
                cancelled_count += 1;
            }
        }
    }
    assert!(total_coinbase > 0, "Wallet {wallet_name} has no coinbase transactions");
    cancelled_count > total_coinbase / 2
}

#[then(expr = "I wait for recovered wallets to have at least {int} uT")]
async fn wait_for_recovered_wallets_to_have_micro_tari(world: &mut TariWorld, amount: u64) {
    let wallet_names: Vec<String> = world.wallets.keys().cloned().collect();
    for wallet_name in wallet_names {
        let num_retries = 100;
        let mut total_balance = 0;
        for i in 0..=num_retries {
            let wallet_ps = world.wallets.get(&wallet_name).unwrap();
            let mut client = wallet_ps.get_grpc_client().await.unwrap();
            let _unused = client.validate_all_transactions(ValidateRequest {}).await;
            let balance = client
                .get_balance(GetBalanceRequest { payment_id: None })
                .await
                .unwrap()
                .into_inner();
            // Include all balance components: recovered coinbase outputs may be stored as
            // UnspentMinedUnconfirmed (pending_incoming) until TXO validation confirms them.
            total_balance = balance.available_balance + balance.timelocked_balance + balance.pending_incoming_balance;
            if total_balance >= amount {
                cucumber_steps_log(format!(
                    "Recovered wallet {wallet_name} has at least {amount} uT (DONE): {total_balance}"
                ));
                break;
            } else if i % 5 == 0 {
                cucumber_steps_log(format!(
                    "Recovered wallet {wallet_name} needs at least {amount} uT, has {total_balance}"
                ));
            } else {
                // clippy
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        assert!(
            total_balance >= amount,
            "Recovered wallet {wallet_name} failed to get balance of at least {amount}, current: {total_balance}"
        );
    }
}

/// Records the current wall-clock time under the given label so it can later be compared with a stop step.
/// Usage in a feature file: `When I start benchmark timer <label>`
#[when(expr = "I start benchmark timer {word}")]
async fn start_benchmark_timer(world: &mut TariWorld, name: String) {
    world.benchmark_timers.insert(name.clone(), std::time::Instant::now());
    let msg = format!("BENCHMARK [{name}]: timer started");
    eprintln!("{msg}");
    cucumber_steps_log(&msg);
}

/// Stops the named benchmark timer, prints the elapsed duration to stderr and appends it to the cucumber step log.
/// Uses eprintln! because Cucumber's Basic writer owns stdout; stderr is the correct channel for step output.
/// Panics if the corresponding start step was never called.
/// Usage in a feature file: `Then I stop benchmark timer <label> and log elapsed time`
#[then(expr = "I stop benchmark timer {word} and log elapsed time")]
async fn stop_benchmark_timer_and_log(world: &mut TariWorld, name: String) {
    let start = world
        .benchmark_timers
        .get(&name)
        .unwrap_or_else(|| panic!("Benchmark timer '{name}' was never started"));
    let elapsed = start.elapsed();
    let msg = format!(
        "BENCHMARK [{}]: {:.3}s ({} ms)",
        name,
        elapsed.as_secs_f64(),
        elapsed.as_millis()
    );
    eprintln!("{msg}");
    cucumber_steps_log(&msg);
}
