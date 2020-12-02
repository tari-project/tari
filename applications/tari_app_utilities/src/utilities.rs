// Copyright 2020. The Tari Project
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

use crate::identity_management::load_from_json;
use futures::future::Either;
use log::*;
use std::{fmt, fmt::Formatter, net::SocketAddr, path::Path};
use tari_common::{CommsTransport, GlobalConfig, SocksAuthentication, TorControlAuthentication};
use tari_comms::{
    multiaddr::{Multiaddr, Protocol},
    peer_manager::NodeId,
    socks,
    tor,
    tor::TorIdentity,
    transports::SocksConfig,
    types::CommsPublicKey,
};
use tari_core::tari_utilities::hex::Hex;
use tari_p2p::transport::{TorConfig, TransportType};
use tari_wallet::util::emoji::EmojiId;
use tokio::{runtime, runtime::Runtime};

pub const LOG_TARGET: &str = "tari::application";

/// Enum to show failure information
#[derive(Debug, Clone)]
pub enum ExitCodes {
    ConfigError(String),
    UnknownError,
    InterfaceError,
    WalletError(String),
    GrpcError(String),
    InputError(String),
    CommandError(String),
}

impl ExitCodes {
    pub fn as_i32(&self) -> i32 {
        match self {
            Self::ConfigError(_) => 101,
            Self::UnknownError => 102,
            Self::InterfaceError => 103,
            Self::WalletError(_) => 104,
            Self::GrpcError(_) => 105,
            Self::InputError(_) => 106,
            Self::CommandError(_) => 107,
        }
    }
}

impl From<tari_common::ConfigError> for ExitCodes {
    fn from(err: tari_common::ConfigError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        Self::ConfigError(err.to_string())
    }
}

impl fmt::Display for ExitCodes {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ExitCodes::ConfigError(e) => write!(f, "Config Error ({}): {}", self.as_i32(), e),
            ExitCodes::WalletError(e) => write!(f, "Wallet Error ({}): {}", self.as_i32(), e),
            ExitCodes::GrpcError(e) => write!(f, "GRPC Error ({}): {}", self.as_i32(), e),
            ExitCodes::InputError(e) => write!(f, "Input Error ({}): {}", self.as_i32(), e),
            ExitCodes::CommandError(e) => write!(f, "Command Error ({}): {}", self.as_i32(), e),
            _ => write!(f, "{}", self.as_i32()),
        }
    }
}

/// Creates a transport type for the console wallet using the provided configuration
/// ## Paramters
/// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
///
/// ##Returns
/// TransportType based on the configuration
pub fn setup_wallet_transport_type(config: &GlobalConfig) -> TransportType {
    debug!(
        target: LOG_TARGET,
        "Console wallet transport is set to '{:?}'", config.comms_transport
    );

    let add_to_port = |addr: Multiaddr, n| -> Multiaddr {
        addr.iter()
            .map(|p| match p {
                Protocol::Tcp(port) => Protocol::Tcp(port + n),
                p => p,
            })
            .collect()
    };

    match config.comms_transport.clone() {
        CommsTransport::Tcp {
            listener_address,
            tor_socks_address,
            tor_socks_auth,
        } => TransportType::Tcp {
            listener_address: add_to_port(listener_address, 1),
            tor_socks_config: tor_socks_address.map(|proxy_address| SocksConfig {
                proxy_address,
                authentication: tor_socks_auth.map(convert_socks_authentication).unwrap_or_default(),
            }),
        },
        CommsTransport::TorHiddenService {
            control_server_address,
            socks_address_override,
            auth,
            ..
        } => {
            // The wallet should always use an OS-assigned forwarding port and an onion port number of 18101
            // to ensure that different wallet implementations cannot be differentiated by their port.
            let port_mapping = (18101u16, "127.0.0.1:0".parse::<SocketAddr>().unwrap()).into();

            let tor_identity_path = Path::new(&config.console_wallet_tor_identity_file);
            let identity = if tor_identity_path.exists() {
                // If this fails, we can just use another address
                load_from_json::<_, TorIdentity>(&tor_identity_path).ok()
            } else {
                None
            };
            info!(
                target: LOG_TARGET,
                "Console wallet tor identity at path '{}' {:?}",
                tor_identity_path.to_string_lossy(),
                identity
                    .as_ref()
                    .map(|ident| format!("loaded for address '{}.onion'", ident.service_id))
                    .or_else(|| Some("not found".to_string()))
                    .unwrap()
            );

            TransportType::Tor(TorConfig {
                control_server_addr: control_server_address,
                control_server_auth: {
                    match auth {
                        TorControlAuthentication::None => tor::Authentication::None,
                        TorControlAuthentication::Password(password) => tor::Authentication::HashedPassword(password),
                    }
                },
                identity: identity.map(Box::new),
                port_mapping,
                // TODO: make configurable
                socks_address_override,
                socks_auth: socks::Authentication::None,
            })
        },
        CommsTransport::Socks5 {
            proxy_address,
            listener_address,
            auth,
        } => TransportType::Socks {
            socks_config: SocksConfig {
                proxy_address,
                authentication: convert_socks_authentication(auth),
            },
            listener_address: add_to_port(listener_address, 1),
        },
    }
}

/// Converts one socks authentication struct into another
/// ## Parameters
/// `auth` - Socks authentication of type SocksAuthentication
///
/// ## Returns
/// Socks authentication of type socks::Authentication
pub fn convert_socks_authentication(auth: SocksAuthentication) -> socks::Authentication {
    match auth {
        SocksAuthentication::None => socks::Authentication::None,
        SocksAuthentication::UsernamePassword(username, password) => {
            socks::Authentication::Password(username, password)
        },
    }
}

/// Sets up the tokio runtime based on the configuration
/// ## Parameters
/// `config` - The configuration  of the base node
///
/// ## Returns
/// A result containing the runtime on success, string indicating the error on failure
pub fn setup_runtime(config: &GlobalConfig) -> Result<Runtime, String> {
    info!(
        target: LOG_TARGET,
        "Configuring the node to run on up to {} core threads and {} mining threads.",
        config.max_threads.unwrap_or(512),
        config.num_mining_threads
    );

    let mut builder = runtime::Builder::new();

    if let Some(max_threads) = config.max_threads {
        // Ensure that there are always enough threads for mining.
        // e.g if the user sets max_threads = 2, mining_threads = 5 then 7 threads are available in total
        builder.max_threads(max_threads + config.num_mining_threads);
    }
    if let Some(core_threads) = config.core_threads {
        builder.core_threads(core_threads);
    }

    builder
        .threaded_scheduler()
        .enable_all()
        .build()
        .map_err(|e| format!("There was an error while building the node runtime. {}", e.to_string()))
}

/// Returns a CommsPublicKey from either a emoji id or a public key
pub fn parse_emoji_id_or_public_key(key: &str) -> Option<CommsPublicKey> {
    EmojiId::str_to_pubkey(&key.trim().replace('|', ""))
        .or_else(|_| CommsPublicKey::from_hex(key))
        .ok()
}

/// Returns a CommsPublicKey from either a emoji id, a public key or node id
pub fn parse_emoji_id_or_public_key_or_node_id(key: &str) -> Option<Either<CommsPublicKey, NodeId>> {
    parse_emoji_id_or_public_key(key)
        .map(Either::Left)
        .or_else(|| NodeId::from_hex(key).ok().map(Either::Right))
}

pub fn either_to_node_id(either: Either<CommsPublicKey, NodeId>) -> NodeId {
    match either {
        Either::Left(pk) => NodeId::from_public_key(&pk),
        Either::Right(n) => n,
    }
}
