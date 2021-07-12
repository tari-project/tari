// Copyright 2021. The Tari Project
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

use crate::{
    dan_layer::{
        models::{HotStuffMessage, HotStuffMessageType, HotStuffTreeNode, Proposal, QuorumCertificate, View},
        services::{infrastructure_services::InboundConnectionService, BftReplicaService},
        workers::states::{ConsensusWorkerStateEvent, State},
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;
use futures::StreamExt;
use tari_shutdown::{Shutdown, ShutdownSignal};

pub struct Prepare<TInboundConnectionService: InboundConnectionService + Send> {
    // bft_service: Box<dyn BftReplicaService>,
    locked_qc: QuorumCertificate,
    inbound_connection: TInboundConnectionService,
}

#[async_trait]
impl<TInboundConnectionService: InboundConnectionService + Send + Sync> State for Prepare<TInboundConnectionService> {
    async fn next_event(
        &mut self,
        current_view: &View,
        shutdown: &ShutdownSignal,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        // let peekable_shutdown = shutdown.peekable();
        if current_view.is_leader {
            self.wait_for_new_view_messages().await;
            let high_qc = self.find_highest_qc();
            let proposal = self.create_proposal();
            self.broadcast_proposal(proposal, high_qc);
        }
        // while peekable_shutdown.peek().await {
        // As replica
        let m = self.wait_for_message().await;
        if !m.matches(HotStuffMessageType::Prepare, current_view.view_id) {
            unimplemented!("Wrong message type received, log");
        }
        if self.does_extend(m.node(), m.justify().node()) {
            if !self.is_safe_node(m.node(), m.justify()) {
                unimplemented!("Node is not safe")
            }

            self.send_vote_to_leader(m.node());
            return Ok(ConsensusWorkerStateEvent::Prepared);
        } else {
            unimplemented!("Did not extend from qc.justify.node")
        }
        // }

        Ok(ConsensusWorkerStateEvent::ShutdownReceived)
    }
}
impl<TInboundConnectionService: InboundConnectionService + Send> Prepare<TInboundConnectionService> {
    pub fn new(inbound_connection: TInboundConnectionService) -> Self {
        Self {
            locked_qc: QuorumCertificate::new(),
            inbound_connection,
        }
    }

    async fn wait_for_new_view_messages(&self) -> HotStuffMessage {
        unimplemented!()
    }

    fn find_highest_qc(&self) -> QuorumCertificate {
        unimplemented!()
    }

    fn create_proposal(&self) -> Proposal {
        unimplemented!()
    }

    async fn wait_for_message(&mut self) -> HotStuffMessage {
        self.inbound_connection.receive_message().await
    }

    fn broadcast_proposal(&self, proposal: Proposal, high_qc: QuorumCertificate) {
        unimplemented!()
    }

    fn does_extend(&self, node: &HotStuffTreeNode, from: &HotStuffTreeNode) -> bool {
        unimplemented!()
    }

    fn is_safe_node(&self, node: &HotStuffTreeNode, quorum_certificate: &QuorumCertificate) -> bool {
        self.does_extend(node, quorum_certificate.node())
    }

    fn send_vote_to_leader(&self, node: &HotStuffTreeNode) {
        unimplemented!()
    }
}
