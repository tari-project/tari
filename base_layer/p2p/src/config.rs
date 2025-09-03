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

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::{serializers, MultiaddrList, Network, StringList},
    SubConfigPath,
};
use tari_common_sqlite::connection::DbConnectionUrl;
use tari_comms::multiaddr::Multiaddr;
use tari_comms_dht::DhtConfig;

use crate::transport::TransportConfig;

/// Peer seed configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerSeedsConfig {
    pub override_from: Option<String>,
    /// Custom specified peer seed nodes
    #[serde(default)]
    pub peer_seeds: StringList,
    /// Custom specified peer seed endpoints
    #[serde(default)]
    pub dns_seeds: StringList,
    #[serde(default, skip)]
    pub dns_seed_name_servers: Option<()>,
    #[serde(default, skip)]
    pub dns_seeds_use_dnssec: Option<()>,
}

impl Default for PeerSeedsConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            peer_seeds: StringList::default(),
            dns_seeds: vec![format!(
                "https://cdn-universe.tari.com/tari-project/tari/{}/seednodes.json",
                Network::get_current_or_user_setting_or_default().as_key_str()
            )]
            .into(),
            dns_seed_name_servers: None,
            dns_seeds_use_dnssec: None,
        }
    }
}

impl SubConfigPath for PeerSeedsConfig {
    fn main_key_prefix() -> &'static str {
        "p2p.seeds"
    }
}

/// Configuration for a comms node
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P2pConfig {
    /// Internal field used for configuration.
    pub override_from: Option<String>,
    /// The public address advertised to other peers by this node. If not set it will be set automatically depending on
    /// the transport type. The TCP transport is not able to determine the users public IP, so this will need to be
    /// manually set.
    pub public_addresses: MultiaddrList,
    /// Transport configuration
    pub transport: TransportConfig,
    /// Path to the SQLite data files.
    pub datastore_path: PathBuf,
    /// Name to use for the peer database
    pub peer_database_name: String,
    /// The maximum number of concurrent Inbound tasks allowed before back-pressure is applied to peers
    pub max_concurrent_inbound_tasks: usize,
    /// The maximum number of concurrent outbound tasks allowed before back-pressure is applied to outbound messaging
    /// queue
    pub max_concurrent_outbound_tasks: usize,
    /// Configuration for DHT
    pub dht: DhtConfig,
    /// Set to true to allow peers to provide test addresses (loopback, memory etc.). If set to false, memory
    /// addresses, loopback, local-link (i.e addresses used in local tests) will not be accepted from peers. This
    /// should always be false for non-test nodes.
    pub allow_test_addresses: bool,
    /// The maximum number of liveness sessions allowed for the connection listener.
    /// Liveness sessions can be used by third party tooling to determine node liveness.
    /// A value of 0 will disallow any liveness sessions.
    pub listener_liveness_max_sessions: usize,
    /// If Some, enables periodic socket-level liveness checks
    #[serde(with = "serializers::optional_seconds")]
    pub listener_self_liveness_check_interval: Option<Duration>,
    /// CIDR for addresses allowed to enter into liveness check mode on the listener.
    pub listener_liveness_allowlist_cidrs: StringList,
    /// The address to bind on using the TCP transport _in addition to_ the primary transport. This is typically useful
    /// for direct comms between a wallet and base node. If this is set to None, no listener will be bound.
    /// Default: None
    pub auxiliary_tcp_listener_address: Option<Multiaddr>,
    /// The global maximum allowed RPC sessions.
    /// Default: 100
    pub rpc_max_simultaneous_sessions: usize,
    /// The maximum allowed RPC sessions per peer.
    /// Default: 10
    pub rpc_max_sessions_per_peer: usize,
    /// If true, and the maximum per peer RPC sessions is reached, the RPC server will close an old session and replace
    /// it with a new session. If false, the RPC server will reject the new session and preserve the older session.
    /// (default value = true).
    pub cull_oldest_peer_rpc_connection_on_full: bool,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            public_addresses: MultiaddrList::default(),
            transport: Default::default(),
            datastore_path: PathBuf::from("peer_db"),
            peer_database_name: "peers".to_string(),
            max_concurrent_inbound_tasks: 100,
            max_concurrent_outbound_tasks: 100,
            dht: DhtConfig {
                database_url: DbConnectionUrl::file("dht.sqlite"),
                auto_join: true,
                ..Default::default()
            },
            allow_test_addresses: false,
            listener_liveness_max_sessions: 0,
            listener_self_liveness_check_interval: None,
            listener_liveness_allowlist_cidrs: StringList::default(),
            auxiliary_tcp_listener_address: None,
            rpc_max_simultaneous_sessions: 100,
            rpc_max_sessions_per_peer: 10,
            cull_oldest_peer_rpc_connection_on_full: true,
        }
    }
}

impl SubConfigPath for P2pConfig {
    fn main_key_prefix() -> &'static str {
        "p2p"
    }
}

impl P2pConfig {
    /// Sets relative paths to use a common base path
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.datastore_path.is_absolute() {
            self.datastore_path = base_path.as_ref().join(self.datastore_path.as_path());
        }
        self.dht.set_base_path(base_path)
    }
}

#[cfg(test)]
mod test {
    use crate::PeerSeedsConfig;

    #[test]
    fn it_deserializes_from_toml() {
        // No empty fields, no omitted fields
        let config_str = r#"
            dns_seeds = ["https://cdn-universe.tari.com/tari-project/tari/esmeralda/seednodes.json"]
            peer_seeds = ["20605a28047938f851e3d0cd3f0ff771b2fb23036f0ab8eaa57947dccc834d15::/onion3/e4dsii6vc5f7frao23syonalgikd5kcd7fddrdjhab6bdo3cu47n3kyd:18141"]
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seeds.into_vec(), vec!["seeds.esmeralda.tari.com"]);
        assert_eq!(config.peer_seeds.into_vec(), vec![
            "20605a28047938f851e3d0cd3f0ff771b2fb23036f0ab8eaa57947dccc834d15::/onion3/\
             e4dsii6vc5f7frao23syonalgikd5kcd7fddrdjhab6bdo3cu47n3kyd:18141"
        ]);

        // 'dns_seeds_name_server' parse error handled
        let config_str = r#"
            dns_seeds = ["seeds.esmeralda.tari.com"]
            #peer_seeds = ["20605a28047938f851e3d0cd3f0ff771b2fb23036f0ab8eaa57947dccc834d15::/onion3/e4dsii6vc5f7frao23syonalgikd5kcd7fddrdjhab6bdo3cu47n3kyd:18141"]
         "#;
        match toml::from_str::<PeerSeedsConfig>(config_str) {
            Ok(_) => panic!("Should fail"),
            Err(e) => assert_eq!(
                e.to_string(),
                "invalid socket address syntax for key `dns_seed_name_servers` at line 4 column 37"
            ),
        }

        // Empty config list fields
        let config_str = r#"
            dns_seeds = []
            peer_seeds = []
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seeds.into_vec(), Vec::<String>::new());
        assert_eq!(config.peer_seeds.into_vec(), Vec::<String>::new());

        // Omitted config fields
        let config_str = r#"
            #dns_seeds = []
            #peer_seeds = []
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seeds.into_vec(), Vec::<String>::new());
        assert_eq!(config.peer_seeds.into_vec(), Vec::<String>::new());
    }
}
