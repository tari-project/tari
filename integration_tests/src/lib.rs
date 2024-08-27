//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{convert::TryFrom, net::TcpListener, ops::Range, path::PathBuf, process, time::Duration};

use rand::Rng;

pub mod base_node_process;
pub mod chat_client;
pub mod chat_ffi;
pub mod ffi;
pub mod merge_mining_proxy;
pub mod miner;
pub mod transaction;
pub mod wallet;
pub mod wallet_ffi;
pub mod wallet_process;
pub mod world;

pub use world::TariWorld;

pub fn get_port(range: Range<u16>) -> Option<u64> {
    let min = range.clone().min().expect("A minimum possible port number");
    let max = range.max().expect("A maximum possible port number");

    loop {
        let port = rand::thread_rng().gen_range(min..max);

        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(u64::from(port));
        }
    }
}

pub fn get_base_dir() -> PathBuf {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root.join(format!("tests/temp/cucumber_{}", process::id()))
}

pub async fn wait_for_service(port: u64) {
    // The idea is that if the port is taken it means the service is running.
    // If the port is not taken the service hasn't come up yet
    let max_tries = 4*60;
    let mut attempts = 0;

    loop {
        if TcpListener::bind(("127.0.0.1", u16::try_from(port).unwrap())).is_err() {
            return;
        }

        if attempts >= max_tries {
            panic!("Service on port {} never started", port);
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
        attempts += 1;
    }
}

pub async fn get_peer_addresses(world: &TariWorld, peers: &[String]) -> Vec<String> {
    peers
        .iter()
        .map(|peer_string| {
            let peer = world.base_nodes.get(peer_string.as_str()).unwrap().identity.to_peer();
            peer.to_short_string()
        })
        .collect()
}
