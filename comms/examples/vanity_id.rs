// Copyright 2021. The Tari Project
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

use tari_comms::peer_manager::NodeId;
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey, tari_utilities::hex::Hex};

#[tokio::main]
async fn main() {
    let mut rng = rand::thread_rng();

    let mut target_hex_prefixes = vec!["3333", "5555", "7777", "9999", "bbbb", "dddd", "eeee"];
    for i in 0u64..1_000_000_000u64 {
        let (k, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_public_key(&pk);
        let node_id_hex = format!("{}", node_id.to_hex());
        if i % 10_000 == 0 {
            println!("{}", i);
        }

        let mut found = "not found";
        for p in target_hex_prefixes.iter() {
            if &&node_id_hex.as_str()[0..p.len()] == p {
                println!("Found in {} iterations", i);
                println!("Secret Key: {}", k.to_hex());
                println!("Public Key: {}", pk);
                println!("Node Id: {}", node_id_hex);
                println!("==================================================");
                found = p.clone();
                break;
            }
        }

        if found != "not found" {
            let pos = target_hex_prefixes.iter().position(|p| *p == found).unwrap();
            target_hex_prefixes.remove(pos);
        }

        if target_hex_prefixes.is_empty() {
            break;
        }
    }
}
