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

use std::{convert::TryFrom, path::PathBuf, time::Duration};

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
    SendShaAtomicSwapRequest,
    TransferRequest,
    ValidateRequest,
};
use minotari_app_grpc::tari_rpc::{self as grpc, TransactionStatus};
use minotari_console_wallet::{CliCommands, ExportUtxosArgs};
use minotari_wallet::transaction_service::config::TransactionRoutingMechanism;
use tari_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            EncryptedData,
            OutputFeatures,
            OutputType,
            RangeProofType,
            TransactionOutputVersion,
            UnblindedOutput,
        },
    },
};
use tari_crypto::{commitment::HomomorphicCommitment, keys::PublicKey as PublicKeyTrait};
use tari_integration_tests::{
    transaction::{
        build_transaction_with_output,
        build_transaction_with_output_and_fee_per_gram,
        build_transaction_with_output_and_lockheight,
    },
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet},
    TariWorld,
};
use tari_script::{ExecutionStack, StackItem, TariScript};
use tari_utilities::hex::Hex;

use crate::steps::{mining_steps::create_miner, CONFIRMATION_PERIOD, HALF_SECOND, TWO_MINUTES_WITH_HALF_SECOND_SLEEP};

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
    spawn_wallet(
        world,
        name,
        Some(node.clone()),
        world.all_seed_nodes().to_vec(),
        None,
        None,
    )
    .await;
}

#[when(expr = "I wait for wallet {word} to have at least {int} uT")]
#[then(expr = "I wait for wallet {word} to have at least {int} uT")]
async fn wait_for_wallet_to_have_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet_ps = world.wallets.get(&wallet).unwrap();
    let num_retries = 100;

    let mut client = wallet_ps.get_grpc_client().await.unwrap();
    let mut curr_amount = 0;

    for _ in 0..=num_retries {
        let _result = client.validate_all_transactions(ValidateRequest {}).await;
        curr_amount = client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner()
            .available_balance;

        if curr_amount >= amount {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // failed to get wallet right amount, so we panic
    panic!(
        "wallet {} failed to get balance of at least amount {}, current amount is {}",
        wallet, amount, curr_amount
    );
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
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;

    while let Some(tx_info) = completed_tx_stream.next().await {
        let tx_info = tx_info.unwrap();
        let tx_id = tx_info.transaction.unwrap().tx_id;

        println!("waiting for tx with tx_id = {} to be {}", tx_id, status);
        for retry in 0..=num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();

            if retry == num_retries {
                panic!(
                    "Wallet {} failed to detect tx with tx_id = {} to be {}, current status is {:?}",
                    wallet_name.as_str(),
                    tx_id,
                    status,
                    tx_info.status()
                );
            }
            match status.as_str() {
                "Pending" => match tx_info.status() {
                    grpc::TransactionStatus::Pending |
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Completed" => match tx_info.status() {
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Broadcast" => match tx_info.status() {
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Mined_or_OneSidedUnconfirmed" => match tx_info.status() {
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed |
                    grpc::TransactionStatus::CoinbaseUnconfirmed |
                    grpc::TransactionStatus::CoinbaseConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Mined_or_OneSidedConfirmed" => match tx_info.status() {
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed |
                    grpc::TransactionStatus::CoinbaseConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Coinbase" => match tx_info.status() {
                    grpc::TransactionStatus::CoinbaseConfirmed | grpc::TransactionStatus::CoinbaseUnconfirmed => {
                        break;
                    },
                    _ => (),
                },
                _ => panic!("Unknown status {}, don't know what to expect", status),
            }
            // tokio sleep 100ms
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
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
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;

    for tx_id in tx_ids {
        println!("waiting for tx with tx_id = {} to be pending", tx_id);
        for retry in 0..=num_retries {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();

            if retry == num_retries {
                panic!(
                    "Wallet {} failed to detect tx with tx_id = {} to be at least {}",
                    wallet_name.as_str(),
                    tx_id,
                    status
                );
            }
            match status.as_str() {
                "Pending" => match tx_info.status() {
                    grpc::TransactionStatus::Pending |
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Completed" => match tx_info.status() {
                    grpc::TransactionStatus::Completed |
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Broadcast" => match tx_info.status() {
                    grpc::TransactionStatus::Broadcast |
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                "Mined_or_OneSidedUnconfirmed" => match tx_info.status() {
                    grpc::TransactionStatus::MinedUnconfirmed |
                    grpc::TransactionStatus::MinedConfirmed |
                    grpc::TransactionStatus::OneSidedUnconfirmed |
                    grpc::TransactionStatus::OneSidedConfirmed => {
                        break;
                    },
                    _ => (),
                },
                _ => panic!("Unknown status {}, don't know what to expect", status),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[then(expr = "wallet {word} detects all transactions are Broadcast")]
async fn wallet_detects_all_txs_as_broadcast(world: &mut TariWorld, wallet_name: String) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;

    for tx_id in tx_ids {
        println!("waiting for tx with tx_id = {} to be mined_confirmed", tx_id);
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
                    println!(
                        "Transaction with tx_id = {} has been detected as mined_confirmed by wallet {}",
                        tx_id,
                        wallet_name.as_str()
                    );
                    return;
                },
                _ => {
                    println!(
                        "Transaction with tx_id = {} has been detected with status = {:?}",
                        tx_id,
                        tx_info.status()
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                },
            }
        }
    }
}

#[when(expr = "wallet {word} detects last transaction is Pending")]
async fn wallet_detects_last_tx_as_pending(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();
    let tx_id = tx_ids.last().unwrap(); // get last transaction
    let num_retries = 100;

    println!("waiting for tx with tx_id = {} to be pending", tx_id);
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
                println!(
                    "Transaction with tx_id = {} has been detected as pending by wallet {}",
                    tx_id,
                    wallet.as_str()
                );
                return;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            },
        }
    }
}

#[when(expr = "wallet {word} detects last transaction is Cancelled")]
async fn wallet_detects_last_tx_as_cancelled(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();
    let tx_id = tx_ids.last().unwrap(); // get last transaction
    let num_retries = 100;

    println!("waiting for tx with tx_id = {} to be Cancelled", tx_id);
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
                println!("Transaction with tx_id = {} has status {:?}", tx_id, tx_info.status());
                return;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            },
        }
    }
}

#[when(expr = "I list all {word} transactions for wallet {word}")]
#[then(expr = "I list all {word} transactions for wallet {word}")]
async fn list_all_txs_for_wallet(world: &mut TariWorld, transaction_type: String, wallet: String) {
    if transaction_type.as_str() != "COINBASE" && transaction_type.as_str() != "NORMAL" {
        panic!(
            "Invalid transaction type. Values should be COINBASE or NORMAL, value passed is {}",
            transaction_type
        );
    }
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();

    let request = GetCompletedTransactionsRequest {};
    let mut completed_txs = client.get_completed_transactions(request).await.unwrap().into_inner();

    while let Some(tx) = completed_txs.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        if (tx_info.message.contains("Coinbase Transaction for Block ") && transaction_type == "COINBASE") ||
            (!tx_info.message.contains("Coinbase Transaction for Block ") && transaction_type == "NORMAL")
        {
            println!("Transaction with status COINBASE found for wallet {}: ", wallet);
        } else {
            continue;
        }
        println!("\n");
        println!("TxId: {}", tx_info.tx_id);
        println!("Status: {}", tx_info.status);
        println!("IsCancelled: {}", tx_info.is_cancelled);
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
        _ => panic!("Invalid transaction status {}", transaction_status),
    };

    let num_retries = 100;
    let mut current_status = 0;

    for _ in 0..num_retries {
        let mut txs = client
            .get_completed_transactions(grpc::GetCompletedTransactionsRequest {})
            .await
            .unwrap()
            .into_inner();
        let mut found_tx = 0;
        while let Some(tx) = txs.next().await {
            let tx_info = tx.unwrap().transaction.unwrap();
            current_status = tx_info.status;
            if current_status == transaction_status {
                found_tx += 1;
            }
        }
        if found_tx >= num_txs {
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} failed to have at least num {} txs with status {}, current status is {}",
        wallet, num_txs, transaction_status, current_status
    );
}

#[when(expr = "I create a transaction {word} spending {word} to {word}")]
pub async fn create_tx_spending_coinbase(world: &mut TariWorld, transaction: String, inputs: String, output: String) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output(utxos, &world.key_manager).await;
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

    let (tx, utxo) = build_transaction_with_output_and_fee_per_gram(utxos, fee, &world.key_manager).await;
    world.utxos.insert(output, utxo);
    world.transactions.insert(transaction, tx);
}

#[when(expr = "I create a custom locked transaction {word} spending {word} to {word} with lockheight {word}")]
async fn create_tx_custom_lock(
    world: &mut TariWorld,
    transaction: String,
    inputs: String,
    output: String,
    lockheight: u64,
) {
    let inputs = inputs.split(',').collect::<Vec<&str>>();
    let utxos = inputs
        .iter()
        .map(|i| world.utxos.get(&i.to_string()).unwrap().clone())
        .collect::<Vec<_>>();

    let (tx, utxo) = build_transaction_with_output_and_lockheight(utxos, lockheight, &world.key_manager).await;
    world.utxos.insert(output, utxo);
    world.transactions.insert(transaction, tx);
}

#[when(expr = "I wait for wallet {word} to have less than {int} uT")]
async fn wait_for_wallet_to_have_less_than_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    println!("Waiting for wallet {} to have less than {} uT", wallet, amount);

    let num_retries = 100;
    let request = GetBalanceRequest {};

    for _ in 0..num_retries {
        let balance_res = client.get_balance(request.clone()).await.unwrap().into_inner();
        let current_balance = balance_res.available_balance;
        if current_balance < amount {
            println!(
                "Wallet {} now has less than {}, with current balance {}",
                wallet, amount, current_balance
            );
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} didn't get less than {} after num_retries {}",
        wallet, amount, num_retries
    );
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

#[when(expr = "I have {int} non-default wallets connected to all seed nodes using {word}")]
async fn non_default_wallets_connected_to_all_seed_nodes(world: &mut TariWorld, num: u64, mechanism: String) {
    let routing_mechanism = TransactionRoutingMechanism::from(mechanism);
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap();
    for ind in 0..num {
        let wallet_name = format!("Wallet_{}", ind);
        world
            .wallet_connected_to_base_node
            .insert(wallet_name.clone(), node.clone());
        spawn_wallet(
            world,
            wallet_name,
            Some(node.clone()),
            world.all_seed_nodes().to_vec(),
            Some(routing_mechanism),
            None,
        )
        .await;
    }
}

#[when(expr = "I send {int} uT without waiting for broadcast from wallet {word} to wallet {word} at fee {int}")]
#[then(expr = "I send {int} uT without waiting for broadcast from wallet {word} to wallet {word} at fee {int}")]
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
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            source_wallet.as_str(),
            dest_wallet.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
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

    let dest_tx_ids = world.wallet_tx_ids.entry(dest_wallet_address.clone()).or_default();

    dest_tx_ids.push(tx_id);

    println!(
        "Transfer amount {} from {} to {} at fee {} succeeded",
        amount, source_wallet, dest_wallet, fee
    );
}

#[then(expr = "I send a one-sided transaction of {int} uT from {word} to {word} at fee {int}")]
async fn send_one_sided_transaction_from_source_wallet_to_dest_wallt(
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
        message: format!(
            "One sided transfer amount {} from {} to {}",
            amount,
            source_wallet.as_str(),
            dest_wallet.as_str()
        ),
        payment_type: 1, // one sided transaction
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = source_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "One sided transaction with amount {} from wallet {} to {} at fee {} failed",
        amount,
        source_wallet.as_str(),
        dest_wallet.as_str(),
        fee
    );

    // we wait for transaction to be broadcasted
    let tx_id = tx_res.transaction_id;
    let num_retries = 100;
    let tx_info_req = GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };

    for i in 0..num_retries {
        let tx_info_res = source_client
            .get_transaction_info(tx_info_req.clone())
            .await
            .unwrap()
            .into_inner();
        let tx_info = tx_info_res.transactions.first().unwrap();

        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info.status == 1_i32 {
            println!(
                "One sided transaction from {} to {} with amount {} at fee {} has been broadcasted",
                source_wallet.clone(),
                dest_wallet.clone(),
                amount,
                fee
            );
            break;
        }

        if i == num_retries - 1 {
            panic!(
                "One sided transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                source_wallet.clone(),
                dest_wallet.clone(),
                amount,
                fee
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(source_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    let dest_tx_ids = world.wallet_tx_ids.entry(dest_wallet_address.clone()).or_default();

    dest_tx_ids.push(tx_id);

    println!(
        "One sided transaction with amount {} from {} to {} at fee {} succeeded",
        amount, source_wallet, dest_wallet, fee
    );
}

#[then(expr = "I send {int} uT from wallet {word} to wallet {word} at fee {int}")]
#[when(expr = "I send {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn send_amount_from_wallet_to_wallet_at_fee(
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
        message: format!(
            "Transfer amount {} from {} to {} as fee {}",
            amount,
            sender.as_str(),
            receiver.as_str(),
            fee_per_gram
        ),
        payment_type: 0, // mimblewimble transaction
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
    };
    let tx_res = sender_wallet_client.transfer(transfer_req).await.unwrap().into_inner();
    let tx_res = tx_res.results;

    assert_eq!(tx_res.len(), 1usize);

    let tx_res = tx_res.first().unwrap();
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
            println!(
                "Transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries - 1 {
            panic!(
                "Transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    receiver_tx_ids.push(tx_id);

    println!(
        "Transaction with amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, fee_per_gram
    );
}

#[then(expr = "wallet {word} detects at least {int} coinbase transactions as CoinbaseConfirmed")]
async fn wallet_detects_at_least_coinbase_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let mut completed_tx_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;
    let mut total_mined_confirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        println!("Detecting coinbase confirmed transactions");
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

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if total_mined_confirmed_coinbases >= coinbases {
        println!(
            "Wallet {} detected at least {} coinbase transactions as CoinbaseConfirmed",
            &wallet_name, coinbases
        );
    } else {
        panic!(
            "Wallet {} failed to detect at least {} coinbase transactions as CoinbaseConfirmed",
            wallet_name, coinbases
        );
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
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let num_retries = 100;
    let mut total_mined_unconfirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        println!("Detecting coinbase unconfirmed transactions");
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

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if total_mined_unconfirmed_coinbases >= coinbases {
        println!(
            "Wallet {} detected at least {} coinbase transactions as CoinbaseConfirmed",
            &wallet_name, coinbases
        );
    } else {
        panic!(
            "Wallet {} failed to detect at least {} coinbase transactions as CoinbaseConfirmed",
            wallet_name, coinbases
        );
    }
}

#[then(expr = "wallet {word} detects exactly {int} coinbase transactions as CoinbaseConfirmed")]
async fn wallet_detects_exactly_coinbase_transactions(world: &mut TariWorld, wallet_name: String, coinbases: u64) {
    let mut client = create_wallet_client(world, wallet_name.clone()).await.unwrap();
    let wallet_address = world.get_wallet_address(&wallet_name).await.unwrap();
    let tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    let num_retries = 100;
    let mut total_mined_confirmed_coinbases = 0;

    'outer: for _ in 0..num_retries {
        println!("Detecting coinbase confirmed transactions");
        'inner: for tx_id in tx_ids {
            let request = GetTransactionInfoRequest {
                transaction_ids: vec![*tx_id],
            };
            let tx_info = client.get_transaction_info(request).await.unwrap().into_inner();
            let tx_info = tx_info.transactions.first().unwrap();
            match tx_info.status() {
                grpc::TransactionStatus::CoinbaseConfirmed => total_mined_confirmed_coinbases += 1,
                _ => continue 'inner,
            }
        }

        if total_mined_confirmed_coinbases >= coinbases {
            break 'outer;
        } else {
            total_mined_confirmed_coinbases = 0;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if total_mined_confirmed_coinbases == coinbases {
        println!(
            "Wallet {} detected exactly {} coinbase transactions as CoinbaseConfirmed",
            &wallet_name, coinbases
        );
    } else {
        panic!(
            "Wallet {} failed to detect exactly {} coinbase transactions as CoinbaseConfirmed",
            wallet_name, coinbases
        );
    }
}

#[then(expr = "I stop all wallets")]
async fn stop_all_wallets(world: &mut TariWorld) {
    for (wallet, wallet_ps) in &mut world.wallets {
        println!("Stopping wallet {}", wallet);

        wallet_ps.kill();
    }
}

#[then(expr = "I stop wallet {word}")]
#[when(expr = "I stop wallet {word}")]
async fn stop_wallet(world: &mut TariWorld, wallet: String) {
    // conveniently, register wallet address
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = wallet_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    world.wallet_addresses.insert(wallet.clone(), wallet_address);
    println!("Stopping wallet {}", wallet.as_str());
    wallet_ps.kill();
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

#[then(expr = "all wallets detect all transactions as Mined_or_OneSidedConfirmed")]
async fn all_wallets_detect_all_txs_as_mined_confirmed(world: &mut TariWorld) {
    for wallet in world.wallets.keys() {
        let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
        let wallet_address = world.get_wallet_address(&wallet).await.unwrap();
        let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address);

        let wallet_tx_ids = if wallet_tx_ids.is_none() {
            println!("Wallet {} has no available transactions", &wallet);
            vec![]
        } else {
            let wallet_tx_ids = wallet_tx_ids.unwrap();
            if wallet_tx_ids.is_empty() {
                panic!("Wallet {} should have available transaction ids", wallet.as_str());
            }
            wallet_tx_ids.clone()
        };

        let num_retries = 100;

        for tx_id in wallet_tx_ids {
            'inner: for retry in 0..=num_retries {
                let req = GetTransactionInfoRequest {
                    transaction_ids: vec![tx_id],
                };
                let res = wallet_client.get_transaction_info(req).await.unwrap().into_inner();
                let tx_status = res.transactions.first().unwrap().status;

                if tx_status == TransactionStatus::MinedConfirmed as i32 ||
                    tx_status == TransactionStatus::OneSidedConfirmed as i32
                {
                    println!(
                        "Wallet {} has detected transaction with id {} as Mined_or_OneSidedConfirmed",
                        &wallet, tx_id
                    );
                    break 'inner;
                }

                if retry == num_retries {
                    panic!(
                        "Transaction with id {} does not have status as Mined_or_OneSidedConfirmed, on wallet {}",
                        tx_id, &wallet
                    );
                }

                tokio::time::sleep(Duration::from_secs(5)).await;
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
        panic!("Invalid comparison value provided: {}", comparison);
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
                .get_completed_transactions(GetCompletedTransactionsRequest {})
                .await
                .unwrap()
                .into_inner();
            while let Some(completed_tx) = stream.next().await {
                let tx_info = completed_tx.unwrap().transaction.unwrap();
                if tx_info.status == grpc::TransactionStatus::CoinbaseUnconfirmed as i32 {
                    unspendable_coinbase_count += 1;
                    println!(
                        "Found coinbase transaction with id {} for wallet '{}' as 'CoinbaseUnconfirmed'",
                        tx_info.tx_id, &wallet
                    );
                }
                if tx_info.status == grpc::TransactionStatus::CoinbaseNotInBlockChain as i32 {
                    unspendable_coinbase_count += 1;
                    println!(
                        "Found coinbase transaction with id {} for wallet '{}' as 'CoinbaseNotInBlockChain'",
                        tx_info.tx_id, &wallet
                    );
                }
                if tx_info.status == grpc::TransactionStatus::CoinbaseConfirmed as i32 {
                    spendable_coinbase_count += 1;
                    println!(
                        "Found coinbase transaction with id {} for wallet '{}' as 'CoinbaseConfirmed'",
                        tx_info.tx_id, &wallet
                    );
                }
            }

            if spendable_coinbase_count >= amount_of_coinbases {
                println!(
                    "Wallet '{}' has found at least {} spendable coinbases within a total of {} coinbase transactions",
                    &wallet,
                    amount_of_coinbases,
                    spendable_coinbase_count + unspendable_coinbase_count
                );
                break 'inner;
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if comparison == at_least && spendable_coinbase_count >= amount_of_coinbases {
            println!("Wallet {} has found at least {}", &wallet, amount_of_coinbases);
        } else if comparison == exactly && spendable_coinbase_count == amount_of_coinbases {
            println!("Wallet {} has found exactly {}", &wallet, amount_of_coinbases);
        } else {
            panic!(
                "Wallet {} hasn't found {} {} spendable outputs, instead got {}",
                wallet, comparison, amount_of_coinbases, spendable_coinbase_count
            );
        }
    }
}

#[when(expr = "I send {int} transactions of {int} uT each from wallet {word} to wallet {word} at fee_per_gram {int}")]
async fn send_num_transactions_to_wallets_at_fee(
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
            message: format!(
                "transfer amount {} from {} to {}",
                amount,
                sender_wallet.as_str(),
                receiver_wallet.as_str()
            ),
            payment_type: 0, // standard mimblewimble transaction
            payment_id: "".to_string(),
        };
        let transfer_req = TransferRequest {
            recipients: vec![payment_recipient],
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

        let dest_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

        dest_tx_ids.append(&mut tx_ids);

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let num_retries = 100;
    println!(
        "Waiting for transactions from wallet {} to wallet {} to be broadcasted",
        &sender_wallet, &receiver_wallet
    );

    for tx_id in tx_ids {
        println!("Waiting for transaction with id {} to be broadcasted", tx_id);
        let request = GetTransactionInfoRequest {
            transaction_ids: vec![tx_id],
        };

        let mut is_broadcast = false;

        'inner: for _ in 0..num_retries {
            let txs_info = sender_wallet_client
                .get_transaction_info(request.clone())
                .await
                .unwrap()
                .into_inner();
            let txs_info = txs_info.transactions.first().unwrap();

            if txs_info.status == 1 {
                println!(
                    "Transaction from wallet {} to wallet {} with id {} has been broadcasted to the network",
                    &sender_wallet, &receiver_wallet, tx_id
                );
                is_broadcast = true;
                break 'inner;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if !is_broadcast {
            panic!(
                "Transaction from wallet {} to wallet {} with id {} was not broacasted to the network",
                &sender_wallet, &receiver_wallet, tx_id
            );
        }
    }
}

#[then(expr = "I wait for {word} to have {int} node connections")]
async fn wait_for_wallet_to_have_num_connections(world: &mut TariWorld, wallet: String, connections: u64) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    println!("Waiting for wallet {} to have {} connections", &wallet, connections);
    let mut actual_connections = 0_u32;

    for _ in 0..num_retries {
        let network_status_res = wallet_client.get_network_status(Empty {}).await.unwrap().into_inner();
        actual_connections = network_status_res.num_node_connections;
        if u64::from(actual_connections) >= connections {
            println!("Wallet {} has at least {} connections", &wallet, connections);
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if u64::from(actual_connections) != connections {
        panic!("Wallet {} does not have {} connections", &wallet, connections);
    }
}

#[then(expr = "I wait for {word} to have {word} connectivity")]
async fn wait_for_wallet_to_have_specific_connectivity(world: &mut TariWorld, wallet: String, connectivity: String) {
    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    println!("Waiting for wallet {} to have connectivity {}", &wallet, &connectivity);
    let connectivity = connectivity.to_uppercase();

    let connectivity_index = match connectivity.as_str() {
        "INITIALIZING" => 0,
        "ONLINE" => 1,
        "DEGRADED" => 2,
        "OFFLINE" => 3,
        _ => panic!("Invalid connectivity value {}", connectivity),
    };

    for _ in 0..=num_retries {
        let network_status_res = wallet_client.get_network_status(Empty {}).await.unwrap().into_inner();
        let connectivity_status = network_status_res.status;
        if connectivity_status == connectivity_index {
            println!("Wallet {} has {} connectivity", &wallet, &connectivity);
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} did not get correct connectivity status {}",
        &wallet, connectivity
    );
}

#[when(expr = "I transfer {int}T from {word} to {word}")]
async fn transfer_tari_from_wallet_to_receiver(world: &mut TariWorld, amount: u64, sender: String, receiver: String) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();
    let receiver_wallet_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount: amount * 1_000_000_u64, // 1T = 1_000_000uT
        fee_per_gram: 10,               // as in the js cucumber tests
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
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

    // we wait for transaction to be broadcasted
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
            println!(
                "Transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                10
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let source_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    source_tx_ids.push(tx_id);

    let dest_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    dest_tx_ids.push(tx_id);

    println!(
        "Transfer amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, 10
    );
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
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner();

        available_balance = balance_res.available_balance;
        if available_balance >= amount * 1_000_000 {
            println!("Wallet {} has at least {}T", wallet.as_str(), amount);
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!(
        "Wallet {} failed to have at least {}T, it ended with {}T",
        wallet, amount, available_balance
    );
}

#[when(expr = "I have wallet {word} with {int}T connected to base node {word}")]
async fn wallet_with_tari_connected_to_base_node(
    world: &mut TariWorld,
    wallet: String,
    amount: u64,
    base_node: String,
) {
    let peer_seeds = world.base_nodes.get(&base_node).unwrap().seed_nodes.clone();
    println!(
        "Start a new wallet {} connected to base node {}",
        wallet.as_str(),
        base_node.as_str()
    );
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
        reward += world.consensus_manager.get_block_reward_at(current_height).as_u64() / 1_000_000; // 1 T = 1_000_000
                                                                                                    // uT
    }

    println!("Creating miner...");
    create_miner(world, "temp_miner".to_string(), base_node.clone(), wallet.clone()).await;

    println!("Mining {} blocks", num_blocks + CONFIRMATION_PERIOD);
    let miner = world.miners.get(&"temp_miner".to_string()).unwrap();
    miner
        .mine(world, Some(num_blocks + CONFIRMATION_PERIOD), None, None)
        .await; // mine some additional blocks to confirm txs

    let mut wallet_client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let num_retries = 100;

    for _ in 0..num_retries {
        let _result = wallet_client.validate_all_transactions(ValidateRequest {}).await;
        let balance_res = wallet_client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner();

        if balance_res.available_balance >= amount * 1_000_000 {
            println!("Wallet {} has at least {}T", wallet.as_str(), amount);
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!("Wallet {} failed to have at least {}T", wallet, amount);
}

#[when(expr = "I transfer {int} uT from {word} to {word} and {word} at fee {int}")]
#[allow(clippy::too_many_lines)]
async fn transfer_from_wallet_to_two_recipients_at_fee(
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
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver1.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
        payment_id: "".to_string(),
    };

    let payment_recipient2 = PaymentRecipient {
        address: receiver2_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver2.as_str()
        ),
        payment_type: 0, // normal mimblewimble payment type
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient1, payment_recipient2],
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

    // we wait for transaction to be broadcasted
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

        println!(
            "Tx_info for first recipient {} is {}, for tx_id = {}",
            receiver1, tx_info1.status, tx_id1
        );
        println!(
            "Tx_info for second recipient {} is {}, for tx_id = {}",
            receiver2, tx_info2.status, tx_id2
        );
        // TransactionStatus::TRANSACTION_STATUS_BROADCAST == 1_i32
        if tx_info1.status == 1_i32 && tx_info2.status == 1_i32 {
            println!(
                "Transaction from {} to {} and {} with amount {} at fee {} has been broadcasted",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Transaction from {} to {} and {} with amount {} at fee {} failed to be broadcasted",
                sender.as_str(),
                receiver1.as_str(),
                receiver2.as_str(),
                amount,
                10
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id1);
    sender_tx_ids.push(tx_id2);

    let receiver1_tx_ids = world.wallet_tx_ids.entry(receiver1_address.clone()).or_default();
    receiver1_tx_ids.push(tx_id1);

    let receiver2_tx_ids = world.wallet_tx_ids.entry(receiver2_address.clone()).or_default();
    receiver2_tx_ids.push(tx_id2);

    println!(
        "Transfer amount {} from {} to {} and {} at fee {} succeeded",
        amount, sender, receiver1, receiver2, fee_per_gram
    );
}

#[when(expr = "I transfer {int} uT to self from wallet {word} at fee {int}")]
async fn transfer_tari_to_self(world: &mut TariWorld, amount: u64, sender: String, fee_per_gram: u64) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = world.get_wallet_address(&sender).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: sender_wallet_address.clone(),
        amount,
        fee_per_gram,
        message: format!("transfer amount {} from {} to self", amount, sender.as_str(),),
        payment_type: 0, // normal mimblewimble payment type
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
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

    // we wait for transaction to be broadcasted
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
            println!(
                "Transaction to self from {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Transaction to self from {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    println!(
        "Transfer amount {} to self from {} at fee {} succeeded",
        amount, sender, fee_per_gram
    );
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
        message: format!(
            "Atomic Swap from {} to {} with amount {} at fee {}",
            sender.as_str(),
            receiver.as_str(),
            amount,
            fee_per_gram
        ),
        payment_type: 0, // normal mimblewimble transaction
        payment_id: "".to_string(),
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

    // we wait for transaction to be broadcasted
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
            println!(
                "Atomic swap transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Atomic swap transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.as_str(),
                receiver.as_str(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    receiver_tx_ids.push(tx_id);
    world.output_hash = Some(sha_atomic_swap_tx_res.output_hash);
    world.pre_image = Some(sha_atomic_swap_tx_res.pre_image);

    println!(
        "Atomic swap transfer amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, fee_per_gram
    );
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

    // we wait for transaction to be broadcasted
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
            println!(
                "Claim HTLC refund transaction with wallet {} at fee {} has been broadcasted",
                wallet.as_str(),
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Claim HTLC refund transaction with wallet {} at fee {} failed to be broadcasted",
                wallet.as_str(),
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let wallet_tx_ids = world.wallet_tx_ids.entry(wallet_address.clone()).or_default();
    wallet_tx_ids.push(tx_id);

    println!(
        "Claim HTLC refund transaction with wallet {} at fee {} succeeded",
        wallet, fee_per_gram
    );
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

    // we wait for transaction to be broadcasted
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
            println!(
                "Claim HTLC transaction with wallet {} at fee {} has been broadcasted",
                wallet.as_str(),
                fee_per_gram
            );
            break;
        }

        if i == num_retries {
            panic!(
                "Claim HTLC transaction with wallet {} at fee {} failed to be broadcasted",
                wallet.as_str(),
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let wallet_tx_ids = world.wallet_tx_ids.entry(wallet_address.clone()).or_default();
    wallet_tx_ids.push(tx_id);

    println!(
        "Claim HTLC transaction with wallet {} at fee {} succeeded",
        wallet, fee_per_gram
    );
}

#[then(expr = "I wait for wallet {word} to have less than {int} uT")]
async fn wait_for_wallet_to_have_less_than_amount(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet_ps = world.wallets.get(&wallet).unwrap();
    let num_retries = 100;

    let mut client = wallet_ps.get_grpc_client().await.unwrap();
    let mut curr_amount = u64::MAX;

    for _ in 0..=num_retries {
        let _result = client.validate_all_transactions(ValidateRequest {}).await;
        curr_amount = client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner()
            .available_balance;

        if curr_amount < amount {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // failed to get wallet right amount, so we panic
    panic!(
        "wallet {} failed to get less balance than amount {}, current amount is {}",
        wallet.as_str(),
        amount,
        curr_amount
    );
}

#[then(expr = "I send a one-sided stealth transaction of {int} uT from {word} to {word} at fee {int}")]
async fn send_one_sided_stealth_transaction(
    world: &mut TariWorld,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = sender_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let mut receiver_client = create_wallet_client(world, receiver.clone()).await.unwrap();
    let receiver_wallet_address = receiver_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let payment_recipient = PaymentRecipient {
        address: receiver_wallet_address.clone(),
        amount,
        fee_per_gram,
        message: format!(
            "One sided stealth transfer amount {} from {} to {}",
            amount,
            sender.as_str(),
            receiver.as_str()
        ),
        payment_type: 2, // one sided stealth transaction
        payment_id: "".to_string(),
    };
    let transfer_req = TransferRequest {
        recipients: vec![payment_recipient],
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

    // we wait for transaction to be broadcasted
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
            println!(
                "One sided stealth transaction from {} to {} with amount {} at fee {} has been broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            );
            break;
        }

        if i == num_retries - 1 {
            panic!(
                "One sided stealth transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                sender.clone(),
                receiver.clone(),
                amount,
                fee_per_gram
            )
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // insert tx_id's to the corresponding world mapping
    let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

    sender_tx_ids.push(tx_id);

    let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

    receiver_tx_ids.push(tx_id);

    println!(
        "One sided stealth transaction with amount {} from {} to {} at fee {} succeeded",
        amount, sender, receiver, fee_per_gram
    );
}

#[then(expr = "I import {word} unspent outputs to {word}")]
async fn import_wallet_unspent_outputs(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();

    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;

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
        let coinbase_extra = Vec::from_hex(&output[7]).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = PublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment: HomomorphicCommitment<PublicKey> =
            HomomorphicCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = PublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_data = EncryptedData::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroMinotari(output[20].parse::<u64>().unwrap());

        let features =
            OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None, RangeProofType::BulletProofPlus);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_x,
            signature_u_a,
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
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversion"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I import {word} spent outputs to {word}")]
async fn import_wallet_spent_outputs(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportSpentUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;

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
        let coinbase_extra = Vec::from_hex(&output[7]).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = PublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment: HomomorphicCommitment<PublicKey> =
            HomomorphicCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = PublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_data = EncryptedData::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroMinotari(output[20].parse::<u64>().unwrap());

        let features =
            OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None, RangeProofType::BulletProofPlus);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_x,
            signature_u_a,
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
        );

        outputs.push(utxo);
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversion"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}
#[allow(clippy::too_many_lines)]
#[then(expr = "I import {word} unspent outputs as faucet outputs to {word}")]
async fn import_unspent_outputs_as_faucets(world: &mut TariWorld, wallet_a: String, wallet_b: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet_a).unwrap();

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;

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
        let coinbase_extra = Vec::from_hex(&output[7]).unwrap();
        let script = TariScript::from_hex(&output[8]).unwrap();
        let covenant = Covenant::from_bytes(&mut Vec::from_hex(&output[9]).unwrap().as_slice()).unwrap();
        let input_data = ExecutionStack::from_hex(&output[10]).unwrap();
        let script_private_key = PrivateKey::from_hex(&output[11]).unwrap();
        let sender_offset_public_key = PublicKey::from_hex(&output[12]).unwrap();
        let ephemeral_commitment: HomomorphicCommitment<PublicKey> =
            HomomorphicCommitment::from_hex(&output[13]).unwrap();
        let ephemeral_nonce = PublicKey::from_hex(&output[14]).unwrap();
        let signature_u_x = PrivateKey::from_hex(&output[15]).unwrap();
        let signature_u_a = PrivateKey::from_hex(&output[16]).unwrap();
        let signature_u_y = PrivateKey::from_hex(&output[17]).unwrap();
        let script_lock_height = output[18].parse::<u64>().unwrap();
        let encrypted_data = EncryptedData::from_hex(&output[19]).unwrap();
        let minimum_value_promise = MicroMinotari(output[20].parse::<u64>().unwrap());

        let features =
            OutputFeatures::new_current_version(flags, maturity, coinbase_extra, None, RangeProofType::BulletProofPlus);
        let metadata_signature = ComAndPubSignature::new(
            ephemeral_commitment,
            ephemeral_nonce,
            signature_u_x,
            signature_u_a,
            signature_u_y,
        );
        let mut utxo = UnblindedOutput::new(
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
        );

        utxo.metadata_signature = ComAndPubSignature::new(
            Commitment::default(),
            PublicKey::default(),
            PrivateKey::default(),
            PrivateKey::default(),
            PrivateKey::default(),
        );
        utxo.script_private_key = utxo.clone().spending_key;

        let script_public_key = PublicKey::from_secret_key(&utxo.script_private_key);
        utxo.input_data = ExecutionStack::new(vec![StackItem::PublicKey(script_public_key)]);
        outputs.push(utxo.clone());
    }

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let import_utxos_req = ImportUtxosRequest {
        outputs: outputs
            .iter()
            .map(|o| grpc::UnblindedOutput::try_from(o.clone()).expect("Unable to make grpc conversion"))
            .collect::<Vec<grpc::UnblindedOutput>>(),
    };

    world.last_imported_tx_ids = wallet_b_client
        .import_utxos(import_utxos_req)
        .await
        .unwrap()
        .into_inner()
        .tx_ids;
}

#[then(expr = "I restart wallet {word}")]
async fn restart_wallet(world: &mut TariWorld, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    // stop wallet
    wallet_ps.kill();
    // start wallet
    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap().clone();
    let base_node_ps = world.base_nodes.get(&base_node).unwrap();
    let seed_nodes = base_node_ps.seed_nodes.clone();

    // need to wait a few seconds before spawning a new wallet
    tokio::time::sleep(Duration::from_secs(5)).await;

    spawn_wallet(world, wallet, Some(base_node), seed_nodes, None, None).await;
}

#[then(expr = "I check if wallet {word} has {int} transactions")]
async fn check_if_wallet_has_num_transactions(world: &mut TariWorld, wallet: String, num_txs: u64) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
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

#[when(expr = "I multi-send {int} transactions of {int} uT from wallet {word} to wallet {word} at fee {int}")]
async fn multi_send_txs_from_wallet(
    world: &mut TariWorld,
    num_txs: u64,
    amount: u64,
    sender: String,
    receiver: String,
    fee_per_gram: u64,
) {
    let mut sender_wallet_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let sender_wallet_address = sender_wallet_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let mut receiver_wallet_client = create_wallet_client(world, receiver.clone()).await.unwrap();
    let receiver_wallet_address = receiver_wallet_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let mut transfer_res = vec![];

    for _ in 0..num_txs {
        let payment_recipient = PaymentRecipient {
            address: receiver_wallet_address.clone(),
            amount,
            fee_per_gram,
            message: format!(
                "I send multi-transfers with amount {} from {} to {} with fee per gram {}",
                amount,
                sender.as_str(),
                receiver.as_str(),
                fee_per_gram
            ),
            payment_type: 0, // mimblewimble transaction
            payment_id: "".to_string(),
        };

        let transfer_req = TransferRequest {
            recipients: vec![payment_recipient],
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
                println!(
                    "Multi-transaction from {} to {} with amount {} at fee {} has been broadcasted",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                );
                break;
            }

            if i == num_retries - 1 {
                panic!(
                    "Multi-transaction from {} to {} with amount {} at fee {} failed to be broadcasted",
                    sender.clone(),
                    receiver.clone(),
                    amount,
                    fee_per_gram
                )
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        // insert tx_id's to the corresponding world mapping
        let sender_tx_ids = world.wallet_tx_ids.entry(sender_wallet_address.clone()).or_default();

        sender_tx_ids.push(tx_id);

        let receiver_tx_ids = world.wallet_tx_ids.entry(receiver_wallet_address.clone()).or_default();

        receiver_tx_ids.push(tx_id);

        println!(
            "Multi-transaction with amount {} from {} to {} at fee {} succeeded",
            amount, sender, receiver, fee_per_gram
        );
    }
}

#[then(expr = "I check if last imported transactions are invalid in wallet {word}")]
async fn check_if_last_imported_txs_are_invalid_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    while let Some(tx) = get_completed_txs_res.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        let status = tx_info.status;
        // 3 => TRANSACTION_STATUS_IMPORTED
        // 5 => TRANSACTION_STATUS_COINBASE
        if ![3, 5].contains(&status) {
            panic!(
                "Imported transaction hasn't been received as such: current status code is {}, it should be 3 or 5",
                status
            );
        }
    }
}

#[then(expr = "I check if last imported transactions are valid in wallet {word}")]
async fn check_if_last_imported_txs_are_valid_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let mut get_completed_txs_res = client
        .get_completed_transactions(GetCompletedTransactionsRequest {})
        .await
        .unwrap()
        .into_inner();

    let mut imported_cnt = 0;

    while let Some(tx) = get_completed_txs_res.next().await {
        let tx_info = tx.unwrap().transaction.unwrap();
        for &tx_id in &world.last_imported_tx_ids {
            if tx_id == tx_info.tx_id {
                assert_eq!(tx_info.status(), grpc::TransactionStatus::OneSidedConfirmed);
                imported_cnt += 1;
            }
        }
    }
    assert_eq!(imported_cnt, world.last_imported_tx_ids.len());
}

#[then(expr = "I cancel last transaction in wallet {word}")]
async fn cancel_last_transaction_in_wallet(world: &mut TariWorld, wallet: String) {
    let mut client = create_wallet_client(world, wallet.clone()).await.unwrap();
    let wallet_address = client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();

    let wallet_tx_ids = world.wallet_tx_ids.get(&wallet_address).unwrap();

    // get the last tx id for wallet
    let tx_id = *wallet_tx_ids.last().unwrap();
    let cancel_tx_req = CancelTransactionRequest { tx_id };
    let cancel_tx_res = client.cancel_transaction(cancel_tx_req).await.unwrap().into_inner();
    assert!(
        cancel_tx_res.is_success,
        "Unable to cancel transaction with id = {}",
        tx_id
    );
}

#[when(expr = "I create a burn transaction of {int} uT from {word} at fee {int}")]
async fn burn_transaction(world: &mut TariWorld, amount: u64, wallet: String, fee: u64) {
    let mut client = world.get_wallet_client(&wallet).await.unwrap();
    let identity = client.identify(GetIdentityRequest {}).await.unwrap().into_inner();

    let req = grpc::CreateBurnTransactionRequest {
        amount,
        fee_per_gram: fee,
        message: "Burning some tari".to_string(),
        claim_public_key: identity.public_key,
    };

    let result = client.create_burn_transaction(req).await.unwrap();
    let tx_id = result.into_inner().transaction_id;

    let mut last_status = 0;
    for _ in 0..(TWO_MINUTES_WITH_HALF_SECOND_SLEEP) {
        let result = client
            .get_transaction_info(grpc::GetTransactionInfoRequest {
                transaction_ids: vec![tx_id],
            })
            .await
            .unwrap();

        last_status = result.into_inner().transactions.last().unwrap().status;

        if let 1 | 2 | 6 = last_status {
            return;
        }

        tokio::time::sleep(Duration::from_millis(HALF_SECOND)).await;
    }

    panic!(
        "Burn transaction has status {} when we desired 1 (TRANSACTION_STATUS_BROADCAST), 2 \
         (TRANSACTION_STATUS_UNCONFIRMED), or 6 (TRANSACTION_STATUS_CONFIRMED)",
        last_status
    )
}
