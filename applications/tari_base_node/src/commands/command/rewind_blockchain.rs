use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_core::base_node::comms_interface::BlockEvent;

use super::{CommandContext, HandleCommand};

/// Rewinds the blockchain to the given height
#[derive(Debug, Parser)]
pub struct Args {
    /// new_height must be less than the current height
    new_height: u64,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.rewind_blockchain(args.new_height).await
    }
}

impl CommandContext {
    pub async fn rewind_blockchain(&self, new_height: u64) -> Result<(), Error> {
        let blocks = self.blockchain_db.rewind_to_height(new_height).await?;
        if !blocks.is_empty() {
            self.node_service
                .publish_block_event(BlockEvent::BlockSyncRewind(blocks));
        }
        Ok(())
    }
}
