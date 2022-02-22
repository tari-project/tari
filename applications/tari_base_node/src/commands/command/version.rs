use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::consts;

use super::{CommandContext, HandleCommand};

/// Gets the current application version
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.print_version()
    }
}

impl CommandContext {
    /// Function process the version command
    pub fn print_version(&self) -> Result<(), Error> {
        println!("Version: {}", consts::APP_VERSION);
        println!("Author: {}", consts::APP_AUTHOR);
        println!("Avx2: {}", match cfg!(feature = "avx2") {
            true => "enabled",
            false => "disabled",
        });

        if let Some(ref update) = *self.software_updater.new_update_notifier().borrow() {
            println!(
                "Version {} of the {} is available: {} (sha: {})",
                update.version(),
                update.app(),
                update.download_url(),
                update.to_hash_hex()
            );
        }
        Ok(())
    }
}
