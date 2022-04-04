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
use tari_app_utilities::common_cli_args::CommonCliArgs;

const DEFAULT_NETWORK: &str = "dibbler";

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub(crate) struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    /// Create and save new node identity if one doesn't exist
    #[clap(long, alias = "create_id")]
    pub create_id: bool,
    /// Create a default configuration file if it doesn't exist
    #[clap(long)]
    pub init: bool,
    /// Enable tracing
    #[clap(long, aliases = &["tracing", "enable-tracing"])]
    pub tracing_enabled: bool,
    /// This will rebuild the db, adding block for block in
    // TODO: Should be a command rather
    #[clap(long, alias = "rebuild_db")]
    pub rebuild_db: bool,
    /// Run in non-interactive mode, with no UI.
    #[clap(short, long, alias = "non-interactive")]
    pub non_interactive_mode: bool,
    /// Watch a command in the non-interactive mode.
    #[clap(long)]
    pub watch: Option<String>,
    /// Supply a network (overrides existing configuration)
    #[clap(long, alias = "network", default_value = DEFAULT_NETWORK)]
    pub network: String,
}

impl Cli {
    pub fn config_property_overrides(&self) -> Vec<(String, String)> {
        let mut overrides = self.common.config_property_overrides();
        overrides.push(("base_node.override_from".to_string(), self.network.clone()));
        overrides.push(("p2p.seeds.override_from".to_string(), self.network.clone()));
        overrides.push(("auto_update.override_from".to_string(), self.network.clone()));
        overrides
    }
}
