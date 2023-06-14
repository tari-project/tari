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

use std::{convert::TryInto, str::FromStr, time::Duration};

use tari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::{
        pow_algo::PowAlgos,
        wallet_client::WalletClient,
        Block,
        GetCoinbaseRequest,
        GetCoinbaseResponse,
        NewBlockTemplate,
        NewBlockTemplateRequest,
        NewBlockTemplateResponse,
        PowAlgo,
        TransactionKernel,
        TransactionOutput,
    },
};
use tari_app_utilities::common_cli_args::CommonCliArgs;
use tari_base_node_grpc_client::BaseNodeGrpcClient;
use tari_common::configuration::Network;
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_core::{
    consensus::ConsensusManager,
    test_helpers::TestKeyManager,
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        transaction_components::WalletOutput,
        CoinbaseBuilder,
    },
};
use tari_miner::{run_miner, Cli};
use tonic::{
    codegen::InterceptedService,
    transport::{Channel, Endpoint},
};

use crate::TariWorld;

type BaseNodeClient = BaseNodeGrpcClient<Channel>;
type WalletGrpcClient = WalletClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;

#[derive(Clone, Debug)]
pub struct MinerProcess {
    pub name: String,
    pub base_node_name: String,
    pub wallet_name: String,
    pub mine_until_height: u64,
}

pub fn register_miner_process(world: &mut TariWorld, miner_name: String, base_node_name: String, wallet_name: String) {
    let miner = MinerProcess {
        name: miner_name.clone(),
        base_node_name,
        wallet_name,
        mine_until_height: 100_000,
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
        let node = world.get_node(&self.base_node_name).unwrap().grpc_port;
        let wallet = world.get_wallet(&self.wallet_name).unwrap().grpc_port;
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
                    (
                        "miner.wallet_grpc_address".to_string(),
                        format!("/ip4/127.0.0.1/tcp/{}", wallet),
                    ),
                    ("miner.num_mining_threads".to_string(), "1".to_string()),
                    ("miner.mine_on_tip_only".to_string(), "false".to_string()),
                ],
                network: Some(Network::LocalNet),
            },
            mine_until_height: None,
            miner_max_blocks: blocks,
            miner_min_diff,
            miner_max_diff,
        };
        run_miner(cli).await.unwrap();
    }
}

#[allow(dead_code)]
pub async fn mine_blocks(world: &mut TariWorld, miner_name: String, num_blocks: u64) {
    let mut base_client = create_base_node_client(world, &miner_name).await;
    let mut wallet_client = create_wallet_client(world, &miner_name).await;

    for _ in 0..num_blocks {
        mine_block(&mut base_client, &mut wallet_client).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Give some time for the base node and wallet to sync the new blocks
    tokio::time::sleep(Duration::from_secs(5)).await;
}

pub async fn mine_blocks_without_wallet(
    base_client: &mut BaseNodeClient,
    num_blocks: u64,
    weight: u64,
    key_manager: &TestKeyManager,
) {
    for _ in 0..num_blocks {
        mine_block_without_wallet(base_client, weight, key_manager).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Give some time for the base node and wallet to sync the new blocks
    tokio::time::sleep(Duration::from_secs(5)).await;
}

async fn create_base_node_client(world: &TariWorld, miner_name: &String) -> BaseNodeClient {
    let miner = world.miners.get(miner_name).unwrap();
    let base_node_grpc_port = world.base_nodes.get(&miner.base_node_name).unwrap().grpc_port;
    let base_node_grpc_url = format!("http://127.0.0.1:{}", base_node_grpc_port);
    eprintln!("Base node GRPC at {}", base_node_grpc_url);
    BaseNodeClient::connect(base_node_grpc_url).await.unwrap()
}

async fn create_wallet_client(world: &TariWorld, miner_name: &String) -> WalletGrpcClient {
    let miner = world.miners.get(miner_name).unwrap();
    let wallet_grpc_port = world.wallets.get(&miner.wallet_name).unwrap().grpc_port;
    let wallet_addr = format!("http://127.0.0.1:{}", wallet_grpc_port);
    eprintln!("Wallet GRPC at {}", wallet_addr);
    let channel = Endpoint::from_str(&wallet_addr).unwrap().connect().await.unwrap();
    WalletClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&GrpcAuthentication::default()).unwrap(),
    )
}

pub async fn mine_block(base_client: &mut BaseNodeClient, wallet_client: &mut WalletGrpcClient) {
    let block_template = create_block_template_with_coinbase(base_client, wallet_client).await;

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

async fn mine_block_without_wallet(base_client: &mut BaseNodeClient, weight: u64, key_manager: &TestKeyManager) {
    let (block_template, _unblinded_output) =
        create_block_template_with_coinbase_without_wallet(base_client, weight, key_manager).await;
    mine_block_without_wallet_with_template(base_client, block_template.new_block_template.unwrap()).await;
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
    wallet_client: &mut WalletGrpcClient,
) -> NewBlockTemplate {
    // get the block template from the base node
    let template_req = NewBlockTemplateRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3.into(),
        }),
        max_weight: 0,
    };

    let template_res = base_client
        .get_new_block_template(template_req)
        .await
        .unwrap()
        .into_inner();

    let mut block_template = template_res.new_block_template.clone().unwrap();

    // add the coinbase outputs and kernels to the block template
    let (output, kernel) = get_coinbase_outputs_and_kernels(wallet_client, template_res).await;
    let body = block_template.body.as_mut().unwrap();

    body.outputs.push(output);
    body.kernels.push(kernel);

    block_template
}

async fn create_block_template_with_coinbase_without_wallet(
    base_client: &mut BaseNodeClient,
    weight: u64,
    key_manager: &TestKeyManager,
) -> (NewBlockTemplateResponse, WalletOutput) {
    // get the block template from the base node
    let template_req = NewBlockTemplateRequest {
        algo: Some(PowAlgo {
            pow_algo: PowAlgos::Sha3.into(),
        }),
        max_weight: weight,
    };

    let mut template_res = base_client
        .get_new_block_template(template_req)
        .await
        .unwrap()
        .into_inner();

    // let mut block_template = template_res.new_block_template.clone().unwrap();

    // add the coinbase outputs and kernels to the block template
    let (output, kernel, unblinded_output) =
        get_coinbase_without_wallet_client(template_res.clone(), key_manager).await;
    // let body = block_template.body.as_mut().unwrap();

    template_res
        .new_block_template
        .as_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap()
        .outputs
        .push(output);
    template_res
        .new_block_template
        .as_mut()
        .unwrap()
        .body
        .as_mut()
        .unwrap()
        .kernels
        .push(kernel);

    (template_res, unblinded_output)
}

async fn get_coinbase_outputs_and_kernels(
    wallet_client: &mut WalletGrpcClient,
    template_res: NewBlockTemplateResponse,
) -> (TransactionOutput, TransactionKernel) {
    let coinbase_req = coinbase_request(&template_res);
    let coinbase_res = wallet_client.get_coinbase(coinbase_req).await.unwrap().into_inner();
    extract_outputs_and_kernels(coinbase_res)
}

async fn get_coinbase_without_wallet_client(
    template_res: NewBlockTemplateResponse,
    key_manager: &TestKeyManager,
) -> (TransactionOutput, TransactionKernel, WalletOutput) {
    let coinbase_req = coinbase_request(&template_res);
    generate_coinbase(coinbase_req, key_manager).await
}

async fn generate_coinbase(
    coinbase_req: GetCoinbaseRequest,
    key_manager: &TestKeyManager,
) -> (TransactionOutput, TransactionKernel, WalletOutput) {
    let reward = coinbase_req.reward;
    let height = coinbase_req.height;
    let fee = coinbase_req.fee;
    let extra = coinbase_req.extra;

    let (spending_key_id, _, script_private_key_id, _) = key_manager.get_next_spend_and_script_key_ids().await.unwrap();

    let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
    let consensus_constants = consensus_manager.consensus_constants(height);

    let (tx, ubutxo) = CoinbaseBuilder::new(key_manager.clone())
        .with_block_height(height)
        .with_fees(fee.into())
        .with_spend_key_id(spending_key_id)
        .with_script_key_id(script_private_key_id)
        .with_extra(extra)
        .build_with_reward(consensus_constants, reward.into())
        .await
        .unwrap();

    let tx_out = tx.body().outputs().first().unwrap().clone();
    let tx_krnl = tx.body().kernels().first().unwrap().clone();

    (tx_out.try_into().unwrap(), tx_krnl.into(), ubutxo)
}

fn coinbase_request(template_response: &NewBlockTemplateResponse) -> GetCoinbaseRequest {
    let template = template_response.new_block_template.as_ref().unwrap();
    let miner_data = template_response.miner_data.as_ref().unwrap();
    let fee = miner_data.total_fees;
    let reward = miner_data.reward;
    let height = template.header.as_ref().unwrap().height;
    GetCoinbaseRequest {
        reward,
        fee,
        height,
        extra: vec![],
    }
}

fn extract_outputs_and_kernels(coinbase: GetCoinbaseResponse) -> (TransactionOutput, TransactionKernel) {
    let transaction_body = coinbase.transaction.unwrap().body.unwrap();
    let output = transaction_body.outputs.get(0).cloned().unwrap();
    let kernel = transaction_body.kernels.get(0).cloned().unwrap();
    (output, kernel)
}

pub async fn mine_block_with_coinbase_on_node(world: &mut TariWorld, base_node: String, coinbase_name: String) {
    let mut client = world
        .base_nodes
        .get(&base_node)
        .unwrap()
        .get_grpc_client()
        .await
        .unwrap();
    let (template, unblinded_output) =
        create_block_template_with_coinbase_without_wallet(&mut client, 0, &world.key_manager).await;
    world.utxos.insert(coinbase_name, unblinded_output);
    mine_block_without_wallet_with_template(&mut client, template.new_block_template.unwrap()).await;
}

pub async fn mine_block_before_submit(client: &mut BaseNodeClient, key_manager: &TestKeyManager) -> Block {
    let (template, _unblinded_output) =
        create_block_template_with_coinbase_without_wallet(client, 0, key_manager).await;

    let new_block = client
        .get_new_block(template.new_block_template.unwrap())
        .await
        .unwrap()
        .into_inner();

    new_block.block.unwrap()
}
