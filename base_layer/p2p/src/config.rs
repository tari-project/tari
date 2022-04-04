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
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::utils::{deserialize_string_or_struct, serialize_string},
    CommsTransport,
    DnsNameServer,
    SubConfigPath,
};
use tari_comms::multiaddr::Multiaddr;
use tari_comms_dht::DhtConfig;

/// Peer seed configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerSeedsConfig {
    override_from: Option<String>,
    /// Unparsed peer seeds
    pub peer_seeds: Vec<String>,
    /// DNS seeds hosts. The DNS TXT records are queried from these hosts and the resulting peers added to the comms
    /// peer list.
    pub dns_seeds: Vec<String>,
    #[serde(
        deserialize_with = "deserialize_string_or_struct",
        serialize_with = "serialize_string"
    )]
    /// DNS name server to use for DNS seeds.
    pub dns_seeds_name_server: DnsNameServer,
    /// All DNS seed records must pass DNSSEC validation
    pub dns_seeds_use_dnssec: bool,
}

impl Default for PeerSeedsConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            peer_seeds: vec![],
            dns_seeds: vec![],
            dns_seeds_name_server: DnsNameServer::from_str("1.1.1.1:53/cloudflare.net").unwrap(),
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
    override_from: Option<String>,
    /// The public address adverised to other peers by this node. If not set it will be set automatically depending on
    /// the transport type. The TCP transport is not able to determine the users public IP, so this will need to be
    /// manually set.
    pub public_address: Option<Multiaddr>,
    /// The type of transport to use
    pub transport: CommsTransport,
    /// Path to the LMDB data files.
    pub datastore_path: PathBuf,
    /// Name to use for the peer database
    pub peer_database_name: String,
    /// The maximum number of concurrent Inbound tasks allowed before back-pressure is applied to peers
    pub max_concurrent_inbound_tasks: usize,
    /// The maximum number of concurrent outbound tasks allowed before back-pressure is applied to outbound messaging
    /// queue
    pub max_concurrent_outbound_tasks: usize,
    /// The size of the buffer (channel) which holds pending outbound message requests
    pub outbound_buffer_size: usize,
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
    /// CIDR for addresses allowed to enter into liveness check mode on the listener.
    pub listener_liveness_allowlist_cidrs: Vec<String>,
    /// User agent string for this node
    pub user_agent: String,
    /// The address to bind on using the TCP transport _in addition to_ the primary transport. This is typically useful
    /// for direct comms between a wallet and base node. If this is set to None, no listener will be bound.
    /// Default: None
    pub auxiliary_tcp_listener_address: Option<Multiaddr>,
    /// The global maximum allowed RPC sessions.
    /// Default: 100
    pub rpc_max_simultaneous_sessions: usize,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            public_address: None,
            transport: Default::default(),
            datastore_path: PathBuf::from("peer_db"),
            peer_database_name: "peers".to_string(),
            max_concurrent_inbound_tasks: 50,
            max_concurrent_outbound_tasks: 100,
            outbound_buffer_size: 100,
            dht: Default::default(),
            allow_test_addresses: false,
            listener_liveness_max_sessions: 0,
            listener_liveness_allowlist_cidrs: vec![],
            user_agent: "".to_string(),
            auxiliary_tcp_listener_address: None,
            rpc_max_simultaneous_sessions: 100,
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
