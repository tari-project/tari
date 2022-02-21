use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::consts;

use super::{CommandContext, HandleCommand};

#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        println!("Checking for updates (current version: {})...", consts::APP_VERSION);
        match self.software_updater.check_for_updates().await {
            Some(update) => {
                println!(
                    "Version {} of the {} is available: {} (sha: {})",
                    update.version(),
                    update.app(),
                    update.download_url(),
                    update.to_hash_hex()
                );
            },
            None => {
                println!("No updates found.",);
            },
        }
        Ok(())
    }
}
