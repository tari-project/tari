// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{discovery::DhtDiscoveryError, envelope::NodeDestination, proto::dht::DiscoveryResponseMessage};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use std::{
    fmt::{Display, Error, Formatter},
    time::Duration,
};
use tari_comms::{
    peer_manager::{NodeId, Peer},
    types::CommsPublicKey,
};
use tokio::time;

#[derive(Debug)]
pub struct DiscoverPeerRequest {
    /// The public key of the peer to be discovered. The message will be encrypted with a DH shared
    /// secret using this public key.
    pub dest_public_key: Box<CommsPublicKey>,
    /// The node id of the peer to be discovered, if it is known. Providing the `NodeId` allows
    /// discovery requests to reach their destination more quickly.
    pub dest_node_id: Option<NodeId>,
    /// The destination to include in the comms header.
    /// `Undisclosed` will require nodes to propagate the message across the network, presumably eventually
    /// reaching the destination node (the node which can decrypt the message). This will happen without
    /// any intermediary nodes knowing who is being searched for.
    /// `NodeId` will direct the discovery request closer to the destination or network region.
    /// `PublicKey` will be propagated across the network. If any node knows the peer, the request can be
    /// forwarded to them immediately. However, more nodes will know that this node is being searched for
    /// which may slightly compromise privacy.
    pub destination: NodeDestination,
}

impl Display for DiscoverPeerRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.debug_struct("DiscoverPeerRequest")
            .field("dest_public_key", &format!("{}", self.dest_public_key))
            .field(
                "dest_node_id",
                &self
                    .dest_node_id
                    .as_ref()
                    .map(|node_id| format!("{}", node_id))
                    .unwrap_or_else(|| "None".to_string()),
            )
            .field("destination", &format!("{}", self.destination))
            .finish()
    }
}

#[derive(Debug)]
pub enum DhtDiscoveryRequest {
    DiscoverPeer(Box<(DiscoverPeerRequest, oneshot::Sender<Result<Peer, DhtDiscoveryError>>)>),
    NotifyDiscoveryResponseReceived(Box<DiscoveryResponseMessage>),
}

impl Display for DhtDiscoveryRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use DhtDiscoveryRequest::*;
        match self {
            DiscoverPeer(boxed) => write!(f, "DiscoverPeer({})", boxed.0),
            NotifyDiscoveryResponseReceived(discovery_resp) => {
                write!(f, "NotifyDiscoveryResponseReceived({:#?})", discovery_resp)
            },
        }
    }
}

#[derive(Clone)]
pub struct DhtDiscoveryRequester {
    sender: mpsc::Sender<DhtDiscoveryRequest>,
    discovery_timeout: Duration,
}

impl DhtDiscoveryRequester {
    pub fn new(sender: mpsc::Sender<DhtDiscoveryRequest>, discovery_timeout: Duration) -> Self {
        Self {
            sender,
            discovery_timeout,
        }
    }

    pub async fn discover_peer(
        &mut self,
        dest_public_key: Box<CommsPublicKey>,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    ) -> Result<Peer, DhtDiscoveryError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = DiscoverPeerRequest {
            dest_public_key,
            dest_node_id,
            destination,
        };
        self.sender
            .send(DhtDiscoveryRequest::DiscoverPeer(Box::new((request, reply_tx))))
            .await?;

        time::timeout(
            self.discovery_timeout,
            reply_rx
        )
            .await
            // Timeout?
            .map_err(|_| DhtDiscoveryError::DiscoveryTimeout)?
            // Channel error?
            .map_err(|_| DhtDiscoveryError::ReplyCanceled)?
    }

    pub async fn notify_discovery_response_received(
        &mut self,
        response: DiscoveryResponseMessage,
    ) -> Result<(), DhtDiscoveryError>
    {
        self.sender
            .send(DhtDiscoveryRequest::NotifyDiscoveryResponseReceived(Box::new(response)))
            .await?;

        Ok(())
    }
}
