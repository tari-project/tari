use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::UniNodeId;
use tari_comms::peer_manager::NodeId;
use thiserror::Error;

use super::{CommandContext, HandleCommand};

/// Bans a peer
#[derive(Debug, Parser)]
pub struct ArgsBan {
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
impl HandleCommand<ArgsUnban> for CommandContext {
    async fn handle_command(&mut self, args: ArgsUnban) -> Result<(), Error> {
        let node_id = args.node_id.into();
        let duration = Duration::from_secs(args.length);
        self.ban_peer(node_id, duration, false).await
    }
}

#[derive(Error, Debug)]
enum ArgsError {
    #[error("Cannot ban our own node")]
    BanSelf,
}

impl CommandContext {
    pub async fn ban_peer(&mut self, node_id: NodeId, duration: Duration, must_ban: bool) -> Result<(), Error> {
        if self.base_node_identity.node_id() == &node_id {
            Err(ArgsError::BanSelf.into())
        } else if must_ban {
            self.connectivity
                .ban_peer_until(node_id.clone(), duration, "UI manual ban".to_string())
                .await?;
            println!("Peer was banned in base node.");
            Ok(())
        } else {
            self.peer_manager.unban_peer(&node_id).await?;
            println!("Peer ban was removed from base node.");
            Ok(())
        }
    }
}
