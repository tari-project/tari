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

use std::time::Instant;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use minotari_app_utilities::utilities::UniNodeId;
use tari_comms::peer_manager::NodeId;
use tokio::task;

use super::{CommandContext, HandleCommand};

/// Attempt to connect to a known peer
#[derive(Debug, Parser)]
pub struct Args {
    /// hex public key or emoji id
    node_id: UniNodeId,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.dial_peer(args.node_id.into()).await
    }
}

impl CommandContext {
    /// Function to process the dial-peer command
    pub async fn dial_peer(&self, dest_node_id: NodeId) -> Result<(), Error> {
        let connectivity = self.comms.connectivity();
        task::spawn(async move {
            let start = Instant::now();
            println!("☎️  Dialing peer...");

            match connectivity.dial_peer(dest_node_id).await {
                Ok(connection) => {
                    println!("⚡️ Peer connected in {}ms!", start.elapsed().as_millis());
                    println!("Connection: {}", connection);
                },
                Err(err) => {
                    println!("☠️ {}", err);
                },
            }
        });
        Ok(())
    }
}
