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

use std::fmt::Debug;

use clap::Parser;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use tari_common::configuration::{ConfigOverrideProvider, Network};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    #[clap(long, alias = "monero-base-node-address")]
    pub monero_base_node_address: Option<String>,
    #[clap(long, alias = "monero-wallet-address")]
    pub monero_wallet_address: Option<String>,
    #[clap(long, alias = "mine-until-height")]
    pub mine_until_height: Option<u64>,
    #[clap(long, alias = "max-blocks")]
    pub miner_max_blocks: Option<u64>,
    #[clap(long, alias = "min-difficulty")]
    pub miner_min_diff: Option<u64>,
    #[clap(long, alias = "max-difficulty")]
    pub miner_max_diff: Option<u64>,
    #[clap(short, long, alias = "non-interactive", env = "TARI_NON_INTERACTIVE")]
    pub non_interactive_mode: bool,
}

impl ConfigOverrideProvider for Cli {
    fn get_config_property_overrides(&self, network: &mut Network) -> Vec<(String, String)> {
        let mut overrides = self.common.get_config_property_overrides(network);
        *network = self.common.network.unwrap_or(*network);
        overrides.push(("randomx_miner.network".to_string(), network.to_string()));
        overrides
    }
}
