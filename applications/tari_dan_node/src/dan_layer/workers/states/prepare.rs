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
            Block,
            Committee,
            HotStuffMessage,
            HotStuffMessageType,
            HotStuffTreeNode,
            Instruction,
            Payload,
            QuorumCertificate,
            View,
            ViewId,
        },
        services::{
            infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
            BftReplicaService,
            MempoolService,
            PayloadProvider,
            SigningService,
        },
        workers::states::ConsensusWorkerStateEvent,
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;
use futures::StreamExt;
use std::{
    any::Any,
    collections::HashMap,
    hash::Hash,
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::Instant,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::time::{delay_for, Duration};

pub struct Prepare<TInboundConnectionService, TOutboundService, TAddr, TSigningService, TPayloadProvider, TPayload>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload> + Send,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TSigningService: SigningService<TAddr>,
    TPayload: Payload,
    TPayloadProvider: PayloadProvider<TPayload>,
{
    node_id: TAddr,
    locked_qc: Arc<QuorumCertificate<TPayload>>,
    // bft_service: Box<dyn BftReplicaService>,
    // TODO remove this hack
    phantom: PhantomData<TInboundConnectionService>,
    phantom_payload_provider: PhantomData<TPayloadProvider>,
    phantom_outbound: PhantomData<TOutboundService>,
    phantom_signing: PhantomData<TSigningService>,
    received_new_view_messages: HashMap<TAddr, HotStuffMessage<TPayload>>,
}

impl<TInboundConnectionService, TOutboundService, TAddr, TSigningService, TPayloadProvider, TPayload>
    Prepare<TInboundConnectionService, TOutboundService, TAddr, TSigningService, TPayloadProvider, TPayload>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload> + Send,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TSigningService: SigningService<TAddr>,
    TPayload: Payload,
    TPayloadProvider: PayloadProvider<TPayload>,
{
    pub fn new(node_id: TAddr, locked_qc: Arc<QuorumCertificate<TPayload>>) -> Self {
        Self {
            node_id,
            locked_qc,
            phantom: PhantomData,
            phantom_payload_provider: PhantomData,
            phantom_outbound: PhantomData,
            phantom_signing: PhantomData,
            received_new_view_messages: HashMap::new(),
        }
    }

    pub async fn next_event(
        &mut self,
        current_view: &View,
        timeout: Duration,
        committee: &Committee<TAddr>,
        inbound_services: &mut TInboundConnectionService,
        outbound_service: &mut TOutboundService,
        payload_provider: &TPayloadProvider,
        signing_service: &TSigningService,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        self.received_new_view_messages.clear();

        let mut next_event_result = ConsensusWorkerStateEvent::Errored {
            reason: "loop ended without setting this event".to_string(),
        };

        // TODO: rather change the loop below to inside the wait for message
        let started = Instant::now();

        loop {
            tokio::select! {
                (from, message) = self.wait_for_message(inbound_services) => {
                    if current_view.is_leader() {
                        if let Some(result) = self.process_leader_message(&current_view, message.clone(), &from, &committee, &payload_provider, outbound_service).await?{
                           next_event_result = result;
                            break;
                        }

                    }
                    if let Some(result) = self.process_replica_message(&message, &current_view, &from,  committee.leader_for_view(current_view.view_id),  outbound_service, &signing_service).await? {
                        next_event_result = result;
                        break;
                    }

                },
                _ = delay_for(timeout.saturating_sub(Instant::now() - started)) =>  {
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
        committee: &Committee<TAddr>,
        payload_provider: &TPayloadProvider,
        outbound: &mut TOutboundService,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if message.message_type() != &HotStuffMessageType::NewView {
            return Ok(None);
        }

        // TODO: This might need to be checked in the QC rather
        if self.received_new_view_messages.contains_key(&sender) {
            dbg!("Already received message from {:?}", &sender);
            return Ok(None);
        }

        self.received_new_view_messages.insert(sender.clone(), message);

        if self.received_new_view_messages.len() >= committee.consensus_threshold() {
            println!(
                "[PREPARE] Consensus has been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                committee.len()
            );
            let high_qc = self.find_highest_qc();
            let proposal = self.create_proposal(high_qc.node(), payload_provider).await?;
            self.broadcast_proposal(outbound, &committee, proposal, high_qc, current_view.view_id)
                .await?;
            // Ok(Some(ConsensusWorkerStateEvent::Prepared))
            Ok(None) // Will move to pre-commit when it receives the message as a replica
        } else {
            println!(
                "[PREPARE] Consensus has NOT YET been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                committee.len()
            );
            return Ok(None);
        }
    }

    async fn process_replica_message(
        &self,
        message: &HotStuffMessage<TPayload>,
        current_view: &View,
        from: &TAddr,
        view_leader: &TAddr,
        outbound: &mut TOutboundService,
        signing_service: &TSigningService,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if !message.matches(HotStuffMessageType::Prepare, current_view.view_id) {
            // println!(
            //     "Wrong message type received, log. {:?} {:?} View {:?}",
            //     &self.node_id,
            //     &message.message_type(),
            //     current_view.view_id
            // );
            return Ok(None);
        }
        if message.node().is_none() {
            unimplemented!("Empty message");
        }
        if from != view_leader {
            dbg!("Message not from leader");
            return Ok(None);
        }
        let node = message.node().unwrap();
        if let Some(justify) = message.justify() {
            if self.does_extend(node, justify.node()) {
                if !self.is_safe_node(node, justify) {
                    unimplemented!("Node is not safe")
                }

                self.send_vote_to_leader(node, outbound, view_leader, current_view.view_id, &signing_service)
                    .await?;
                return Ok(Some(ConsensusWorkerStateEvent::Prepared));
            } else {
                unimplemented!("Did not extend from qc.justify.node")
            }
        } else {
            unimplemented!("unexpected Null justify ")
        }
    }

    fn find_highest_qc(&self) -> QuorumCertificate<TPayload> {
        let mut max_qc = None;
        for (sender, message) in &self.received_new_view_messages {
            match &max_qc {
                None => max_qc = message.justify().map(|qc| qc.clone()),
                Some(qc) => {
                    if let Some(justify) = message.justify() {
                        if qc.view_number() < justify.view_number() {
                            max_qc = Some(justify.clone())
                        }
                    }
                },
            }
        }
        // TODO: this will panic if nothing found
        max_qc.unwrap()
    }

    async fn create_proposal(
        &self,
        parent: &HotStuffTreeNode<TPayload>,
        payload_provider: &TPayloadProvider,
    ) -> Result<HotStuffTreeNode<TPayload>, DigitalAssetError> {
        // TODO: Artificial delay here to set the block time
        delay_for(Duration::from_secs(3)).await;

        let payload = payload_provider.create_payload()?;
        dbg!(&payload);
        Ok(HotStuffTreeNode::from_parent(parent, payload))
    }

    async fn broadcast_proposal(
        &self,
        outbound: &mut TOutboundService,
        committee: &Committee<TAddr>,
        proposal: HotStuffTreeNode<TPayload>,
        high_qc: QuorumCertificate<TPayload>,
        view_number: ViewId,
    ) -> Result<(), DigitalAssetError> {
        let message = HotStuffMessage::prepare(proposal, Some(high_qc), view_number);
        outbound
            .broadcast(self.node_id.clone(), committee.members.as_slice(), message)
            .await
    }

    fn does_extend(&self, node: &HotStuffTreeNode<TPayload>, from: &HotStuffTreeNode<TPayload>) -> bool {
        &from.calculate_hash() == node.parent()
    }

    fn is_safe_node(
        &self,
        node: &HotStuffTreeNode<TPayload>,
        quorum_certificate: &QuorumCertificate<TPayload>,
    ) -> bool {
        self.does_extend(node, self.locked_qc.node()) || quorum_certificate.view_number() > self.locked_qc.view_number()
    }

    async fn send_vote_to_leader(
        &self,
        node: &HotStuffTreeNode<TPayload>,
        outbound: &mut TOutboundService,
        view_leader: &TAddr,
        view_number: ViewId,
        signing_service: &TSigningService,
    ) -> Result<(), DigitalAssetError> {
        let mut message = HotStuffMessage::prepare(node.clone(), None, view_number);
        message.add_partial_sig(signing_service.sign(&self.node_id, &message.create_signature_challenge())?);
        outbound.send(self.node_id.clone(), view_leader.clone(), message).await
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::dan_layer::{
        models::ViewId,
        services::{
            infrastructure_services::mocks::{mock_inbound, mock_outbound},
            mocks::{mock_payload_provider, mock_signing_service},
        },
    };
    use tokio::time::Duration;

    #[tokio::test(threaded_scheduler)]
    async fn basic_test_as_leader() {
        // let mut inbound = mock_inbound();
        // let mut sender = inbound.create_sender();
        let locked_qc = QuorumCertificate::genesis("Hello world");
        let mut state = Prepare::new("B", Arc::new(locked_qc));
        let view = View {
            view_id: ViewId(1),
            is_leader: true,
        };
        let committee = Committee::new(vec!["A", "B", "C", "D"]);
        let mut outbound = mock_outbound(committee.members.clone());
        let mut outbound2 = outbound.clone();
        let mut inbound = outbound.take_inbound(&"B").unwrap();
        let payload_provider = mock_payload_provider();
        let signing = mock_signing_service();
        let task = state.next_event(
            &view,
            Duration::from_secs(10),
            &committee,
            &mut inbound,
            &mut outbound,
            &payload_provider,
            &signing,
        );
        outbound2
            .send(
                "A",
                "B",
                HotStuffMessage::new_view(QuorumCertificate::genesis("empty"), ViewId(0)),
            )
            .await;
        outbound2
            .send(
                "C",
                "B",
                HotStuffMessage::new_view(QuorumCertificate::genesis("empty"), ViewId(0)),
            )
            .await;
        outbound2
            .send(
                "D",
                "B",
                HotStuffMessage::new_view(QuorumCertificate::genesis("empty"), ViewId(0)),
            )
            .await;
        let event = task.await.unwrap();
        assert_eq!(event, ConsensusWorkerStateEvent::Prepared);
    }
}
