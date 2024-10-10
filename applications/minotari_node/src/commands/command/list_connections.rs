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
use tari_comms::PeerConnection;
use tari_core::base_node::state_machine_service::states::PeerMetadata;

use super::{CommandContext, HandleCommand};
use crate::{table::Table, utils::format_duration_basic};

/// Lists the peer connections currently held by this node
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.list_connections().await
    }
}

impl CommandContext {
    async fn list_connections_print_table(&mut self, conns: &[PeerConnection]) {
        let num_connections = conns.len();
        let mut table = Table::new();
        table.set_titles(vec![
            "NodeId",
            "Public Key",
            "Address",
            "Direction",
            "Age",
            "User Agent",
            "Info",
        ]);
        let peer_manager = self.network.peer_manager();
        for conn in conns {
            let peer = peer_manager
                .find_by_node_id(conn.peer_node_id())
                .await
                .expect("Unexpected peer database error")
                .expect("Peer not found");

            let chain_height = peer
                .get_metadata(1)
                .and_then(|v| bincode::deserialize::<PeerMetadata>(v).ok())
                .map(|metadata| format!("height: {}", metadata.metadata.best_block_height()));

            let ua = peer.user_agent;
            let rpc_sessions = self
                .rpc_server
                .get_num_active_sessions_for(peer.node_id.clone())
                .await
                .unwrap_or(0);
            table.add_row(row![
                peer.node_id,
                peer.public_key,
                conn.address(),
                conn.direction(),
                format_duration_basic(conn.age()),
                {
                    if ua.is_empty() {
                        "<unknown>"
                    } else {
                        ua.as_ref()
                    }
                },
                format!(
                    "{}hnd: {}, ss: {}, rpc: {}",
                    chain_height.map(|s| format!("{}, ", s)).unwrap_or_default(),
                    // Exclude the handle held by list-connections
                    conn.handle_count().saturating_sub(1),
                    conn.substream_count(),
                    rpc_sessions
                ),
            ]);
        }

        table.print_stdout();

        println!("{} active connection(s)", num_connections);
    }
}

impl CommandContext {
    /// Function to process the list-connections command
    pub async fn list_connections(&mut self) -> Result<(), Error> {
        let conns = self.network.connectivity().get_active_connections().await?;
        let (mut nodes, mut clients) = conns
            .into_iter()
            .partition::<Vec<_>, _>(|a| a.peer_features().is_node());
        nodes.sort_by(|a, b| a.peer_node_id().cmp(b.peer_node_id()));
        clients.sort_by(|a, b| a.peer_node_id().cmp(b.peer_node_id()));

        println!();
        println!("Base Nodes");
        println!("----------");
        if nodes.is_empty() {
            println!("No active node connections.");
        } else {
            println!();
            self.list_connections_print_table(&nodes).await;
        }
        println!();
        println!("Wallets");
        println!("-------");
        if clients.is_empty() {
            println!("No active wallet connections.");
        } else {
            println!();
            self.list_connections_print_table(&clients).await;
        }
        Ok(())
    }
}
