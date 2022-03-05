use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::UniNodeId;
use tari_comms::peer_manager::NodeId;

use super::{CommandContext, HandleCommand};
use crate::LOG_TARGET;

/// Bans a peer
#[derive(Debug, Parser)]
pub struct ArgsBan {
    /// hex public key or emoji id
    node_id: UniNodeId,
    /// length of time to ban the peer for in seconds
    #[clap(default_value_t = std::u64::MAX)]
    length: u64,
}

/// Removes a peer ban
#[derive(Debug, Parser)]
pub struct ArgsUnban {
    /// hex public key or emoji id
    node_id: UniNodeId,
    /// length of time to ban the peer for in seconds
    #[clap(default_value_t = std::u64::MAX)]
    length: u64,
}

#[async_trait]
impl HandleCommand<ArgsBan> for CommandContext {
    async fn handle_command(&mut self, args: ArgsBan) -> Result<(), Error> {
        let node_id = args.node_id.into();
        let duration = Duration::from_secs(args.length);
        self.ban_peer(node_id, duration, true).await
    }
}

#[async_trait]
impl HandleCommand<ArgsUnban> for CommandContext {
    async fn handle_command(&mut self, args: ArgsUnban) -> Result<(), Error> {
        let node_id = args.node_id.into();
        let duration = Duration::from_secs(args.length);
        self.ban_peer(node_id, duration, false).await
    }
}

impl CommandContext {
    pub async fn ban_peer(&mut self, node_id: NodeId, duration: Duration, must_ban: bool) -> Result<(), Error> {
        if self.base_node_identity.node_id() == &node_id {
            println!("Cannot ban our own node");
        } else if must_ban {
            // TODO: Use errors
            match self
                .connectivity
                .ban_peer_until(node_id.clone(), duration, "UI manual ban".to_string())
                .await
            {
                Ok(_) => println!("Peer was banned in base node."),
                Err(err) => {
                    println!("Failed to ban peer: {:?}", err);
                    log::error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                },
            }
        } else {
            match self.peer_manager.unban_peer(&node_id).await {
                Ok(_) => {
                    println!("Peer ban was removed from base node.");
                },
                Err(err) if err.is_peer_not_found() => {
                    println!("Peer not found in base node");
                },
                Err(err) => {
                    println!("Failed to ban peer: {:?}", err);
                    log::error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                },
            }
        }
        Ok(())
    }
}
