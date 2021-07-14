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
        models::{
            Committee,
            HotStuffMessage,
            HotStuffMessageType,
            HotStuffTreeNode,
            Payload,
            QuorumCertificate,
            View,
            ViewId,
        },
        services::infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
        workers::states::ConsensusWorkerStateEvent,
    },
    digital_assets_error::DigitalAssetError,
};
use std::{collections::HashMap, marker::PhantomData};
use tokio::time::{delay_for, Duration};

pub struct PreCommitState<TAddr, TPayload, TInboundConnectionService, TOutboundService>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TPayload: Payload,
    TOutboundService: OutboundService<TAddr, TPayload>,
{
    node_id: TAddr,
    committee: Committee<TAddr>,
    phantom_inbound: PhantomData<TInboundConnectionService>,
    phantom_outbound: PhantomData<TOutboundService>,
    ta: PhantomData<TAddr>,
    p_p: PhantomData<TPayload>,

    received_new_view_messages: HashMap<TAddr, HotStuffMessage<TPayload>>,
    prepare_qc: Option<QuorumCertificate<TPayload>>,
}

impl<TAddr, TPayload, TInboundConnectionService, TOutboundService>
    PreCommitState<TAddr, TPayload, TInboundConnectionService, TOutboundService>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TPayload: Payload,
{
    pub fn new(node_id: TAddr, committee: Committee<TAddr>) -> Self {
        Self {
            node_id,
            committee,
            phantom_inbound: PhantomData,
            phantom_outbound: PhantomData,
            ta: PhantomData,
            p_p: PhantomData,
            received_new_view_messages: HashMap::new(),
            prepare_qc: None,
        }
    }

    pub async fn next_event(
        &mut self,
        timeout: Duration,
        current_view: &View,
        inbound_services: &mut TInboundConnectionService,
        outbound: &mut TOutboundService,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut next_event_result = ConsensusWorkerStateEvent::Errored {
            reason: "loop ended without setting this event".to_string(),
        };

        self.received_new_view_messages.clear();

        loop {
            tokio::select! {
                           (from, message) = self.wait_for_message(inbound_services) => {
                              dbg!("[PreCommit] Received message: ", &message.message_type(), &from);
            if current_view.is_leader() {
                                  if let Some(result) = self.process_leader_message(&current_view, message.clone(), from, outbound
                            ).await?{
                                     next_event_result = result;
                                      break;
                                  }

                              }
                              // if let Some(result) = self.process_replica_message(&message, &current_view, committee.leader_for_view(current_view.view_id),  outbound_service, &signing_service).await? {
                              //     next_event_result = result;
                              //     break;
                              // }

                              }
                      _ = delay_for(timeout) =>  {
                                    // TODO: perhaps this should be from the time the state was entered
                                    next_event_result = ConsensusWorkerStateEvent::TimedOut;
                                    break;
                                }
                            }
        }
        Ok(next_event_result)
    }

    async fn wait_for_message(
        &self,
        inbound_connection: &mut TInboundConnectionService,
    ) -> (TAddr, HotStuffMessage<TPayload>) {
        inbound_connection.receive_message().await
    }

    async fn process_leader_message(
        &mut self,
        current_view: &View,
        message: HotStuffMessage<TPayload>,
        sender: TAddr,
        outbound: &mut TOutboundService,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if !message.matches(HotStuffMessageType::Prepare, current_view.view_id) {
            return Ok(None);
        }

        // TODO: This might need to be checked in the QC rather
        if self.received_new_view_messages.contains_key(&sender) {
            dbg!("Already received message from {:?}", &sender);
            return Ok(None);
        }

        self.received_new_view_messages.insert(sender, message);

        if self.received_new_view_messages.len() >= self.committee.consensus_threshold() {
            dbg!(
                "Consensus has been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                self.committee.len()
            );

            if let Some(qc) = self.create_qc(&current_view) {
                self.prepare_qc = Some(qc.clone());
                self.broadcast(outbound, qc, current_view.view_id).await?;
                return Ok(Some(ConsensusWorkerStateEvent::PreCommitted));
            }
            dbg!("committee did not agree on node");
            return Ok(None);

            // let high_qc = self.find_highest_qc();
            // let proposal = self.create_proposal(high_qc.node(), payload_provider);
            // self.broadcast_proposal(outbound, proposal, high_qc, current_view.view_id)
            //     .await?;
            // Ok(Some(ConsensusWorkerStateEvent::Prepared))
        } else {
            dbg!(
                "Consensus has NOT YET been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                self.committee.len()
            );
            return Ok(None);
        }
    }

    async fn broadcast(
        &self,
        outbound: &mut TOutboundService,
        prepare_qc: QuorumCertificate<TPayload>,
        view_number: ViewId,
    ) -> Result<(), DigitalAssetError> {
        let message = HotStuffMessage::pre_commit(None, Some(prepare_qc), view_number);
        outbound.broadcast(self.node_id.clone(), message).await
    }

    fn create_qc(&self, current_view: &View) -> Option<QuorumCertificate<TPayload>> {
        // TODO: This can be done in one loop instead of two
        let mut node = None;
        for message in self.received_new_view_messages.values() {
            node = match node {
                None => message.node().map(|n| n.clone()),
                Some(n) => {
                    if let Some(m_node) = message.node() {
                        if &n != m_node {
                            unimplemented!("Nodes did not match");
                        }
                        Some(m_node.clone())
                    } else {
                        Some(n)
                    }
                },
            };
        }

        let node = node.unwrap();
        let mut qc = QuorumCertificate::new(HotStuffMessageType::Prepare, current_view.view_id, node);
        for message in self.received_new_view_messages.values() {
            qc.combine_sig(message.partial_sig().unwrap())
        }
        Some(qc)
    }
}
