// Copyright 2019. The Tari Project
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
use clap::{value_t, App, Arg};
use log::*;
use serde::{Deserialize, Serialize};

const LOG_TARGET: &str = "applications::testnet_miner";

pub mod testnet_miner {
    tonic::include_proto!("testnet_miner_rpc");
}

use testnet_miner::{client::TestNetMinerClient, Block, VoidParams};

#[derive(Debug, Default, Deserialize)]
struct Settings {
    wallet_address: Option<String>,
    base_node_address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = env_logger::init();
    let matches = App::new("Tari test-net miner")
        .version("0.1")
        .arg(
            Arg::with_name("config")
                .value_name("FILE")
                .long("config")
                .short("c")
                .help("The relative path of the miner config.toml file")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("wallet_address")
                .long("wallet_address")
                .short("w")
                .help("The address the wallet should use to connect to")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("base_node_address")
                .long("base_node_address")
                .short("b")
                .help("This is the address the server should use to connect to the base_node for blocks")
                .takes_value(true)
                .required(false),
        )
        .get_matches();

    let mut settings = Settings::default();

    if matches.is_present("config") {
        let mut settings_file = config::Config::default();
        settings_file
            .merge(config::File::with_name(matches.value_of("config").unwrap()))
            .expect("Could not open specified config file");
        settings = settings_file.try_into().unwrap();
    }
    if let Some(_c) = matches.values_of("wallet_address") {
        if let Ok(v) = value_t!(matches, "wallet_address", String) {
            settings.wallet_address = Some(v)
        }
    }
    if let Some(_c) = matches.values_of("base_node_address") {
        if let Ok(v) = value_t!(matches, "base_node_address", String) {
            settings.base_node_address = Some(v);
        }
    }

    if settings.wallet_address.is_none() || settings.base_node_address.is_none() {
        error!(
            target: LOG_TARGET,
            "Not all data has not been provided via command line or config file"
        );
        std::process::exit(1);
    };

    info!(target: LOG_TARGET, "Settings loaded");

    // ToDo run logic

    let mut base_node = TestNetMinerClient::connect(settings.base_node_address.unwrap())?;
    let request = tonic::Request::new(VoidParams {});

    let response = base_node.get_block(request).await?;

    // ToDo miner logic

    // ToDo get private key logic

    // ToDo send mined block to base node
    Ok(())
}
