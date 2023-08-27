//  Copyright 2022, The Taiji Project
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

use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use minotaiji_app_utilities::utilities::UniNodeId;
use taiji_comms::peer_manager::NodeId;
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
            self.comms
                .connectivity()
                .ban_peer_until(node_id.clone(), duration, "UI manual ban".to_string())
                .await?;
            println!("Peer was banned in base node.");
            Ok(())
        } else {
            self.comms.peer_manager().unban_peer(&node_id).await?;
            println!("Peer ban was removed from base node.");
            Ok(())
        }
    }
}
