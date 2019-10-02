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
#[macro_use]
extern crate clap;

use pnet::datalink::{self, NetworkInterface};

use clap::{App, Arg};
use log::*;
use serde::{Deserialize, Serialize};
use std::{fs, sync::Arc, time::Duration};
use tari_comms::{
    connection::NetAddress,
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::keys::PublicKey;
use tari_grpc_wallet::wallet_server::WalletServer;
use tari_p2p::initialization::CommsConfig;
use tari_utilities::{hex::Hex, message_format::MessageFormat};
use tari_wallet::{text_message_service_sync::Contact, wallet::WalletConfig, Wallet};

const LOG_TARGET: &str = "applications::grpc_wallet";

#[derive(Debug, Default, Deserialize)]
struct Settings {
    control_port: Option<u32>,
    grpc_port: Option<u32>,
    secret_key: Option<String>,
    data_path: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct ConfigPeer {
    screen_name: String,
    pub_key: String,
    address: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Peers {
    peers: Vec<ConfigPeer>,
}

/// Entry point into the gRPC server binary
pub fn main() {
    let _ = env_logger::init();
    let matches = App::new("Tari Wallet gRPC server")
        .version("0.1")
        .arg(
            Arg::with_name("node-num")
                .long("node_num")
                .short("N")
                .help(
                    "An integer indicating which Node number config to load from the Tari repo root (Node config is a \
                     pair of files consisting of config + peers for that node)",
                )
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("config")
                .value_name("FILE")
                .long("config")
                .short("c")
                .help("The relative path of a wallet config.toml file")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("grpc-port")
                .long("grpc")
                .short("g")
                .help("The port the gRPC server will listen on")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("control-port")
                .long("control-port")
                .short("p")
                .help("The port the p2p stack will listen on")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("secret-key")
                .long("secret")
                .short("s")
                .help("This nodes communication secret key")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("data-path")
                .long("data-path")
                .short("d")
                .help("Path where this node's database files will be stored")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("peers")
                .value_name("FILE")
                .long("peers")
                .takes_value(true)
                .required(false),
        )
        .get_matches();

    let mut settings = Settings::default();
    let mut contacts = Peers { peers: Vec::new() };
    let mut database_path = "./data/text_message_service.sqlite3".to_string();
    // The node-num switch overrides the config and peers switch for quick testing from the tari repo root
    if matches.is_present("node-num") {
        let node_num = value_t!(matches, "node-num", u32).unwrap();
        let peer_path = format!("./applications/grpc_wallet/sample_config/node{}_peers.json", node_num);
        let config_path = format!(
            "./applications/grpc_wallet/sample_config/wallet_config_node{}.toml",
            node_num
        );
        let mut settings_file = config::Config::default();
        settings_file
            .merge(config::File::with_name(config_path.as_str()))
            .expect("Could not open specified config file");
        settings = settings_file.try_into().unwrap();
        let contents = fs::read_to_string(peer_path).expect("Could not open specified Peers json file");
        contacts = Peers::from_json(contents.as_str()).expect("Could not parse JSON from specified Peers json file");
        database_path = format!("./data/text_message_service_node{}.sqlite3", node_num).to_string();
    } else {
        if matches.is_present("config") {
            let mut settings_file = config::Config::default();
            settings_file
                .merge(config::File::with_name(matches.value_of("config").unwrap()))
                .expect("Could not open specified config file");
            settings = settings_file.try_into().unwrap();
        }
        if let Some(f) = matches.value_of("peers") {
            let contents = fs::read_to_string(f).expect("Could not open specified Peers json file");
            contacts =
                Peers::from_json(contents.as_str()).expect("Could not parse JSON from specified Peers json file");
        }
    }
    if let Some(_c) = matches.values_of("control-port") {
        if let Ok(v) = value_t!(matches, "control-port", u32) {
            settings.control_port = Some(v)
        }
    }
    if let Some(_c) = matches.values_of("grpc-port") {
        if let Ok(v) = value_t!(matches, "grpc-port", u32) {
            settings.grpc_port = Some(v);
        }
    }
    if let Some(c) = matches.value_of("secret-key") {
        settings.secret_key = Some(c.to_string())
    }
    if let Some(p) = matches.value_of("data-path") {
        settings.data_path = Some(p.to_string())
    }

    if settings.secret_key.is_none() ||
        settings.control_port.is_none() ||
        settings.grpc_port.is_none() ||
        settings.data_path.is_none()
    {
        error!(
            target: LOG_TARGET,
            "Control port, gRPC port, Data path or Secret Key has not been provided via command line or config file"
        );
        std::process::exit(1);
    }

    // Setup the local comms stack
    let listener_address: NetAddress = format!("0.0.0.0:{}", settings.control_port.unwrap()).parse().unwrap();
    let secret_key = CommsSecretKey::from_hex(settings.secret_key.unwrap().as_str()).unwrap();
    let public_key = CommsPublicKey::from_secret_key(&secret_key);

    // get and filter interfaces
    let interfaces: Vec<NetworkInterface> = datalink::interfaces()
        .into_iter()
        .filter(|interface| {
            !interface.is_loopback() && interface.is_up() && interface.ips.iter().any(|addr| addr.is_ipv4())
        })
        .collect();

    // select first interface
    if interfaces.first().is_none() {
        error!(
            target: LOG_TARGET,
            "No available network interface with an Ipv4 Address."
        );
        std::process::exit(1);
    }

    // get network interface and retrieve ipv4 address
    let interface = interfaces.first().unwrap().clone();
    let local_ip = interface
        .ips
        .iter()
        .find(|addr| addr.is_ipv4())
        .unwrap()
        .ip()
        .to_string();

    let local_net_address = match format!("{}:{}", local_ip, settings.control_port.unwrap()).parse() {
        Ok(na) => na,
        Err(_) => {
            error!(target: LOG_TARGET, "Could not resolve local IP address");
            std::process::exit(1);
        },
    };

    info!(target: LOG_TARGET, "Local Net Address: {:?}", local_net_address);

    let peer_database_name = public_key.to_hex();
    let node_identity = NodeIdentity::new(secret_key, public_key.clone(), local_net_address)
        .map(Arc::new)
        .unwrap();

    let config = WalletConfig {
        comms: CommsConfig {
            control_service: ControlServiceConfig {
                listener_address: listener_address.clone(),
                socks_proxy_address: None,
                requested_connection_timeout: Duration::from_millis(5000),
            },
            socks_proxy_address: None,
            host: "0.0.0.0".parse().unwrap(),
            node_identity,
            datastore_path: settings.data_path.unwrap(),
            peer_database_name,
            // TODO: configureable
            inbound_buffer_size: 30,
            outbound_buffer_size: 30,
            dht: Default::default(),
        },
        inbound_message_buffer_size: 100,
        public_key,
        database_path,
    };

    let wallet = Wallet::new(config).unwrap();

    // Add any provided peers to Peer Manager and Text Message Service Contacts
    if !contacts.peers.is_empty() {
        for p in contacts.peers.iter() {
            let pk = CommsPublicKey::from_hex(p.pub_key.as_str()).expect("Error parsing pub key from Hex");
            if let Ok(na) = p.address.clone().parse::<NetAddress>() {
                let peer = Peer::from_public_key_and_address(pk.clone(), na.clone()).unwrap();
                wallet.comms_services.peer_manager().add_peer(peer).unwrap();
                if let Err(e) = wallet.text_message_service.add_contact(Contact {
                    screen_name: p.screen_name.clone(),
                    pub_key: pk.clone(),
                    address: na.clone(),
                }) {
                    info!("Error adding config file contacts: {:?}", e);
                }
            }
        }
    }

    let wallet_server = WalletServer::new(settings.grpc_port.unwrap(), Arc::new(wallet));
    let _res = wallet_server.start();
}
