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
use std::{convert::TryFrom, path::PathBuf, str::FromStr, time::Duration};

use cucumber::{then, when};
use minotari_app_grpc::tari_rpc::{Empty, PaymentRecipient, TransferRequest};
use minotari_app_utilities::utilities::UniPublicKey;
use minotari_console_wallet::{
    CliCommands,
    CoinSplitArgs,
    ExportUtxosArgs,
    ExportViewKeyAndSpendKeyArgs,
    MakeItRainArgs,
    WhoisArgs,
};
use tari_common_types::{seeds::seed_words::SeedWords, tari_address::TariAddress};
use tari_integration_tests::{
    TariWorld,
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet},
};
use tari_transaction_components::{
    MicroMinotari,
    transaction_components::memo_field::{MemoField, TxType},
};
use tari_utilities::hex::Hex;

use crate::steps::get_saved_seed_words;

#[then(expr = "the password of wallet {word} is not {word}")]
async fn password_is(world: &mut TariWorld, wallet: String, _password: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    let _config_path = wallet_ps.temp_dir_path.clone();
}

#[then(expr = "I get balance of wallet {word} is at least {int} uT via command line")]
async fn get_balance_of_wallet(world: &mut TariWorld, wallet: String, _amount: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill().await;
    tari_integration_tests::wait_for!(
        timeout: Duration::from_secs(10),
        description: "wallet to shut down",
        condition: async { Ok(!wallet_ps.is_running()) }
    );

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::GetBalance);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await
}

#[when(
    expr = "I make-it-rain from {word} rate {int} txns_per_sec duration {int} sec value {int} uT increment {int} uT \
            to {word} via command line"
)]
async fn make_it_rain(
    world: &mut TariWorld,
    wallet_a: String,
    txs_per_second: u32,
    duration: u64,
    start_amount: u64,
    increment_amount: u64,
    wallet_b: String,
) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill().await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .interactive_address
        .to_hex();
    let wallet_b_address = TariAddress::from_base58(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = MakeItRainArgs {
        start_amount: MicroMinotari(start_amount),
        transactions_per_second: f64::from(txs_per_second),
        duration: Duration::from_secs(duration),
        increase_amount: MicroMinotari(increment_amount),
        destination: wallet_b_address,
        start_time: None,
        one_sided: false,
        payment_id: format!("Make it raing amount {start_amount} from {wallet_a} to {wallet_b}"),
    };

    cli.command2 = Some(CliCommands::MakeItRain(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I do coin split on wallet {word} to {int} uT {int} coins via command line")]
async fn coin_split_via_cli(world: &mut TariWorld, wallet: String, amount: u64, splits: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill().await;

    let mut cli = get_default_cli();

    let args = CoinSplitArgs {
        amount_per_split: MicroMinotari(amount),
        num_splits: usize::try_from(splits).unwrap(),
        fee_per_gram: MicroMinotari(20),
        payment_id: format!("coin split amount {amount} with splits {splits}"),
    };

    cli.command2 = Some(CliCommands::CoinSplit(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I get count of utxos of wallet {word} and it's at least {int} via command line")]
async fn count_utxos_of_wallet(world: &mut TariWorld, wallet: String, _amount: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill().await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::CountUtxos);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I export the utxos of wallet {word} via command line")]
async fn export_utxos(world: &mut TariWorld, wallet: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_a_ps.kill().await;

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

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();

    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I run whois {word} on wallet {word} via command line")]
async fn whois(world: &mut TariWorld, node: String, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill().await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = WhoisArgs {
        public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::Whois(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I recover wallet {word} into wallet {word} from seed words on node {word}")]
async fn recover_wallet_via_cli(
    world: &mut TariWorld,
    source_wallet_name: String,
    target_wallet_name: String,
    node: String,
) {
    if let Some(wallet_ps) = world.wallets.get_mut(&target_wallet_name) {
        wallet_ps.kill().await;
    }

    let mut cli = get_default_cli();

    cli.recovery = true;
    let saved_seed_words = get_saved_seed_words(world, &source_wallet_name);
    let mut seed_words = SeedWords::new(vec![]);
    for word in &saved_seed_words {
        seed_words.push(word.to_string());
    }
    cli.seed_words = Some(seed_words);

    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(
        world,
        target_wallet_name,
        Some(node.clone()),
        seed_nodes,
        None,
        Some(cli),
    )
    .await;
}

#[then(expr = "I export wallet {word} view and spend keys as {word}")]
async fn export_wallet_view_and_spend_keys_via_cli(
    world: &mut TariWorld,
    wallet_name: String,
    view_and_spend_key: String,
) {
    let wallet_ps = if let Some(wallet_ps) = world.wallets.get_mut(&wallet_name) {
        wallet_ps.kill().await;
        wallet_ps.clone()
    } else {
        panic!("Wallet '{wallet_name}' not found");
    };

    let mut cli = get_default_cli();

    let output_file = wallet_ps.temp_dir_path.clone().join("view_and_spend_key.txt");
    let args = ExportViewKeyAndSpendKeyArgs {
        output_file: Some(output_file.clone()),
    };
    cli.command2 = Some(CliCommands::ExportViewKeyAndSpendKey(args));

    world.view_and_spend_keys.insert(view_and_spend_key, output_file);

    spawn_wallet(
        world,
        wallet_name,
        wallet_ps.base_node_name.clone(),
        wallet_ps.peer_seeds.clone(),
        None,
        Some(cli),
    )
    .await;
}

#[then(expr = "I create view wallet {word} from view and spend keys {word} on node {word}")]
async fn recover_wallet_from_view_and_spend_keys_via_cli(
    world: &mut TariWorld,
    wallet_name: String,
    view_and_spend_key: String,
    node: String,
) {
    if let Some(wallet_ps) = world.wallets.get_mut(&wallet_name) {
        wallet_ps.kill().await;
        world.wallets.remove(&wallet_name);
    }

    // Extract view_key and spend_key from the file with format:
    // {"view_key":"c593a5d131d46ece6d08c39693b1260aeb502d25c0e898358e6fc1b2b19fe404","public_view_key":"
    // 5ade435cc6947ac2979c0ea2f3d6a1f64d0c35b1995d48a60e2720c283dd3e38","spend_key":"
    // 9e9c7d4eedb70a1e31bfaad1ad38ffc7340a4b61120e6c911afcd702219c1764","birthday":1252}
    let keys_file = world.view_and_spend_keys.get(&view_and_spend_key);
    let keys_file = if let Some(file) = keys_file {
        file
    } else {
        panic!("View and spend keys file not found for '{view_and_spend_key}'");
    };
    let keys_content = std::fs::read_to_string(keys_file).unwrap_or_else(|e| panic!("Failed to read keys file: {e}"));
    let keys_json: serde_json::Value =
        serde_json::from_str(&keys_content).unwrap_or_else(|e| panic!("Failed to parse keys JSON: {e}"));
    let view_key = keys_json["view_key"]
        .as_str()
        .unwrap_or_else(|| panic!("Missing 'view_key' in keys file"));
    let spend_key = keys_json["spend_key"]
        .as_str()
        .unwrap_or_else(|| panic!("Missing 'spend_key' in keys file"));

    let mut cli = get_default_cli();

    cli.view_private_key = Some(view_key.to_string());
    cli.spend_key = Some(spend_key.to_string());

    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_name, Some(node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I change base node of {word} to {word} via command line")]
async fn change_base_node_via_cli(world: &mut TariWorld, wallet: String, base_node: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill().await;
    let seed_nodes = world.base_nodes.get(&base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node), seed_nodes, None, None).await;
}

#[then(expr = "I set custom base node of {word} to {word} via command line")]
async fn set_custom_base_node_via_cli(world: &mut TariWorld, wallet: String, base_node: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill().await;
    let seed_nodes = world.base_nodes.get(&base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node), seed_nodes, None, None).await;
}

#[when(expr = "I clear custom base node of wallet {word} via command line")]
async fn clear_custom_base_node_via_cli(world: &mut TariWorld, wallet: String) {
    let (base_node_name, peer_seeds) = {
        let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
        wallet_ps.kill().await;
        (wallet_ps.base_node_name.clone(), wallet_ps.peer_seeds.clone())
    };
    spawn_wallet(world, wallet, base_node_name, peer_seeds, None, None).await;
}

#[when(expr = "I send {int} uT from {word} to {word} via command line")]
async fn send_via_cli(world: &mut TariWorld, amount: u64, sender: String, receiver: String) {
    let mut sender_client = create_wallet_client(world, sender.clone()).await.unwrap();
    let receiver_address = world.get_wallet_address(&receiver).await.unwrap();

    let payment_recipient = PaymentRecipient {
        address: receiver_address,
        amount,
        fee_per_gram: 20,
        payment_type: 2, // one-sided stealth transaction
        raw_payment_id: MemoField::new_open_from_string(
            &format!("CLI send {} uT from {} to {}", amount, sender, receiver),
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
    assert!(
        !tx_res.is_empty(),
        "Send from {} to {} returned no results",
        sender,
        receiver
    );
    let tx_res = tx_res.first().unwrap();
    assert!(
        tx_res.is_success,
        "Send {} uT from {} to {} failed: {}",
        amount, sender, receiver, tx_res.failure_message
    );
}

#[when(expr = "I recover wallet {word} into wallet {word} connected to all seed nodes")]
async fn recover_wallet_into_wallet_connected_to_all_seed_nodes(
    world: &mut TariWorld,
    source_wallet_name: String,
    target_wallet_name: String,
) {
    if let Some(wallet_ps) = world.wallets.get_mut(&target_wallet_name) {
        wallet_ps.kill().await;
    }

    let mut cli = get_default_cli();
    cli.recovery = true;
    let saved_seed_words = get_saved_seed_words(world, &source_wallet_name);
    let mut seed_words = SeedWords::new(vec![]);
    for word in &saved_seed_words {
        seed_words.push(word.to_string());
    }
    cli.seed_words = Some(seed_words);

    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap().clone();
    spawn_wallet(world, target_wallet_name, Some(node), nodes, None, Some(cli)).await;
}

#[when(expr = "I recover all wallets connected to all seed nodes")]
async fn recover_all_wallets_connected_to_all_seed_nodes(world: &mut TariWorld) {
    let wallet_names: Vec<String> = world.wallets.keys().cloned().collect();
    let nodes = world.all_seed_nodes().to_vec();
    let node = nodes.first().unwrap().clone();

    for wallet_name in wallet_names {
        if let Some(wallet_ps) = world.wallets.get_mut(&wallet_name) {
            wallet_ps.kill().await;
        }

        // Delete the wallet data directory so recovery can start fresh (boot() rejects recovery if db exists)
        if let Some(wallet_ps) = world.wallets.get(&wallet_name) {
            let wallet_data_dir = wallet_ps.temp_dir_path.join("data").join("wallet");
            if wallet_data_dir.exists() {
                std::fs::remove_dir_all(&wallet_data_dir).unwrap();
            }
        }

        let mut cli = get_default_cli();
        cli.recovery = true;
        let saved_seed_words = get_saved_seed_words(world, &wallet_name);
        let mut seed_words = SeedWords::new(vec![]);
        for word in &saved_seed_words {
            seed_words.push(word.to_string());
        }
        cli.seed_words = Some(seed_words);

        spawn_wallet(world, wallet_name, Some(node.clone()), nodes.clone(), None, Some(cli)).await;
    }
}
