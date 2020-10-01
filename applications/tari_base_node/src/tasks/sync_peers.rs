//  Copyright 2020, The Tari Project
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

use futures::StreamExt;
use log::*;
use std::sync::Arc;
use tari_comms::{ConnectionManagerEvent, PeerManager};
use tokio::sync::broadcast;

const LOG_TARGET: &str = "c::bn::sync_peers";

/// Asynchronously syncs peers with base node, adding peers if the peer is not already known
///
/// ## Parameters
/// `events_rx` - The event stream
/// `base_node_peer_manager` - The peer manager for the base node wrapped in an atomic reference counter
/// `wallet_peer_manager` - The peer manager for the base node's wallet wrapped in an atomic reference counter
pub async fn sync_peers(
    mut events_rx: broadcast::Receiver<Arc<ConnectionManagerEvent>>,
    base_node_peer_manager: Arc<PeerManager>,
    wallet_peer_manager: Arc<PeerManager>,
)
{
    while let Some(Ok(event)) = events_rx.next().await {
        if let ConnectionManagerEvent::PeerConnected(conn) = &*event {
            if !wallet_peer_manager.exists_node_id(conn.peer_node_id()).await {
                match base_node_peer_manager.find_by_node_id(conn.peer_node_id()).await {
                    Ok(mut peer) => {
                        peer.unset_id();
                        if let Err(err) = wallet_peer_manager.add_peer(peer).await {
                            warn!(target: LOG_TARGET, "Failed to add peer to wallet: {:?}", err);
                        }
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Failed to find peer in base node: {:?}", err);
                    },
                }
            }
        }
    }
    info!(target: LOG_TARGET, "sync_peers task shutdown");
}
