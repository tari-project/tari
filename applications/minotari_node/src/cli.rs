//  Copyright 2022. The Tari Project
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

use clap::Parser;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use tari_common::configuration::{ConfigOverrideProvider, Network};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    /// Create a default configuration file if it doesn't exist
    #[clap(long)]
    pub init: bool,
    /// This will rebuild the db, adding block for block in
    #[clap(long, alias = "rebuild_db")]
    pub rebuild_db: bool,
    /// Run in non-interactive mode, with no UI.
    #[clap(short, long, alias = "non-interactive", env = "TARI_NON_INTERACTIVE")]
    pub non_interactive_mode: bool,
    /// Watch a command in the non-interactive mode.
    #[clap(long)]
    pub watch: Option<String>,
    #[clap(long, alias = "profile")]
    pub profile_with_tokio_console: bool,
    #[clap(long, env = "MINOTARI_NODE_ENABLE_GRPC", alias = "enable-grpc")]
    pub grpc_enabled: bool,
    #[clap(long, env = "MINOTARI_NODE_ENABLE_MINING", alias = "enable-mining")]
    pub mining_enabled: bool,
    #[clap(long, env = "MINOTARI_NODE_SECOND_LAYER_GRPC_ENABLED", alias = "enable-second-layer")]
    pub second_layer_grpc_enabled: bool,
}

impl ConfigOverrideProvider for Cli {
    fn get_config_property_overrides(&self, network: &mut Network) -> Vec<(String, String)> {
        let mut overrides = self.common.get_config_property_overrides(network);
        *network = self.common.network.unwrap_or(*network);
        overrides.push(("base_node.network".to_string(), network.to_string()));
        overrides.push(("base_node.override_from".to_string(), network.to_string()));
        overrides.push(("p2p.seeds.override_from".to_string(), network.to_string()));
        overrides.push(("auto_update.override_from".to_string(), network.to_string()));
        overrides.push(("metrics.override_from".to_string(), network.to_string()));
        if self.grpc_enabled {
            overrides.push(("base_node.grpc_enabled".to_string(), "true".to_string()));
        }
        if self.mining_enabled {
            overrides.push(("base_node.grpc_enabled".to_string(), "true".to_string()));
            overrides.push(("base_node.mining_enabled".to_string(), "true".to_string()));
        }
        if self.second_layer_grpc_enabled {
            overrides.push(("base_node.grpc_enabled".to_string(), "true".to_string()));
            overrides.push(("base_node.second_layer_grpc_enabled".to_string(), "true".to_string()));
        }
        overrides
    }
}
