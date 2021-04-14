// Copyright 2020. The Tari Project
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

use crate::base_node_service::{
    error::BaseNodeServiceError,
    handle::{BaseNodeServiceRequest, BaseNodeServiceResponse},
    service::{BaseNodeState, OnlineState},
};
use futures::{pin_mut, StreamExt};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::peer_manager::Peer;
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;

/// TODO Move this into the test support utilities when we remove the Test Harness feature from this crate
pub struct MockBaseNodeService {
    request_stream: Option<Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>>,
    pub base_node_peer: Option<Peer>,
    pub state: BaseNodeState,
    shutdown_signal: Option<ShutdownSignal>,
}

impl MockBaseNodeService {
    pub fn new(
        request_stream: Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>,
        shutdown_signal: ShutdownSignal,
    ) -> MockBaseNodeService
    {
        let base_node_peer = None;
        let state = Default::default();
        MockBaseNodeService {
            request_stream: Some(request_stream),
            base_node_peer,
            state,
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn run(mut self) -> Result<(), BaseNodeServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Wallet Base Node Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Wallet Base Node Service initialized without shutdown signal");

        loop {
            futures::select! {
                // Incoming requests
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request).await;
                    let _ = reply_tx.send(response);
                },

                // Shutdown
                _ = shutdown_signal => {
                    break Ok(());
                }
            }
        }
    }

    /// Set the mock server state, either online and synced to a specific height, or offline with None
    pub fn set_base_node_state(&mut self, height: Option<u64>) {
        let (chain_metadata, is_synced, online) = match height {
            Some(height) => {
                let metadata = ChainMetadata::new(height, Vec::new(), 0, 0, 0);
                (Some(metadata), Some(true), OnlineState::Online)
            },
            None => (None, None, OnlineState::Offline),
        };
        self.state = BaseNodeState {
            chain_metadata,
            is_synced,
            updated: None,
            latency: None,
            online,
            base_node_peer: self.state.base_node_peer.clone(),
        }
    }

    pub fn set_default_base_node_state(&mut self) {
        let metadata = ChainMetadata::new(std::u64::MAX, Vec::new(), 0, 0, 0);
        self.state = BaseNodeState {
            chain_metadata: Some(metadata),
            is_synced: Some(true),
            updated: None,
            latency: None,
            online: OnlineState::Online,
            base_node_peer: None,
        }
    }

    fn set_base_node_peer(&mut self, peer: Peer) {
        self.state.base_node_peer = Some(peer);
    }

    /// This handler is called when requests arrive from the various streams
    async fn handle_request(
        &mut self,
        request: BaseNodeServiceRequest,
    ) -> Result<BaseNodeServiceResponse, BaseNodeServiceError>
    {
        match request {
            BaseNodeServiceRequest::SetBaseNodePeer(peer) => {
                self.set_base_node_peer(*peer);
                Ok(BaseNodeServiceResponse::BaseNodePeerSet)
            },
            BaseNodeServiceRequest::GetChainMetadata => Ok(BaseNodeServiceResponse::ChainMetadata(
                self.state.chain_metadata.clone(),
            )),
        }
    }
}
