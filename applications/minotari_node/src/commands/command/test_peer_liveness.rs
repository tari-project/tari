//  Copyright 2022, The Tari Project
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
    fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    process,
    time::{Duration, Instant},
};

use anyhow::Error;
use async_trait::async_trait;
use chrono::Local;
use clap::Parser;
use minotari_app_utilities::utilities::UniPublicKey;
use tari_common_types::types::PublicKey;
use tari_comms::{
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
};
use tari_p2p::services::liveness::{LivenessEvent, LivenessHandle};
use tokio::{sync::watch, task};

use super::{CommandContext, HandleCommand};

/// Adds a peer
#[derive(Debug, Parser)]
pub struct ArgsTestPeerLiveness {
    /// The public key of the peer to be tested
    public_key: UniPublicKey,
    /// The address of the peer to be tested
    address: Multiaddr,
    /// Auto exit the base node after test
    exit: Option<bool>,
    /// Write the responsiveness result to file - results will be written to
    /// 'peer_liveness_test.log'
    output_to_file: Option<bool>,
    /// Start with a new log file
    refresh_file: Option<bool>,
    /// Optional output directory (otherwise current directory will be used)
    output_directory: Option<PathBuf>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum PingResult {
    Initial,
    Success,
    Fail,
}

#[async_trait]
impl HandleCommand<ArgsTestPeerLiveness> for CommandContext {
    async fn handle_command(&mut self, args: ArgsTestPeerLiveness) -> Result<(), Error> {
        println!("\nTesting peer liveness...\n");
        let peer_manager = self.comms.peer_manager();

        let public_key = args.public_key.into();
        if *self.comms.node_identity().public_key() == public_key {
            return Err(Error::msg("Self liveness test not supported"));
        }
        let node_id = NodeId::from_public_key(&public_key);
        let node_id_clone = node_id.clone();
        let public_key_clone = public_key.clone();
        let address_clone = args.address.clone();

        // Remove the peer from the peer manager (not the peer db)
        let _res = peer_manager.delete_peer(&node_id).await;

        // Create a new peer with the given address, if the peer exists, this will merge the given address
        let peer = Peer::new(
            public_key.clone(),
            node_id.clone(),
            MultiaddressesWithStats::from_addresses_with_source(vec![args.address], &PeerAddressSource::Config),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            vec![],
            String::new(),
        );
        peer_manager.add_peer(peer).await?;

        let (tx, mut rx) = watch::channel(PingResult::Initial);

        // Attempt to dial and ping the peer
        let start = Instant::now();
        for _ in 0..5 {
            if self.dial_peer(node_id.clone()).await.is_ok() {
                println!("🏓 Peer ({}, {}) dialed successfully", node_id, public_key);
                let liveness = self.liveness.clone();
                task::spawn(async move {
                    ping_peer_liveness(liveness, node_id, public_key, tx).await;
                });
                // Break if the dial was successful
                break;
            } else {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }

        // Wait for the liveness test to complete
        loop {
            tokio::select! {
                _ = rx.changed() => {
                    let test_duration = start.elapsed();
                    let responsive = *rx.borrow();
                    let date_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

                    print_results_to_console(&date_time, responsive, &public_key_clone, &node_id_clone, &address_clone, test_duration);

                    if let Some(true) = args.output_to_file {
                        print_to_file(
                            &date_time,
                            responsive,
                            args.output_directory,
                            args.refresh_file,
                            public_key_clone,
                            address_clone,
                            test_duration
                        ).await;
                    }

                    if let Some(true) = args.exit {
                        println!("The liveness test is complete and base node will now exit\n");
                        self.shutdown.trigger();
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        match responsive {
                            PingResult::Success => process::exit(0),
                            _ => process::exit(1),
                        }
                    }

                    break;
                },

                _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            }
        }

        Ok(())
    }
}

fn print_results_to_console(
    date_time: &str,
    responsive: PingResult,
    public_key: &PublicKey,
    node_id: &NodeId,
    address: &Multiaddr,
    test_duration: Duration,
) {
    println!();
    if responsive == PingResult::Success {
        println!("✅ Peer is responsive");
    } else {
        println!("❌ Peer is unresponsive");
    }
    println!("  Date Time:     {}", date_time);
    println!("  Public Key:    {}", public_key);
    println!("  Node ID:       {}", node_id);
    println!("  Address:       {}", address);
    println!("  Result:        {:?}", responsive);
    println!("  Test Duration: {:.2?}", test_duration);
    println!();
}

async fn ping_peer_liveness(
    mut liveness: LivenessHandle,
    node_id: NodeId,
    public_key: PublicKey,
    tx: watch::Sender<PingResult>,
) {
    let mut liveness_events = liveness.get_event_stream();
    if let Ok(nonce) = liveness.send_ping(node_id.clone()).await {
        println!("🏓 Pinging peer ({}, {}) with nonce {} ...", node_id, public_key, nonce);
        for _ in 0..5 {
            match liveness_events.recv().await {
                Ok(event) => {
                    if let LivenessEvent::ReceivedPong(pong) = &*event {
                        if pong.node_id == node_id && pong.nonce == nonce {
                            println!(
                                "🏓️ Pong: peer ({}, {}) responded with nonce {}, round-trip-time is {:.2?}!",
                                pong.node_id,
                                public_key,
                                pong.nonce,
                                pong.latency.unwrap_or_default()
                            );
                            let _ = tx.send(PingResult::Success);
                            return;
                        }
                    }
                },
                Err(e) => {
                    println!("🏓 Ping peer ({}, {}) gave error: {}", node_id, public_key, e);
                },
            }
        }
        let _ = tx.send(PingResult::Fail);
    }
}

async fn print_to_file(
    date_time: &str,
    responsive: PingResult,
    output_directory: Option<PathBuf>,
    refresh_file: Option<bool>,
    public_key: PublicKey,
    address: Multiaddr,
    test_duration: Duration,
) {
    let test_result = if responsive == PingResult::Success {
        "PASS"
    } else {
        "FAIL"
    };

    let file_name = "peer_liveness_test.csv";
    let file_path = if let Some(path) = output_directory.clone() {
        if let Ok(true) = fs::exists(&path) {
            path.join(file_name)
        } else if fs::create_dir_all(&path).is_ok() {
            path.join(file_name)
        } else {
            PathBuf::from(file_name)
        }
    } else {
        PathBuf::from(file_name)
    };

    if let Some(true) = refresh_file {
        let _unused = fs::remove_file(&file_path);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let write_header = !file_path.exists();
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(file_path.clone()) {
        let mut file_content = String::new();
        if write_header {
            file_content.push_str("Date Time,Public Key,Address,Result,Test Duration\n");
        }
        file_content.push_str(&format!(
            "{},{},{},{},{:.2?}",
            date_time, public_key, address, test_result, test_duration
        ));
        match writeln!(file, "{}", file_content) {
            Ok(_) => {
                println!("📝 Test result written to file: {}", file_path.display());
            },
            Err(e) => {
                println!("❌ Error writing test result to file: {}", e);
            },
        }
    }
}
