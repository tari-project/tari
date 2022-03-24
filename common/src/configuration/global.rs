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
//
//! # Global configuration of tari base layer system

use std::{
    convert::{TryFrom, TryInto},
    fmt,
    fmt::{Display, Formatter},
    net::SocketAddr,
    num::{NonZeroU16, TryFromIntError},
    path::PathBuf,
    prelude::rust_2021::FromIterator,
    str::FromStr,
    time::Duration,
};

use config::{Config, ConfigError, Environment};
use multiaddr::{Error, Multiaddr, Protocol};
use serde::{Deserialize, Serialize};
use tari_storage::lmdb_store::LMDBConfig;

use crate::{
    configuration::{bootstrap::ApplicationType, name_server::DnsNameServer, CollectiblesConfig, Network},
    ConfigurationError,
};

const DB_INIT_DEFAULT_MB: usize = 1000;
const DB_GROW_SIZE_DEFAULT_MB: usize = 500;
const DB_RESIZE_THRESHOLD_DEFAULT_MB: usize = 100;

const DB_INIT_MIN_MB: i64 = 100;
const DB_GROW_SIZE_MIN_MB: i64 = 20;
const DB_RESIZE_THRESHOLD_MIN_MB: i64 = 10;

//-------------------------------------        Main Configuration Struct      --------------------------------------//

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub inner: Config,
    pub network: Network,
    pub data_dir: PathBuf,
}

impl GlobalConfig {}

#[cfg(unix)]
fn libtor_enabled(cfg: &Config, net_str: &str) -> (bool, bool) {
    let key = config_string("base_node", net_str, "use_libtor");
    let base_node_use_libtor = cfg.get_bool(&key).unwrap_or(false);
    let key = config_string("wallet", net_str, "use_libtor");
    let console_wallet_use_libtor = cfg.get_bool(&key).unwrap_or(false);

    (base_node_use_libtor, console_wallet_use_libtor)
}

#[cfg(windows)]
fn libtor_enabled(_: &Config, _: &str) -> (bool, bool) {
    (false, false)
}

/// Changes ConfigError::NotFound into None
fn optional<T>(result: Result<T, ConfigError>) -> Result<Option<T>, ConfigError> {
    match result {
        Ok(v) => Ok(Some(v)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

fn one_of<T>(cfg: &Config, keys: &[&str]) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    for k in keys {
        if let Some(v) = optional(cfg.get_string(k))? {
            return v
                .parse()
                .map_err(|err| ConfigError::Message(format!("Failed to parse {}: {}", k, err)));
        }
    }
    Err(ConfigError::NotFound(format!(
        "None of the config keys [{}] were found",
        keys.join(", ")
    )))
}

// Clippy thinks "socks5" is not lowercase ...?
#[allow(clippy::match_str_case_mismatch)]
fn network_transport_config(
    cfg: &Config,
    mut application: ApplicationType,
    network: &str,
) -> Result<CommsTransport, ConfigurationError> {
    todo!()
    // const P2P_APPS: &[ApplicationType] = &[ApplicationType::BaseNode, ApplicationType::ConsoleWallet];
    // if !P2P_APPS.contains(&application) {
    //     // TODO: If/when we split the configs by app, this hack can be removed
    //     //       This removed the need to setup defaults for apps that dont use the network,
    //     //       assuming base node has been set up
    //     application = ApplicationType::BaseNode;
    // }
    //
    // let get_conf_str = |key| {
    //     cfg.get_string(key)
    //         .map_err(|err| ConfigurationError::new(key, None, &err.to_string()))
    // };
    //
    // let get_conf_multiaddr = |key| {
    //     let path_str = get_conf_str(key)?;
    //     path_str
    //         .parse::<Multiaddr>()
    //         .map_err(|err| ConfigurationError::new(key, Some(path_str), &err.to_string()))
    // };
    //
    // let app_str = application.as_config_str();
    // let transport_key = config_string(app_str, network, "transport");
    // let transport = get_conf_str(&transport_key)?;
    //
    // match transport.to_lowercase().as_str() {
    //     "tcp" => {
    //         let key = config_string(app_str, network, "tcp_listener_address");
    //         let listener_address = get_conf_multiaddr(&key)?;
    //         let key = config_string(app_str, network, "tcp_tor_socks_address");
    //         let tor_socks_address = get_conf_multiaddr(&key).ok();
    //         let key = config_string(app_str, network, "tcp_tor_socks_auth");
    //         let tor_socks_auth = get_conf_str(&key).ok().and_then(|auth_str| auth_str.parse().ok());
    //
    //         Ok(CommsTransport::Tcp {
    //             listener_address,
    //             tor_socks_auth,
    //             tor_socks_address,
    //         })
    //     },
    //     "tor" => {
    //         let key = config_string(app_str, network, "tor_control_address");
    //         let control_server_address = get_conf_multiaddr(&key)?;
    //
    //         let key = config_string(app_str, network, "tor_control_auth");
    //         let auth_str = get_conf_str(&key)?;
    //         let auth = auth_str
    //             .parse()
    //             .map_err(|err: String| ConfigurationError::new(&key, Some(auth_str), &err))?;
    //
    //         let key = config_string(app_str, network, "tor_forward_address");
    //         let forward_address = get_conf_multiaddr(&key)?;
    //         let key = config_string(app_str, network, "tor_onion_port");
    //         let onion_port = cfg
    //             .get::<NonZeroU16>(&key)
    //             .map_err(|err| ConfigurationError::new(&key, None, &err.to_string()))?;
    //
    //         // TODO
    //         let key = config_string(app_str, network, "tor_proxy_bypass_addresses");
    //         let tor_proxy_bypass_addresses = optional(cfg.get_array(&key))?
    //             .unwrap_or_default()
    //             .into_iter()
    //             .map(|v| {
    //                 v.into_string()
    //                     .map_err(|err| ConfigurationError::new(&key, None, &err.to_string()))
    //                     .and_then(|s| {
    //                         Multiaddr::from_str(&s)
    //                             .map_err(|err| ConfigurationError::new(&key, Some(s), &err.to_string()))
    //                     })
    //             })
    //             .collect::<Result<_, _>>()?;
    //
    //         let key = config_string(app_str, network, "tor_socks_address_override");
    //         let socks_address_override = match get_conf_str(&key).ok() {
    //             Some(addr) => Some(
    //                 addr.parse::<Multiaddr>()
    //                     .map_err(|err| ConfigurationError::new(&key, Some(addr), &err.to_string()))?,
    //             ),
    //             None => None,
    //         };
    //
    //         let key = config_string(app_str, network, "tor_proxy_bypass_for_outbound_tcp");
    //         let tor_proxy_bypass_for_outbound_tcp = optional(cfg.get_bool(&key))?.unwrap_or(false);
    //
    //         Ok(CommsTransport::TorHiddenService {
    //             control_server_address,
    //             auth,
    //             socks_address_override,
    //             forward_address,
    //             onion_port,
    //             tor_proxy_bypass_addresses,
    //             tor_proxy_bypass_for_outbound_tcp,
    //         })
    //     },
    //     "socks5" => {
    //         let key = config_string(app_str, network, "socks5_proxy_address");
    //         let proxy_address = get_conf_multiaddr(&key)?;
    //
    //         let key = config_string(app_str, network, "socks5_auth");
    //         let auth_str = get_conf_str(&key)?;
    //         let auth = auth_str
    //             .parse()
    //             .map_err(|err: String| ConfigurationError::new(&key, Some(auth_str), &err))?;
    //
    //         let key = config_string(app_str, network, "socks5_listener_address");
    //         let listener_address = get_conf_multiaddr(&key)?;
    //
    //         Ok(CommsTransport::Socks5 {
    //             proxy_address,
    //             listener_address,
    //             auth,
    //         })
    //     },
    //     t => Err(ConfigurationError::new(
    //         &transport_key,
    //         Some(t.to_string()),
    //         &format!("Invalid transport type '{}'", t),
    //     )),
    // }
}

/// Returns prefix.network.key as a String
fn config_string(prefix: &str, network: &str, key: &str) -> String {
    format!("{}.{}.{}", prefix, network, key)
}

//---------------------------------------------     Network Transport     ------------------------------------------//
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub enum TorControlAuthentication {
    None,
    Password(String),
}

fn parse_key_value(s: &str, split_chr: char) -> (String, Option<&str>) {
    let mut parts = s.splitn(2, split_chr);
    (
        parts
            .next()
            .expect("splitn always emits at least one part")
            .to_lowercase(),
        parts.next(),
    )
}

/// Interpret a string as either a socket address (first) or a multiaddr format string.
/// If the former, it gets converted into a MultiAddr before being returned.
pub fn socket_or_multi(addr: &str) -> Result<Multiaddr, Error> {
    addr.parse::<SocketAddr>()
        .map(|socket| match socket {
            SocketAddr::V4(ip4) => Multiaddr::from_iter([Protocol::Ip4(*ip4.ip()), Protocol::Tcp(ip4.port())]),
            SocketAddr::V6(ip6) => Multiaddr::from_iter([Protocol::Ip6(*ip6.ip()), Protocol::Tcp(ip6.port())]),
        })
        .or_else(|_| addr.parse::<Multiaddr>())
}

impl TryFrom<String> for TorControlAuthentication {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().parse()
    }
}

impl FromStr for TorControlAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(TorControlAuthentication::None),
            "password" => {
                let password = maybe_value.ok_or_else(|| {
                    "Invalid format for 'password' tor authentication type. It should be in the format \
                     'password=xxxxxx'."
                        .to_string()
                })?;
                Ok(TorControlAuthentication::Password(password.to_string()))
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

impl fmt::Debug for TorControlAuthentication {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use TorControlAuthentication::*;
        match self {
            None => write!(f, "None"),
            Password(_) => write!(f, "Password(...)"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SocksAuthentication {
    None,
    UsernamePassword { username: String, password: String },
}

impl FromStr for SocksAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(SocksAuthentication::None),
            "username_password" => {
                let (username, password) = maybe_value
                    .and_then(|value| {
                        let (un, pwd) = parse_key_value(value, ':');
                        // If pwd is None, return None
                        pwd.map(|p| (un, p))
                    })
                    .ok_or_else(|| {
                        "Invalid format for 'username-password' socks authentication type. It should be in the format \
                         'username_password=my_username:xxxxxx'."
                            .to_string()
                    })?;
                Ok(SocksAuthentication::UsernamePassword {
                    username,
                    password: password.to_string(),
                })
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields, rename_all = "snake_case")]
pub enum CommsTransport {
    None,
    /// Use TCP to join the Tari network. This transport can only communicate with TCP/IP addresses, so peers with
    /// e.g. tor onion addresses will not be contactable.
    Tcp {
        listener_address: Multiaddr,
        tor_socks_address: Option<Multiaddr>,
        tor_socks_auth: Option<SocksAuthentication>,
    },
    /// Configures the node to run over a tor hidden service using the Tor proxy. This transport recognises ip/tcp,
    /// onion v2, onion v3 and DNS addresses.
    #[serde(rename = "tor")]
    TorHiddenService {
        /// The address of the control server
        tor_control_address: Multiaddr,
        socks_address_override: Option<Multiaddr>,
        /// The address used to receive proxied traffic from the tor proxy to the Tari node. This port must be
        /// available
        tor_forward_address: Multiaddr,
        tor_control_auth: TorControlAuthentication,
        tor_onion_port: NonZeroU16,
        tor_proxy_bypass_addresses: Vec<Multiaddr>,
        tor_proxy_bypass_for_outbound_tcp: bool,
        tor_identity_file: Option<PathBuf>,
    },
    /// Use a SOCKS5 proxy transport. This transport recognises any addresses supported by the proxy.
    Socks5 {
        proxy_address: Multiaddr,
        auth: SocksAuthentication,
        listener_address: Multiaddr,
    },
}

impl Default for CommsTransport {
    fn default() -> Self {
        Self::None
    }
}
