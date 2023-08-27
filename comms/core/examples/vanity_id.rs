// Copyright 2021. The Taiji Project
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

use std::{
    env,
    error::Error,
    fmt::Write as FmtWrite,
    fs,
    io::{stdout, Write},
    time::Instant,
};

use anyhow::anyhow;
use multiaddr::Multiaddr;
use rand::rngs::OsRng;
use taiji_comms::{
    peer_manager::{NodeId, PeerFeatures},
    NodeIdentity,
};
use tari_crypto::{
    keys::PublicKey,
    ristretto::RistrettoPublicKey,
    tari_utilities::{message_format::MessageFormat, ByteArray},
};
use tokio::{sync::mpsc, task, task::JoinHandle};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let target_hex_prefixes = env::args().skip(1).map(|s| s.to_lowercase()).collect::<Vec<String>>();

    for prefix in target_hex_prefixes {
        let now = Instant::now();
        let (tx, mut rx) = mpsc::channel(1);
        println!("Finding {}...", prefix);
        spawn_identity_miners(16, tx, prefix);
        let found = rx.recv().await.unwrap();
        rx.close();
        // Drain any answers that have been found by multiple workers (common for small prefixes)
        while rx.try_recv().is_ok() {}
        println!("Found '{}' in {:.2?}", found.node_id(), now.elapsed());
        println!("==================================================");
        println!("{}", found);
        write_identity(found);
    }
    println!("Done");

    Ok(())
}

fn write_identity(identity: NodeIdentity) {
    let tmp = env::temp_dir().join("vanity_ids");
    fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join(format!("{}.json", identity.node_id()));
    println!("Wrote '{}'", path.to_str().unwrap());
    fs::write(path, identity.to_json().unwrap()).unwrap();
}

fn spawn_identity_miners(
    num_miners: usize,
    tx: mpsc::Sender<NodeIdentity>,
    prefix: String,
) -> Vec<JoinHandle<Result<(), anyhow::Error>>> {
    (0..num_miners)
        .map(|id| {
            let tx = tx.clone();
            let prefix = prefix.clone();
            task::spawn_blocking(move || start_miner(id, prefix, tx))
        })
        .collect()
}

fn start_miner(id: usize, prefix: String, tx: mpsc::Sender<NodeIdentity>) -> Result<(), anyhow::Error> {
    let mut node_id_hex = String::with_capacity(26);
    for i in 0u64.. {
        let (k, pk) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let node_id = NodeId::from_public_key(&pk);
        node_id_hex.clear();
        for byte in node_id.as_bytes() {
            write!(&mut node_id_hex, "{:02x}", byte).expect("Unable to write");
        }
        if i % 50_000 == 0 {
            if i == 0 {
                println!("Worker #{} - started", id);
            } else {
                println!("Worker #{} - {} iterations", id, i);
            }
            stdout().flush()?;
        }

        if tx.is_closed() {
            break;
        }

        if node_id_hex[0..prefix.len()] == *prefix {
            if tx
                .try_send(NodeIdentity::new(
                    k,
                    vec![Multiaddr::empty()],
                    PeerFeatures::COMMUNICATION_NODE,
                ))
                .is_err()
            {
                eprintln!("Failed to send");
                return Err(anyhow!("Failed to send"));
            }
            break;
        }
    }
    Ok(())
}
