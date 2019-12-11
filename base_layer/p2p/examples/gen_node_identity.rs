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
use rand::{rngs::OsRng, Rng};
use std::{
    env::current_dir,
    fs,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_utilities::message_format::MessageFormat;

fn random_address() -> Multiaddr {
    let mut rng = OsRng::new().unwrap();
    let port = rng.gen_range(9000, std::u16::MAX);
    let socket_addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
    socket_addr.into()
}

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

fn main() {
    let matches = App::new("Peer file generator")
        .version("1.0")
        .about("Generates peer json files")
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

    let mut rng = OsRng::new().unwrap();
    let address = random_address();
    let node_identity = NodeIdentity::random(&mut rng, address, PeerFeatures::COMMUNICATION_NODE).unwrap();
    let json = node_identity.to_json().unwrap();
    let out_path = to_abs_path(matches.value_of("output").unwrap());
    fs::write(out_path, json).unwrap();
}
