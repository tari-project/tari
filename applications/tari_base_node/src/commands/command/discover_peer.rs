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

use std::{ops::Deref, time::Instant};

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::UniPublicKey;
use tari_comms_dht::envelope::NodeDestination;
use tari_crypto::ristretto::RistrettoPublicKey;

use super::{CommandContext, HandleCommand};

/// Attempt to discover a peer on the Tari network
#[derive(Debug, Parser)]
pub struct Args {
    /// hex public key or emoji id
    id: UniPublicKey,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.discover_peer(Box::new(args.id.into())).await
    }
}

impl CommandContext {
    /// Function to process the discover-peer command
    pub async fn discover_peer(&mut self, dest_pubkey: Box<RistrettoPublicKey>) -> Result<(), Error> {
        let start = Instant::now();
        println!("🌎 Peer discovery started.");
        let peer = self
            .discovery_service
            .discover_peer(dest_pubkey.deref().clone(), NodeDestination::PublicKey(dest_pubkey))
            .await?;
        println!("⚡️ Discovery succeeded in {}ms!", start.elapsed().as_millis());
        println!("This peer was found:");
        println!("{}", peer);
        Ok(())
    }
}
