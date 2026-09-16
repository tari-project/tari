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
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    #[clap(short, long, alias = "non-interactive", env = "TARI_NON_INTERACTIVE")]
    pub non_interactive_mode: bool,
}

impl ConfigOverrideProvider for Cli {
    /// Get the configuration property overrides for the given network. In case of duplicates, the final override
    /// added to the list will have preference.
    fn get_config_property_overrides(&self, network: &Network) -> Vec<(String, String)> {
        // Config file overrides
        let mut overrides = vec![("merge_mining_proxy.override_from".to_string(), network.to_string())];
        overrides.push(("merge_mining_proxy.network".to_string(), network.to_string()));
        // Command-line overrides
        let command_line_overrides = self.common.get_config_property_overrides(network);
        command_line_overrides.iter().for_each(|(k, v)| {
            replace_or_add_override(&mut overrides, k, v);
        });
        overrides
    }
}

fn replace_or_add_override(overrides: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(index) = overrides.iter().position(|(k, _)| k == key) {
        overrides.remove(index);
    }
    overrides.push((key.to_string(), value.to_string()));
}
