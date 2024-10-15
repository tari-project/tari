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

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::{
        deserialize_dns_name_server_list,
        utils::{deserialize_from_str, serialize_string},
        DnsNameServerList,
        MultiaddrList,
        Network,
        StringList,
    },
    SubConfigPath,
};
use tari_network::{
    multiaddr::{multiaddr, Multiaddr},
    ReachabilityMode,
};

/// Peer seed configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerSeedsConfig {
    pub override_from: Option<String>,
    /// Custom specified peer seed nodes
    #[serde(default)]
    pub peer_seeds: StringList,
    /// DNS seeds hosts. The DNS TXT records are queried from these hosts and the resulting peers added to the comms
    /// peer list.
    #[serde(default)]
    pub dns_seeds: StringList,
    /// DNS name server to use for DNS seeds.
    #[serde(
        default,
        deserialize_with = "deserialize_dns_name_server_list",
        serialize_with = "serialize_string"
    )]
    pub dns_seed_name_servers: DnsNameServerList,
    /// All DNS seed records must pass DNSSEC validation
    #[serde(default)]
    pub dns_seeds_use_dnssec: bool,
}

impl Default for PeerSeedsConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            peer_seeds: StringList::default(),
            dns_seeds: vec![format!(
                "seeds.{}.tari.com",
                Network::get_current_or_user_setting_or_default().as_key_str()
            )]
            .into(),
            dns_seed_name_servers: DnsNameServerList::from_str(
                "system, 1.1.1.1:853/cloudflare-dns.com, 8.8.8.8:853/dns.google, 9.9.9.9:853/dns.quad9.net",
            )
            .expect("string is valid"),
            dns_seeds_use_dnssec: false,
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
    /// The multiaddrs to listen on.
    /// Default: ["/ip4/0.0.0.0/tcp/0"]
    pub listen_addresses: Vec<Multiaddr>,
    #[serde(
        default,
        deserialize_with = "deserialize_from_str",
        serialize_with = "serialize_string"
    )]
    pub reachability_mode: ReachabilityMode,
    /// The global maximum allowed RPC sessions.
    /// Default: 100
    pub rpc_max_simultaneous_sessions: usize,
    /// The maximum allowed RPC sessions per peer.
    /// Default: 10
    pub rpc_max_sessions_per_peer: usize,
    pub enable_mdns: bool,
    pub enable_relay: bool,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            public_addresses: MultiaddrList::default(),
            listen_addresses: vec![multiaddr!(Ip4([0, 0, 0, 0]), Tcp(0u16))],
            reachability_mode: Default::default(),
            rpc_max_simultaneous_sessions: 100,
            rpc_max_sessions_per_peer: 10,
            enable_mdns: true,
            enable_relay: false,
        }
    }
}

impl SubConfigPath for P2pConfig {
    fn main_key_prefix() -> &'static str {
        "p2p"
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use tari_common::DnsNameServer;

    use crate::PeerSeedsConfig;

    #[test]
    fn default_dns_seed_name_servers_test() {
        let dns_seed_name_servers = PeerSeedsConfig::default().dns_seed_name_servers;
        assert_eq!(dns_seed_name_servers.into_vec(), vec![
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
            DnsNameServer::from_str("8.8.8.8:853/dns.google").unwrap(),
            DnsNameServer::from_str("9.9.9.9:853/dns.quad9.net").unwrap()
        ]);
    }

    #[test]
    fn it_deserializes_from_toml() {
        // No empty fields, no omitted fields
        let config_str = r#"
            dns_seeds = ["seeds.esmeralda.tari.com"]
            peer_seeds = ["20605a28047938f851e3d0cd3f0ff771b2fb23036f0ab8eaa57947dccc834d15::/onion3/e4dsii6vc5f7frao23syonalgikd5kcd7fddrdjhab6bdo3cu47n3kyd:18141"]
            dns_seed_name_servers = ["1.1.1.1:853/cloudflare-dns.com"]
            dns_seeds_use_dnssec = false
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seeds.into_vec(), vec!["seeds.esmeralda.tari.com"]);
        assert_eq!(config.peer_seeds.into_vec(), vec![
            "20605a28047938f851e3d0cd3f0ff771b2fb23036f0ab8eaa57947dccc834d15::/onion3/\
             e4dsii6vc5f7frao23syonalgikd5kcd7fddrdjhab6bdo3cu47n3kyd:18141"
        ]);
        assert_eq!(
            config.dns_seed_name_servers.to_string(),
            "1.1.1.1:853/cloudflare-dns.com".to_string()
        );
        assert!(!config.dns_seeds_use_dnssec);

        // 'dns_seeds_name_server' parse error handled
        let config_str = r#"
            dns_seeds = ["seeds.esmeralda.tari.com"]
            peer_seeds = ["20605a28047938f851e3d0cd3f0ff771b2fb23036f0ab8eaa57947dccc834d15::/onion3/e4dsii6vc5f7frao23syonalgikd5kcd7fddrdjhab6bdo3cu47n3kyd:18141"]
            dns_seed_name_servers = "111"
            #dns_seeds_use_dnssec = false
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
            dns_seed_name_servers = ["system", "1.1.1.1:853/cloudflare-dns.com"]
            dns_seeds_use_dnssec = false
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seeds.into_vec(), Vec::<String>::new());
        assert_eq!(config.peer_seeds.into_vec(), Vec::<String>::new());
        assert_eq!(config.dns_seed_name_servers.into_vec(), vec![
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
        ]);
        assert!(!config.dns_seeds_use_dnssec);

        // Omitted config fields
        let config_str = r#"
            #dns_seeds = []
            #peer_seeds = []
            #dns_seed_name_servers = []
            #dns_seeds_use_dnssec = false
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seeds.into_vec(), Vec::<String>::new());
        assert_eq!(config.peer_seeds.into_vec(), Vec::<String>::new());
        assert_eq!(config.dns_seed_name_servers.into_vec(), vec![]);
        assert!(!config.dns_seeds_use_dnssec);

        // System
        let config_str = r#"
            #dns_seeds = []
            #peer_seeds = []
            dns_seed_name_servers = "system"
            #dns_seeds_use_dnssec = false
         "#;
        let config = toml::from_str::<PeerSeedsConfig>(config_str).unwrap();
        assert_eq!(config.dns_seed_name_servers.into_vec(), vec![DnsNameServer::from_str(
            "system"
        )
        .unwrap(),]);
    }
}
