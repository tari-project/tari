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

use std::path::PathBuf;

use clap::Parser;
use tari_app_utilities::common_cli_args::CommonCliArgs;

const DEFAULT_NETWORK: &str = "dibbler";

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub(crate) struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    /// Enable tracing
    #[clap(long, aliases = &["tracing", "enable-tracing"])]
    pub tracing_enabled: bool,
    /// Supply the password for the console wallet. It's very bad security practice to provide the password on the
    /// command line, since it's visible using `ps ax` from anywhere on the system, so always use the env var where
    /// possible.
    #[clap(long)] // , env = "TARI_WALLET_PASSWORD")]
    pub password: Option<String>,
    /// Change the password for the console wallet
    #[clap(long, alias = "update-password")]
    pub change_password: bool,
    /// Force wallet recovery
    #[clap(long, alias = "recover")]
    pub recovery: bool,
    /// Supply the optional wallet seed words for recovery on the command line
    #[clap(long, alias = "seed_words")]
    pub seed_words: Option<String>,
    /// Supply the optional file name to save the wallet seed words into
    #[clap(long, aliases = &["seed_words_file_name", "seed-words-file"], parse(from_os_str))]
    pub seed_words_file_name: Option<PathBuf>,
    /// Run in non-interactive mode, with no UI.
    #[clap(short, long, alias = "non-interactive")]
    pub non_interactive_mode: bool,
    /// Path to input file of commands
    #[clap(short, long, aliases = &["input", "script"], parse(from_os_str))]
    pub input_file: Option<PathBuf>,
    /// Single input command
    #[clap(long)]
    pub command: Option<String>,
    /// Wallet notify script
    #[clap(long, alias = "notify")]
    pub wallet_notify: Option<PathBuf>,
    /// Automatically exit wallet command/script mode when done
    #[clap(long, alias = "auto-exit")]
    pub command_mode_auto_exit: bool,
    /// Supply a network (overrides existing configuration)
    #[clap(long, alias = "network", default_value = DEFAULT_NETWORK)]
    pub network: String,
}

impl Cli {
    pub fn config_property_overrides(&self) -> Vec<(String, String)> {
        let mut overrides = self.common.config_property_overrides();
        overrides.push(("wallet.override_from".to_string(), self.network.clone()));
        overrides.push(("p2p.seeds.override_from".to_string(), self.network.clone()));
        overrides
    }
}
