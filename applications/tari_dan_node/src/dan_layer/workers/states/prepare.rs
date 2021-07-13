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
        workers::states::ConsensusWorkerStateEvent,
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;
use futures::StreamExt;
use std::any::Any;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::time::{delay_for, Duration};

pub struct Prepare<TInboundConnectionService: InboundConnectionService + Send> {
    // bft_service: Box<dyn BftReplicaService>,
    locked_qc: QuorumCertificate,
    inbound_connection: TInboundConnectionService,
}

impl<TInboundConnectionService: InboundConnectionService + Send> Prepare<TInboundConnectionService> {
    pub fn new(inbound_connection: TInboundConnectionService) -> Self {
        Self {
            locked_qc: QuorumCertificate::new(),
            inbound_connection,
        }
    }

    pub async fn next_event(
        &mut self,
        current_view: &View,
        timeout: Duration,
        shutdown: &ShutdownSignal,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut next_event_result = ConsensusWorkerStateEvent::Errored {
            reason: "loop ended without setting this event".to_string(),
        };

        loop {
            tokio::select! {
                message = self.wait_for_message() => {
                    if current_view.is_leader() {
                        if let Some(result) = self.process_leader_message(&message).await?{
                           next_event_result = result;
                            break;
                        }

                    }
                    if let Some(result) = self.process_replica_message(&message, &current_view).await? {
                        next_event_result = result;
                        break;
                    }

                },
                _ = delay_for(timeout) =>  {
                    dbg!("Time out");
                    // TODO: perhaps this should be from the time the state was entered
                    next_event_result = ConsensusWorkerStateEvent::TimedOut;
                    break;
                }
                // _ = shutdown => {
                //     return Ok(ConsensusWorkerStateEvent::ShutdownReceived)
                // }
            }
        }
        Ok(next_event_result)
    }

    async fn wait_for_new_view_messages(&self) -> HotStuffMessage {
        unimplemented!()
    }

    async fn process_leader_message(
        &self,
        message: &HotStuffMessage,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if message.message_type() != &HotStuffMessageType::NewView {
            return Ok(None);
        }
        let high_qc = self.find_highest_qc();
        let proposal = self.create_proposal();
        self.broadcast_proposal(proposal, high_qc);
        Ok(Some(ConsensusWorkerStateEvent::Prepared))
    }

    async fn process_replica_message(
        &self,
        message: &HotStuffMessage,
        current_view: &View,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if !message.matches(HotStuffMessageType::Prepare, current_view.view_id) {
            dbg!("Wrong message type received, log");
            return Ok(None);
        }
        if message.node().is_none() {
            unimplemented!("Empty message");
        }
        let node = message.node().unwrap();
        if self.does_extend(node, message.justify().node()) {
            if !self.is_safe_node(node, message.justify()) {
                unimplemented!("Node is not safe")
            }

            self.send_vote_to_leader(node);
            return Ok(Some(ConsensusWorkerStateEvent::Prepared));
        } else {
            unimplemented!("Did not extend from qc.justify.node")
        }
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
