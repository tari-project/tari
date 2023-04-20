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

use std::{path::PathBuf, str::FromStr, time::Duration};

use cucumber::{then, when};
use tari_app_grpc::tari_rpc::Empty;
use tari_app_utilities::utilities::UniPublicKey;
use tari_common_types::tari_address::TariAddress;
use tari_comms::multiaddr::Multiaddr;
use tari_console_wallet::{
    BurnTariArgs,
    CliCommands,
    CoinSplitArgs,
    DiscoverPeerArgs,
    ExportUtxosArgs,
    MakeItRainArgs,
    SendTariArgs,
    SetBaseNodeArgs,
    WhoisArgs,
};
use tari_core::transactions::tari_amount::MicroTari;
use tari_integration_tests::{
    wallet_process::{create_wallet_client, get_default_cli, spawn_wallet},
    TariWorld,
};
use tari_utilities::hex::Hex;

#[then(expr = "I change base node of {word} to {word} via command line")]
async fn change_base_node_of_wallet_via_cli(world: &mut TariWorld, wallet: String, node: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = SetBaseNodeArgs {
        public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
        address: Multiaddr::from_str(node_identity.public_addresses[0].as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::SetBaseNode(args));

    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I set custom base node of {word} to {word} via command line")]
async fn change_custom_base_node_of_wallet_via_cli(world: &mut TariWorld, wallet: String, node: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = SetBaseNodeArgs {
        public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
        address: Multiaddr::from_str(node_identity.public_addresses[0].as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::SetCustomBaseNode(args));

    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I clear custom base node of wallet {word} via command line")]
async fn clear_custom_base_node(world: &mut TariWorld, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::ClearCustomBaseNode);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "the password of wallet {word} is not {word}")]
async fn password_is(world: &mut TariWorld, wallet: String, _password: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    let _config_path = wallet_ps.temp_dir_path.clone();
}

#[then(expr = "I get balance of wallet {word} is at least {int} uT via command line")]
async fn get_balance_of_wallet(world: &mut TariWorld, wallet: String, _amount: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::GetBalance);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await
}

#[when(expr = "I send {int} uT from {word} to {word} via command line")]
async fn send_from_cli(world: &mut TariWorld, amount: u64, wallet_a: String, wallet_b: String) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_b_address = TariAddress::from_hex(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = SendTariArgs {
        amount: MicroTari(amount),
        message: format!("Send amount {} from {} to {}", amount, wallet_a, wallet_b),
        destination: wallet_b_address,
    };
    cli.command2 = Some(CliCommands::SendTari(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I create a burn transaction of {int} uT from {word} via command line")]
async fn create_burn_tx_via_cli(world: &mut TariWorld, amount: u64, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let args = BurnTariArgs {
        amount: MicroTari(amount),
        message: format!("Burn, burn amount {} !!!", amount,),
    };
    cli.command2 = Some(CliCommands::BurnTari(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I send one-sided {int} uT from {word} to {word} via command line")]
async fn send_one_sided_tx_via_cli(world: &mut TariWorld, amount: u64, wallet_a: String, wallet_b: String) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_b_address = TariAddress::from_hex(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = SendTariArgs {
        amount: MicroTari(amount),
        message: format!("Send one sided amount {} from {} to {}", amount, wallet_a, wallet_b),
        destination: wallet_b_address,
    };
    cli.command2 = Some(CliCommands::SendOneSided(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(
    expr = "I make it rain from wallet {word} {int} tx per sec {int} sec {int} uT {int} increment to {word} via \
            command line"
)]
async fn make_it_rain(
    world: &mut TariWorld,
    wallet_a: String,
    txs_per_second: u64,
    duration: u64,
    start_amount: u64,
    increment_amount: u64,
    wallet_b: String,
) {
    let wallet_ps = world.wallets.get_mut(&wallet_a).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut wallet_b_client = create_wallet_client(world, wallet_b.clone()).await.unwrap();
    let wallet_b_address = wallet_b_client
        .get_address(Empty {})
        .await
        .unwrap()
        .into_inner()
        .address
        .to_hex();
    let wallet_b_address = TariAddress::from_hex(wallet_b_address.as_str()).unwrap();

    let mut cli = get_default_cli();

    let args = MakeItRainArgs {
        start_amount: MicroTari(start_amount),
        transactions_per_second: txs_per_second as u32,
        duration: Duration::from_secs(duration),
        message: format!(
            "Make it raing amount {} from {} to {}",
            start_amount, wallet_a, wallet_b
        ),
        increase_amount: MicroTari(increment_amount),
        destination: wallet_b_address,
        start_time: None,
        one_sided: false,
        stealth: false,
        burn_tari: false,
    };

    cli.command2 = Some(CliCommands::MakeItRain(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet_a).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet_a, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I do coin split on wallet {word} to {int} uT {int} coins via command line")]
async fn coin_split_via_cli(world: &mut TariWorld, wallet: String, amount: u64, splits: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let args = CoinSplitArgs {
        amount_per_split: MicroTari(amount),
        num_splits: splits as usize,
        fee_per_gram: MicroTari(20),
        message: format!("coin split amount {} with splits {}", amount, splits),
    };

    cli.command2 = Some(CliCommands::CoinSplit(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I get count of utxos of wallet {word} and it's at least {int} via command line")]
async fn count_utxos_of_wallet(world: &mut TariWorld, wallet: String, _amount: u64) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    cli.command2 = Some(CliCommands::CountUtxos);

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();

    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I export the utxos of wallet {word} via command line")]
async fn export_utxos(world: &mut TariWorld, wallet: String) {
    let wallet_a_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_a_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let temp_dir_path = wallet_a_ps.temp_dir_path.clone();

    let mut cli = get_default_cli();

    let mut path_buf = PathBuf::new();
    path_buf.push(temp_dir_path);
    path_buf.push("exported_utxos.csv");

    let args = ExportUtxosArgs {
        output_file: Some(path_buf.clone()),
    };
    cli.command2 = Some(CliCommands::ExportUtxos(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();

    let seed_nodes = world.base_nodes.get(base_node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[when(expr = "I discover peer {word} on wallet {word} via command line")]
async fn discover_peer(world: &mut TariWorld, node: String, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut cli = get_default_cli();

    let mut node_client = world.get_node_client(&node).await.unwrap();
    let node_identity = node_client.identify(Empty {}).await.unwrap().into_inner();

    let args = DiscoverPeerArgs {
        dest_public_key: UniPublicKey::from_str(node_identity.public_key.to_hex().as_str()).unwrap(),
    };

    cli.command2 = Some(CliCommands::DiscoverPeer(args));

    let base_node = world.wallet_connected_to_base_node.get(&wallet).unwrap();
    let seed_nodes = world.base_nodes.get(&node).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet, Some(base_node.clone()), seed_nodes, None, Some(cli)).await;
}

#[then(expr = "I run whois {word} on wallet {word} via command line")]
async fn whois(world: &mut TariWorld, node: String, wallet: String) {
    let wallet_ps = world.wallets.get_mut(&wallet).unwrap();
    wallet_ps.kill();

    tokio::time::sleep(Duration::from_secs(5)).await;

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
