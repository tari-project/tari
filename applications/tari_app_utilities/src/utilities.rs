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

use futures::future::Either;
use log::*;
use std::sync::Arc;
use tari_common::{CommsTransport, GlobalConfig, SocksAuthentication, TorControlAuthentication};
use tari_common_types::types::BlockHash;
use tari_comms::{
    peer_manager::NodeId,
    socks,
    tor,
    tor::TorIdentity,
    transports::SocksConfig,
    types::CommsPublicKey,
    utils::multiaddr::multiaddr_to_socketaddr,
};
use tari_core::tari_utilities::hex::Hex;
use tari_p2p::transport::{TorConfig, TransportType};
use tokio::{runtime, runtime::Runtime};

use crate::identity_management::load_from_json;
use tari_common_types::emoji::EmojiId;
use tari_comms::transports::predicate::FalsePredicate;

pub const LOG_TARGET: &str = "tari::application";

/// Creates a transport type from the given configuration
///
/// ## Paramters
/// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
///
/// ##Returns
/// TransportType based on the configuration
pub fn create_transport_type(config: &GlobalConfig) -> TransportType {
    debug!(target: LOG_TARGET, "Transport is set to '{:?}'", config.comms_transport);

    match config.comms_transport.clone() {
        CommsTransport::Tcp {
            listener_address,
            tor_socks_address,
            tor_socks_auth,
        } => TransportType::Tcp {
            listener_address,
            tor_socks_config: tor_socks_address.map(|proxy_address| SocksConfig {
                proxy_address,
                authentication: tor_socks_auth.map(convert_socks_authentication).unwrap_or_default(),
                proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
            }),
        },
        CommsTransport::TorHiddenService {
            control_server_address,
            socks_address_override,
            forward_address,
            auth,
            onion_port,
            tor_proxy_bypass_addresses,
            tor_proxy_bypass_for_outbound_tcp,
        } => {
            let identity = Some(&config.base_node_tor_identity_file)
                .filter(|p| p.exists())
                .and_then(|p| {
                    // If this fails, we can just use another address
                    load_from_json::<_, TorIdentity>(p).ok()
                });
            info!(
                target: LOG_TARGET,
                "Tor identity at path '{}' {:?}",
                config.base_node_tor_identity_file.to_string_lossy(),
                identity
                    .as_ref()
                    .map(|ident| format!("loaded for address '{}.onion'", ident.service_id))
                    .or_else(|| Some("not found".to_string()))
                    .unwrap()
            );

            let forward_addr = multiaddr_to_socketaddr(&forward_address).expect("Invalid tor forward address");
            TransportType::Tor(TorConfig {
                control_server_addr: control_server_address,
                control_server_auth: {
                    match auth {
                        TorControlAuthentication::None => tor::Authentication::None,
                        TorControlAuthentication::Password(password) => tor::Authentication::HashedPassword(password),
                    }
                },
                identity: identity.map(Box::new),
                port_mapping: (onion_port, forward_addr).into(),
                socks_address_override,
                socks_auth: socks::Authentication::None,
                tor_proxy_bypass_addresses,
                tor_proxy_bypass_for_outbound_tcp,
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
                proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
            },
            listener_address,
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
    let mut builder = runtime::Builder::new_multi_thread();

    if let Some(core_threads) = config.core_threads {
        info!(
            target: LOG_TARGET,
            "Configuring the node to run on up to {} core threads.",
            config
                .core_threads
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "<num cores>".to_string()),
        );
        builder.worker_threads(core_threads);
    }

    builder
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

/// Returns a hash from a hex string
pub fn parse_hash(hash_string: &str) -> Option<BlockHash> {
    BlockHash::from_hex(hash_string).ok()
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
