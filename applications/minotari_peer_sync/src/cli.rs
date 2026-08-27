// Copyright 2026. The Tari Project
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

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use tari_common::configuration::{ConfigOverrideProvider, Network};
use tari_p2p::TransportType;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    #[clap(flatten)]
    pub common: CommonCliArgs,
    /// How many rounds to run. Round 1 is the base node's own peer sync from the seed peers; every round after that
    /// asks the peers that were successfully dialled for their peer lists, and dials only the peers that are new.
    #[clap(long, default_value_t = 5)]
    pub rounds: usize,
    /// How long to wait for the DHT seed strap (peer sync) to complete, in seconds
    #[clap(long, default_value_t = 180)]
    pub sync_timeout: u64,
    /// How long to wait after peer sync completes before reading the peer list, in seconds. Gives late-arriving
    /// peers from concurrent seed syncs a chance to land in the peer database.
    #[clap(long, default_value_t = 5)]
    pub settle_time: u64,
    /// Per-peer dial timeout in seconds
    #[clap(long, default_value_t = 30)]
    pub dial_timeout: u64,
    /// How many peers to dial concurrently
    #[clap(long, default_value_t = 10)]
    pub concurrency: usize,
    /// Only dial the first N new peers each round (default: dial all of them)
    #[clap(long)]
    pub max_peers: Option<usize>,
    /// Do not dial the seed peers themselves, only the peers that were downloaded from them
    #[clap(long)]
    pub skip_seeds: bool,
    /// Print a line for every peer that was dialled
    #[clap(long)]
    pub show_peers: bool,
    /// Use the base node's identity file (and tor identity) instead of a throw-away identity. Do NOT use this while
    /// the base node itself is running - two nodes with the same identity will interfere with each other.
    #[clap(long)]
    pub use_node_identity: bool,
    /// Where to keep the peer database for this run. Defaults to a `peer_sync` directory inside the configured
    /// datastore path, so that the base node's own peer database is left alone.
    #[clap(long)]
    pub peer_db_dir: Option<PathBuf>,
    /// Keep (and re-use) the peer database from a previous run instead of starting from an empty one. Note that the
    /// base node skips peer sync entirely when it already knows enough peers, and so does this tool.
    #[clap(long)]
    pub reuse_peer_db: bool,
    /// The port to listen on. Defaults to 0 (an OS-assigned port) so that a running base node's listener is not
    /// clashed with.
    #[clap(long, default_value_t = 0)]
    pub listener_port: u16,
    /// The user agent to advertise. Defaults to the same string the base node uses.
    #[clap(long)]
    pub user_agent: Option<String>,
    /// Start the bundled tor instance even when `base_node.use_libtor` is false in the config (unix builds with the
    /// `libtor` feature only)
    #[clap(long, conflicts_with = "no_libtor")]
    pub libtor: bool,
    /// Do not start the bundled tor instance, use an already-running tor instead (unix builds with the `libtor`
    /// feature only)
    #[clap(long)]
    pub no_libtor: bool,
    /// Path to the libtor data directory. Defaults to `<base path>/libtor/peer_sync`.
    #[clap(short = 'z', long)]
    pub libtor_data_dir: Option<PathBuf>,
    /// Override the configured transport. `tor`, `tor-tcp` and `tcp-tor` need a running tor with a control port;
    /// `tcp` needs no tor but cannot reach peers that only advertise onion addresses.
    #[clap(long, value_enum)]
    pub transport: Option<Transport>,
}

/// The transports that can be selected from the command line. Mirrors [`TransportType`], which has no `FromStr`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Transport {
    Tcp,
    Tor,
    TorTcp,
    TcpTor,
    Socks5,
}

impl From<Transport> for TransportType {
    fn from(value: Transport) -> Self {
        match value {
            Transport::Tcp => TransportType::Tcp,
            Transport::Tor => TransportType::Tor,
            Transport::TorTcp => TransportType::TorTcp,
            Transport::TcpTor => TransportType::TcpTor,
            Transport::Socks5 => TransportType::Socks5,
        }
    }
}

impl ConfigOverrideProvider for Cli {
    /// The same overrides the base node applies, so that the network sub-sections of config.toml are picked up in
    /// exactly the same way.
    fn get_config_property_overrides(&self, network: &Network) -> Vec<(String, String)> {
        let mut overrides = vec![
            ("base_node.network".to_string(), network.to_string()),
            ("base_node.override_from".to_string(), network.to_string()),
            ("p2p.seeds.override_from".to_string(), network.to_string()),
        ];
        overrides.extend(self.common.get_config_property_overrides(network));
        overrides
    }
}
