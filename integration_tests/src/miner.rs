//   Copyright 2022. The Tari Project
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

use std::{convert::TryFrom, time::Duration};

use minotari_app_grpc::tari_rpc::{
    pow_algo::PowAlgos,
    Block,
    NewBlockTemplate,
    NewBlockTemplateRequest,
    PowAlgo,
    TransactionOutput as GrpcTransactionOutput,
};
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use minotari_miner::{run_miner, Cli};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use minotari_wallet_grpc_client::{grpc, WalletGrpcClient};
use tari_common::{configuration::Network, network_check::set_network_if_choice_valid};
use tari_common_types::tari_address::TariAddress;
use tari_core::{
    consensus::ConsensusManager,
    transactions::{
        generate_coinbase_with_wallet_output,
        key_manager::{MemoryDbKeyManager, TariKeyId},
        tari_amount::MicroMinotari,
        transaction_components::{RangeProofType, WalletOutput},
    },
};
use tonic::transport::Channel;

use crate::TariWorld;

type BaseNodeClient = BaseNodeGrpcClient<Channel>;

#[derive(Clone, Debug)]
pub struct MinerProcess {
    pub name: String,
    pub base_node_name: String,
    pub wallet_name: String,
    pub mine_until_height: u64,
    pub stealth: bool,
}

pub fn register_miner_process(
    world: &mut TariWorld,
    miner_name: String,
    base_node_name: String,
    wallet_name: String,
    stealth: bool,
) {
    let miner = MinerProcess {
        name: miner_name.clone(),
        base_node_name,
        wallet_name,
        mine_until_height: 100_000,
        stealth,
    };

    world.miners.insert(miner_name, miner);
}

impl MinerProcess {
    pub async fn mine(
        &self,
        world: &TariWorld,
        blocks: Option<u64>,
        miner_min_diff: Option<u64>,
        miner_max_diff: Option<u64>,
    ) {
        std::env::set_var("TARI_NETWORK", "localnet");
        set_network_if_choice_valid(Network::LocalNet).unwrap();

        let mut wallet_client = create_wallet_client(world, self.wallet_name.clone())
            .await
            .expect("wallet grpc client");

        let wallet_public_key = &wallet_client
            .get_address(grpc::Empty {})
            .await
            .unwrap()
            .into_inner()
            .address;
        let wallet_payment_address = TariAddress::from_bytes(&wallet_public_key).unwrap();

        let node = world.get_node(&self.base_node_name).unwrap().grpc_port;
        let temp_dir = world
            .current_base_dir
            .as_ref()
            .expect("Base dir on world")
            .join("miners")
            .join(&self.name);
        let data_dir = temp_dir.as_path().join("data/miner");
        let data_dir_str = data_dir.clone().into_os_string().into_string().unwrap();
        let mut config_path = data_dir;
        config_path.push("config.toml");
        let cli = Cli {
            common: CommonCliArgs {
                base_path: data_dir_str,
                config: config_path.into_os_string().into_string().unwrap(),
                log_config: None,
                log_level: None,
                config_property_overrides: vec![
                    (
                        "miner.base_node_grpc_address".to_string(),
                        format!("/ip4/127.0.0.1/tcp/{}", node),
                    ),
                    ("miner.num_mining_threads".to_string(), "1".to_string()),
                    ("miner.mine_on_tip_only".to_string(), "false".to_string()),
                    (
                        "miner.wallet_payment_address".to_string(),
                        wallet_payment_address.to_hex(),
                    ),
                    ("miner.stealth_payment".to_string(), self.stealth.to_string()),
                ],
                network: Some(Network::LocalNet),
            },
            mine_until_height: None,
            miner_max_blocks: blocks,
            miner_min_diff,
            miner_max_diff,
            non_interactive_mode: true,
        };
        run_miner(cli).await.unwrap();
    }
}

pub async fn create_wallet_client(world: &TariWorld, wallet_name: String) -> anyhow::Result<WalletGrpcClient<Channel>> {
    let wallet_grpc_port = world.wallets.get(&wallet_name).unwrap().grpc_port;
    let wallet_addr = format!("http://127.0.0.1:{}", wallet_grpc_port);

    eprintln!("Wallet GRPC at {}", wallet_addr);

    Ok(WalletGrpcClient::connect(wallet_addr.as_str()).await?)
}

pub async fn mine_blocks_without_wallet(
    base_client: &mut BaseNodeClient,
    num_blocks: u64,
    weight: u64,
    key_manager: &MemoryDbKeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_manager: &ConsensusManager,
) {
    for _ in 0..num_blocks {
        mine_block_without_wallet(
            base_client,
            weight,
            key_manager,
            script_key_id,
            wallet_payment_address,
            stealth_payment,
            consensus_manager,
        )
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Give some time for the base node and wallet to sync the new blocks
    tokio::time::sleep(Duration::from_secs(5)).await;
}

pub async fn mine_block(
    base_client: &mut BaseNodeClient,
    key_manager: &MemoryDbKeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_manager: &ConsensusManager,
) {
    let (block_template, _wallet_output) = create_block_template_with_coinbase(
        base_client,
        0,
        key_manager,
        script_key_id,
        wallet_payment_address,
        stealth_payment,
        consensus_manager,
    )
    .await;

    // Ask the base node for a valid block using the template
    let block_result = base_client
        .get_new_block(block_template.clone())
        .await
        .unwrap()
        .into_inner();
    let block = block_result.block.unwrap();

    // We don't need to mine, as Localnet blocks have difficulty 1s
    let _sumbmit_res = base_client.submit_block(block).await.unwrap();
    println!(
        "Block successfully mined at height {:?}",
        block_template.header.unwrap().height
    );
}

async fn mine_block_without_wallet(
    base_client: &mut BaseNodeClient,
    weight: u64,
    key_manager: &MemoryDbKeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_manager: &ConsensusManager,
) {
    let (block_template, _wallet_output) = create_block_template_with_coinbase(
        base_client,
        weight,
        key_manager,
        script_key_id,
        wallet_payment_address,
        stealth_payment,
        consensus_manager,
    )
    .await;
    mine_block_without_wallet_with_template(base_client, block_template).await;
}

async fn mine_block_without_wallet_with_template(base_client: &mut BaseNodeClient, block_template: NewBlockTemplate) {
    // Ask the base node for a valid block using the template
    let block_result = base_client
        .get_new_block(block_template.clone())
        .await
        .unwrap()
        .into_inner();
    let block = block_result.block.unwrap();

    // We don't need to mine, as Localnet blocks have difficulty 1s
    let _submit_res = base_client.submit_block(block).await.unwrap();
    println!(
        "Block successfully mined at height {:?}",
        block_template.header.unwrap().height
    );
}

async fn create_block_template_with_coinbase(
    base_client: &mut BaseNodeClient,
    weight: u64,
    key_manager: &MemoryDbKeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_manager: &ConsensusManager,
) -> (NewBlockTemplate, WalletOutput) {
    // get the block template from the base node
    let template_req = NewBlockTemplateRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3x.into(),
        }),
        max_weight: weight,
    };

    let template_response = base_client
        .get_new_block_template(template_req)
        .await
        .unwrap()
        .into_inner();

    let mut block_template = template_response.new_block_template.clone().unwrap();

    let template = template_response.new_block_template.as_ref().unwrap();
    let miner_data = template_response.miner_data.as_ref().unwrap();
    let fee = miner_data.total_fees;
    let reward = miner_data.reward;
    let height = template.header.as_ref().unwrap().height;

    // add the coinbase outputs and kernels to the block template
    let (_, coinbase_output, coinbase_kernel, coinbase_wallet_output) = generate_coinbase_with_wallet_output(
        MicroMinotari::from(fee),
        MicroMinotari::from(reward),
        height,
        &[],
        key_manager,
        script_key_id,
        wallet_payment_address,
        stealth_payment,
        consensus_manager.consensus_constants(height),
        RangeProofType::BulletProofPlus,
    )
    .await
    .unwrap();
    let body = block_template.body.as_mut().unwrap();

    let grpc_output = GrpcTransactionOutput::try_from(coinbase_output).unwrap();
    body.outputs.push(grpc_output);
    body.kernels.push(coinbase_kernel.into());

    (block_template, coinbase_wallet_output)
}

pub async fn mine_block_with_coinbase_on_node(world: &mut TariWorld, base_node: String, coinbase_name: String) {
    let mut client = world
        .base_nodes
        .get(&base_node)
        .unwrap()
        .get_grpc_client()
        .await
        .unwrap();
    let script_key_id = &world.script_key_id().await;
    let (template, wallet_output) = create_block_template_with_coinbase(
        &mut client,
        0,
        &world.key_manager,
        script_key_id,
        &world.default_payment_address.clone(),
        false,
        &world.consensus_manager.clone(),
    )
    .await;
    world.utxos.insert(coinbase_name, wallet_output);
    mine_block_without_wallet_with_template(&mut client, template).await;
}

pub async fn mine_block_before_submit(
    client: &mut BaseNodeClient,
    key_manager: &MemoryDbKeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
    stealth_payment: bool,
    consensus_manager: &ConsensusManager,
) -> Block {
    let (template, _wallet_output) = create_block_template_with_coinbase(
        client,
        0,
        key_manager,
        script_key_id,
        wallet_payment_address,
        stealth_payment,
        consensus_manager,
    )
    .await;

    let new_block = client.get_new_block(template).await.unwrap().into_inner();

    new_block.block.unwrap()
}
