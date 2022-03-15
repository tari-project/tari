use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::UniNodeId;
use tari_comms::peer_manager::NodeId;
use tari_p2p::services::liveness::LivenessEvent;
use tokio::sync::broadcast;

use super::{CommandContext, HandleCommand};

/// Send a ping to a known peer and wait for a pong reply
#[derive(Debug, Parser)]
pub struct Args {
    /// hex public key or emoji id
    node_id: UniNodeId,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.ping_peer(args.node_id.into()).await
    }
}

impl CommandContext {
    /// Function to process the dial-peer command
    pub async fn ping_peer(&mut self, dest_node_id: NodeId) -> Result<(), Error> {
        println!("🏓 Pinging peer...");
        let mut liveness_events = self.liveness.get_event_stream();

        self.liveness.send_ping(dest_node_id.clone()).await?;
        loop {
            match liveness_events.recv().await {
                Ok(event) => {
                    if let LivenessEvent::ReceivedPong(pong) = &*event {
                        if pong.node_id == dest_node_id {
                            println!(
                                "🏓️ Pong received, latency in is {:.2?}!",
                                pong.latency.unwrap_or_default()
                            );
                            break;
                        }
                    }
                },
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                },
                _ => {},
            }
        }
        Ok(())
    }
}
