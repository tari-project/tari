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

use std::{
    fmt::{Display, Error, Formatter},
    time::Duration,
};

use tari_comms::{peer_manager::Peer, types::CommsPublicKey};
use tokio::{
    sync::{mpsc, oneshot},
    time,
};

use super::DhtDiscoveryError;
use crate::{envelope::NodeDestination, proto::dht::DiscoveryResponseMessage};

#[derive(Debug)]
pub enum DhtDiscoveryRequest {
    DiscoverPeer(
        Box<CommsPublicKey>,
        NodeDestination,
        oneshot::Sender<Result<Peer, DhtDiscoveryError>>,
    ),
    NotifyDiscoveryResponseReceived(Box<DiscoveryResponseMessage>),
}

impl Display for DhtDiscoveryRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use DhtDiscoveryRequest::*;
        match self {
            DiscoverPeer(public_key, dest, _) => write!(f, "DiscoverPeer({}, {})", public_key, dest),
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

    /// Initiate a peer discovery
    ///
    /// ## Arguments
    /// - `dest_public_key` - The public key of the recipient used to create a shared ECDH key which in turn is used to
    /// encrypt the discovery message
    /// - `destination` - The `NodeDestination` to use in the DhtHeader when sending a discovery message.
    ///    - `Unknown` destination will maintain complete privacy, the trade off is that discovery needs to propagate
    ///      the entire network to reach the destination and so may take longer
    ///    - `NodeId` Instruct propagation nodes to direct the message to peers closer to the given NodeId. The `NodeId`
    ///      may be directed to a region close to the real destination (somewhat private) or directed at a particular
    ///      node (not private)
    ///    - `PublicKey` if any node on the network knows this public key, the message will be directed to that node.
    ///      This sacrifices privacy for more efficient discovery in terms of network bandwidth and may result in
    ///      quicker discovery times.
    pub async fn discover_peer(
        &mut self,
        dest_public_key: CommsPublicKey,
        destination: NodeDestination,
    ) -> Result<Peer, DhtDiscoveryError> {
        let (reply_tx, reply_rx) = oneshot::channel();

        self.sender
            .send(DhtDiscoveryRequest::DiscoverPeer(
                Box::new(dest_public_key),
                destination,
                reply_tx,
            ))
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

    pub(crate) async fn notify_discovery_response_received(
        &mut self,
        response: DiscoveryResponseMessage,
    ) -> Result<(), DhtDiscoveryError> {
        self.sender
            .send(DhtDiscoveryRequest::NotifyDiscoveryResponseReceived(Box::new(response)))
            .await?;

        Ok(())
    }
}
