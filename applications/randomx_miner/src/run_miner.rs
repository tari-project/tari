//  Copyright 2024. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{sync::Arc, time::Duration};

use log::info;
use reqwest::Client;
use tari_common::{load_configuration, DefaultConfigLoader};
use tokio::{sync::Mutex, time::sleep};

use crate::{
    cli::Cli,
    config::RandomXMinerConfig,
    error::{ConfigError, Error},
    json_rpc::{get_block_count, get_block_template},
};

pub const LOG_TARGET: &str = "minotari::randomx_miner::main";

pub async fn start_miner(cli: Cli) -> Result<(), Error> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path.as_path(), true, cli.non_interactive_mode, &cli)?;
    let mut config = RandomXMinerConfig::load_from(&cfg).expect("Failed to load config");
    config.set_base_path(cli.common.get_base_path());

    let node_address = config.monero_base_node_address.ok_or(ConfigError::MissingBaseNode)?;
    info!(target: LOG_TARGET, "Using Monero node address: {}", node_address);

    let monero_wallet_address = config
        .monero_wallet_address
        .ok_or(ConfigError::MissingMoneroWalletAddress)?;
    info!(target: LOG_TARGET, "Mining to Monero wallet address: {}", &monero_wallet_address);

    let client = Client::new();

    let mut tip = Arc::new(Mutex::new(0u64));
    let mut blocks_found: u64 = 0;
    loop {
        info!(target: LOG_TARGET, "Starting new mining cycle");

        get_block_count(&client, &node_address, tip.clone()).await?;
        let block_template = get_block_template(&client, &node_address, &monero_wallet_address).await?;

        sleep(Duration::from_secs(15)).await
    }
}
