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
        services::{
            infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
            PayloadProcessor,
            SigningService,
        },
        workers::states::ConsensusWorkerStateEvent,
    },
    digital_assets_error::DigitalAssetError,
};
use std::{collections::HashMap, marker::PhantomData, time::Instant};
use tokio::time::{sleep, Duration};

// TODO: This is very similar to pre-commit, and commit state
pub struct DecideState<TAddr, TPayload, TInboundConnectionService, TOutboundService, TSigningService>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TPayload: Payload,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TSigningService: SigningService<TAddr>,
{
    node_id: TAddr,
    committee: Committee<TAddr>,
    phantom_inbound: PhantomData<TInboundConnectionService>,
    phantom_outbound: PhantomData<TOutboundService>,
    ta: PhantomData<TAddr>,
    p_p: PhantomData<TPayload>,
    p_s: PhantomData<TSigningService>,
    received_new_view_messages: HashMap<TAddr, HotStuffMessage<TPayload>>,
    commit_qc: Option<QuorumCertificate<TPayload>>,
    _locked_qc: Option<QuorumCertificate<TPayload>>,
}

impl<TAddr, TPayload, TInboundConnectionService, TOutboundService, TSigningService>
    DecideState<TAddr, TPayload, TInboundConnectionService, TOutboundService, TSigningService>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TPayload: Payload,
    TSigningService: SigningService<TAddr>,
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
            commit_qc: None,
            _locked_qc: None,
            p_s: PhantomData,
        }
    }

    pub async fn next_event(
        &mut self,
        timeout: Duration,
        current_view: &View,
        inbound_services: &mut TInboundConnectionService,
        outbound_service: &mut TOutboundService,
        _signing_service: &TSigningService,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut next_event_result = ConsensusWorkerStateEvent::Errored {
            reason: "loop ended without setting this event".to_string(),
        };
        dbg!(next_event_result);

        self.received_new_view_messages.clear();
        let started = Instant::now();
        loop {
            tokio::select! {
                           (from, message) = self.wait_for_message(inbound_services) => {
            if current_view.is_leader() {
                                  if let Some(result) = self.process_leader_message(current_view, message.clone(), &from, outbound_service
                            ).await?{
                                     next_event_result = result;
                                      break;
                                  }

                              }
                    let leader= self.committee.leader_for_view(current_view.view_id).clone();
                              if let Some(result) = self.process_replica_message(&message, current_view, &from, &leader).await? {
                                  next_event_result = result;
                                  break;
                              }

                              }
                      _ = sleep(timeout.saturating_sub(Instant::now() - started)) =>  {
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
        sender: &TAddr,
        outbound: &mut TOutboundService,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if !message.matches(HotStuffMessageType::Commit, current_view.view_id) {
            return Ok(None);
        }

        if self.received_new_view_messages.contains_key(sender) {
            dbg!("Already received message from {:?}", &sender);
            return Ok(None);
        }

        self.received_new_view_messages.insert(sender.clone(), message);

        if self.received_new_view_messages.len() >= self.committee.consensus_threshold() {
            println!(
                "[DECIDE] Consensus has been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                self.committee.len()
            );

            if let Some(qc) = self.create_qc(current_view) {
                self.commit_qc = Some(qc.clone());
                self.broadcast(outbound, qc, current_view.view_id).await?;
                return Ok(None); // Replica will move this on
            }
            dbg!("committee did not agree on node");
            Ok(None)
        } else {
            println!(
                "[DECIDE] Consensus has NOT YET been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                self.committee.len()
            );
            Ok(None)
        }
    }

    async fn broadcast(
        &self,
        outbound: &mut TOutboundService,
        commit_qc: QuorumCertificate<TPayload>,
        view_number: ViewId,
    ) -> Result<(), DigitalAssetError> {
        let message = HotStuffMessage::decide(None, Some(commit_qc), view_number);
        outbound
            .broadcast(self.node_id.clone(), self.committee.members.as_slice(), message)
            .await
    }

    fn create_qc(&self, current_view: &View) -> Option<QuorumCertificate<TPayload>> {
        let mut node = None;
        for message in self.received_new_view_messages.values() {
            node = match node {
                None => message.node().cloned(),
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
        let mut qc = QuorumCertificate::new(HotStuffMessageType::Commit, current_view.view_id, node, None);
        for message in self.received_new_view_messages.values() {
            qc.combine_sig(message.partial_sig().unwrap())
        }
        Some(qc)
    }

    async fn process_replica_message(
        &mut self,
        message: &HotStuffMessage<TPayload>,
        current_view: &View,
        from: &TAddr,
        view_leader: &TAddr,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if let Some(justify) = message.justify() {
            if !justify.matches(HotStuffMessageType::Commit, current_view.view_id) {
                dbg!(
                    "Wrong justify message type received, log",
                    &self.node_id,
                    &justify.message_type(),
                    current_view.view_id
                );
                return Ok(None);
            }
            // if message.node().is_none() {
            //     unimplemented!("Empty message");
            // }

            if from != view_leader {
                dbg!("Message not from leader");
                return Ok(None);
            }

            // self.locked_qc = Some(justify.clone());
            // self.send_vote_to_leader(
            //     justify.node(),
            //     outbound,
            //     view_leader,
            //     current_view.view_id,
            //     &signing_service,
            // )
            // .await?;

            Ok(Some(ConsensusWorkerStateEvent::Decided))
        } else {
            dbg!("received non justify message");
            Ok(None)
        }
    }

    async fn _send_vote_to_leader(
        &self,
        node: &HotStuffTreeNode<TPayload>,
        outbound: &mut TOutboundService,
        view_leader: &TAddr,
        view_number: ViewId,
        signing_service: &TSigningService,
    ) -> Result<(), DigitalAssetError> {
        let mut message = HotStuffMessage::commit(Some(node.clone()), None, view_number);
        message.add_partial_sig(signing_service.sign(&self.node_id, &message.create_signature_challenge())?);
        outbound.send(self.node_id.clone(), view_leader.clone(), message).await
    }
}
