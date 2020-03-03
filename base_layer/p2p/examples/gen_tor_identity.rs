//  Copyright 2019 The Tari Project
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

/// Generates a random node identity JSON file. A node identity contains a node's public and secret keys, it's node
/// id and an address used to establish peer connections. The files generated from this example are used to
/// populate the peer manager in other examples.
use clap::{App, Arg};
use std::{env::current_dir, fs, path::Path};
use tari_comms::{multiaddr::Multiaddr, tor};
use tari_crypto::tari_utilities::message_format::MessageFormat;

fn to_abs_path(path: &str) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_str().unwrap().to_string()
    } else {
        let mut abs_path = current_dir().unwrap();
        abs_path.push(path);
        abs_path.to_str().unwrap().to_string()
    }
}

#[tokio_macros::main]
async fn main() {
    let matches = App::new("Tor identity file generator")
        .version("1.0")
        .about("Generates peer json files")
        .arg(
            Arg::with_name("tor-control-addr")
                .value_name("TOR_CONTROL_ADDR")
                .long("tor-control-addr")
                .short("t")
                .help("The address of the tor control server")
                .takes_value(true)
                .default_value("/ip4/127.0.0.1/tcp/9051"),
        )
        .arg(
            Arg::with_name("onion-port")
                .value_name("ONION_PORT")
                .long("onion-port")
                .short("p")
                .help("The port to use for the onion address")
                .takes_value(true)
                .default_value("9999"),
        )
        .arg(
            Arg::with_name("output")
                .value_name("FILE")
                .long("output")
                .short("o")
                .help("The relative path of the file to output")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    let tor_control_addr = matches
        .value_of("tor-control-addr")
        .unwrap()
        .parse::<Multiaddr>()
        .expect("Invalid tor-control-addr");

    let port = matches
        .value_of("onion-port")
        .unwrap()
        .parse::<u16>()
        .expect("Invalid port");

    let hidden_service = tor::HiddenServiceBuilder::new()
        .with_port_mapping(port)
        .with_control_server_address(tor_control_addr)
        .finish()
        .await
        .unwrap();

    let json = hidden_service.get_tor_identity().to_json().unwrap();
    let out_path = to_abs_path(matches.value_of("output").unwrap());
    fs::write(out_path, json).unwrap();
}
