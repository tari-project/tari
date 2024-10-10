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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use minotari_app_utilities::utilities::UniPeerId;
use tari_network::{identity::PeerId, ToPeerId};
use tari_p2p::services::liveness::LivenessEvent;
use tokio::{sync::broadcast::error::RecvError, task};

use super::{CommandContext, HandleCommand};

/// Send a ping to a known peer and wait for a pong reply
#[derive(Debug, Parser)]
pub struct Args {
    /// hex public key or emoji id
    node_id: UniPeerId,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.ping_peer(args.node_id.to_peer_id()).await
    }
}

impl CommandContext {
    /// Function to process the dial-peer command
    pub async fn ping_peer(&mut self, dest_peer_id: PeerId) -> Result<(), Error> {
        println!("🏓 Pinging peer...");
        let mut liveness_events = self.liveness.get_event_stream();
        let mut liveness = self.liveness.clone();
        task::spawn(async move {
            if let Err(e) = liveness.send_ping(dest_peer_id.clone()).await {
                println!("🏓 Ping failed to send to {}: {}", dest_peer_id, e);
                return;
            }
            loop {
                match liveness_events.recv().await {
                    Ok(event) => {
                        if let LivenessEvent::ReceivedPong(pong) = &*event {
                            if pong.peer_id == dest_peer_id {
                                println!(
                                    "🏓️ Pong received, round-trip-time is {:.2?}!",
                                    pong.latency.unwrap_or_default()
                                );
                                break;
                            }
                        }
                    },
                    Err(RecvError::Closed) => {
                        break;
                    },
                    Err(RecvError::Lagged(_)) => {},
                }
            }
        });
        Ok(())
    }
}
