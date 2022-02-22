use std::time::Instant;
use std::ops::Deref;

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
