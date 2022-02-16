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

use std::{collections::HashMap, marker::PhantomData};

use log::*;
use tari_common_types::types::PublicKey;
use tokio::time::{sleep, Duration};

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{Committee, HotStuffMessage, HotStuffMessageType, QuorumCertificate, TreeNodeHash, View, ViewId},
    services::{
        infrastructure_services::{InboundConnectionService, OutboundService},
        SigningService,
    },
    storage::chain::ChainDbUnitOfWork,
    workers::states::ConsensusWorkerStateEvent,
};

const LOG_TARGET: &str = "tari::dan::workers::states::precommit";

pub struct PreCommitState<TInboundConnectionService, TOutboundService, TSigningService>
where TOutboundService: OutboundService
{
    node_id: TOutboundService::Addr,
    asset_public_key: PublicKey,
    committee: Committee<TOutboundService::Addr>,
    phantom_inbound: PhantomData<TInboundConnectionService>,
    phantom_outbound: PhantomData<TOutboundService>,
    ta: PhantomData<TOutboundService::Addr>,
    p_p: PhantomData<TOutboundService::Payload>,
    p_s: PhantomData<TSigningService>,
    received_prepare_messages: HashMap<TOutboundService::Addr, HotStuffMessage<TOutboundService::Payload>>,
}

impl<TInboundConnectionService, TOutboundService, TSigningService>
    PreCommitState<TInboundConnectionService, TOutboundService, TSigningService>
where
    TInboundConnectionService:
        InboundConnectionService<Addr = TOutboundService::Addr, Payload = TOutboundService::Payload>,
    TOutboundService: OutboundService,
    TSigningService: SigningService<TOutboundService::Addr>,
{
    pub fn new(
        node_id: TOutboundService::Addr,
        committee: Committee<TOutboundService::Addr>,
        asset_public_key: PublicKey,
    ) -> Self {
        Self {
            node_id,
            asset_public_key,
            committee,
            phantom_inbound: PhantomData,
            phantom_outbound: PhantomData,
            ta: PhantomData,
            p_p: PhantomData,
            received_prepare_messages: HashMap::new(),
            p_s: PhantomData,
        }
    }

    pub async fn next_event<TUnitOfWork: ChainDbUnitOfWork>(
        &mut self,
        timeout: Duration,
        current_view: &View,
        inbound_services: &TInboundConnectionService,
        outbound_service: &mut TOutboundService,
        signing_service: &TSigningService,
        unit_of_work: TUnitOfWork,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        self.received_prepare_messages.clear();
        let mut unit_of_work = unit_of_work;
        let next_event_result;
        let timeout = sleep(timeout);
        futures::pin_mut!(timeout);
        loop {
            tokio::select! {
                r = inbound_services.wait_for_message(HotStuffMessageType::Prepare, current_view.view_id()) => {
                    let (from, message) = r?;
                    debug!(target: LOG_TARGET, "Received message: {:?} view:{}",  message.message_type(), message.view_number());
                     if current_view.is_leader() {
                         if let Some(result) = self.process_leader_message(current_view, message.clone(), &from, outbound_service).await? {
                            next_event_result = result;
                            break;
                         }
                     }
                },
                r = inbound_services.wait_for_qc(HotStuffMessageType::Prepare, current_view.view_id()) => {
                   let (from, message) = r?;
                   let leader = self.committee.leader_for_view(current_view.view_id).clone();
                   if let Some(result) = self.process_replica_message(&message, current_view, &from, &leader,  outbound_service, signing_service, &mut unit_of_work).await? {
                       next_event_result = result;
                       break;
                   }
                },
                _ = &mut timeout =>  {
                      next_event_result = ConsensusWorkerStateEvent::TimedOut;
                      break;
                 }
            }
        }
        Ok(next_event_result)
    }

    async fn process_leader_message(
        &mut self,
        current_view: &View,
        message: HotStuffMessage<TOutboundService::Payload>,
        sender: &TOutboundService::Addr,
        outbound: &mut TOutboundService,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        debug!(
            target: LOG_TARGET,
            "Received message as leader:{:?} for view:{}",
            message.message_type(),
            message.view_number()
        );
        if !message.matches(HotStuffMessageType::Prepare, current_view.view_id) {
            return Ok(None);
        }

        if self.received_prepare_messages.contains_key(sender) {
            return Ok(None);
        }

        self.received_prepare_messages.insert(sender.clone(), message);

        if self.received_prepare_messages.len() >= self.committee.consensus_threshold() {
            debug!(
                target: LOG_TARGET,
                "[PRECOMMIT] Consensus has been reached with {:?} out of {} votes",
                self.received_prepare_messages.len(),
                self.committee.len()
            );

            if let Some(qc) = self.create_qc(current_view) {
                self.broadcast(outbound, &self.committee, qc, current_view.view_id)
                    .await?;
                // return Ok(Some(ConsensusWorkerStateEvent::PreCommitted));
                return Ok(None);
            }
            warn!(target: LOG_TARGET, "committee did not agree on node");
            Ok(None)
        } else {
            debug!(
                target: LOG_TARGET,
                "[PRECOMMIT] Consensus has NOT YET been reached with {:?} out of {} votes",
                self.received_prepare_messages.len(),
                self.committee.len()
            );
            Ok(None)
        }
    }

    async fn broadcast(
        &self,
        outbound: &mut TOutboundService,
        committee: &Committee<TOutboundService::Addr>,
        prepare_qc: QuorumCertificate,
        view_number: ViewId,
    ) -> Result<(), DigitalAssetError> {
        let message = HotStuffMessage::pre_commit(None, Some(prepare_qc), view_number, self.asset_public_key.clone());
        outbound
            .broadcast(self.node_id.clone(), committee.members.as_slice(), message)
            .await
    }

    fn create_qc(&self, current_view: &View) -> Option<QuorumCertificate> {
        let mut node_hash = None;
        for message in self.received_prepare_messages.values() {
            node_hash = match node_hash {
                None => message.node_hash().cloned(),
                Some(n) => {
                    if let Some(m_node) = message.node_hash() {
                        if &n != m_node {
                            unimplemented!("Nodes did not match");
                        }
                        Some(*m_node)
                    } else {
                        Some(n)
                    }
                },
            };
        }

        let node_hash = node_hash.unwrap();
        let mut qc = QuorumCertificate::new(HotStuffMessageType::Prepare, current_view.view_id, node_hash, None);
        for message in self.received_prepare_messages.values() {
            qc.combine_sig(message.partial_sig().unwrap())
        }
        Some(qc)
    }

    async fn process_replica_message<TUnitOfWork: ChainDbUnitOfWork>(
        &mut self,
        message: &HotStuffMessage<TOutboundService::Payload>,
        current_view: &View,
        from: &TOutboundService::Addr,
        view_leader: &TOutboundService::Addr,
        outbound: &mut TOutboundService,
        signing_service: &TSigningService,
        unit_of_work: &mut TUnitOfWork,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        debug!(
            target: LOG_TARGET,
            "Received message as replica:{:?} for view:{}",
            message.message_type(),
            message.view_number()
        );
        if let Some(justify) = message.justify() {
            if !justify.matches(HotStuffMessageType::Prepare, current_view.view_id) {
                warn!(
                    target: LOG_TARGET,
                    "Wrong justify message type received, log. {}, {:?}, {}",
                    &self.node_id,
                    &justify.message_type(),
                    current_view.view_id
                );
                return Ok(None);
            }

            if from != view_leader {
                warn!(target: LOG_TARGET, "Message not from leader");
                return Ok(None);
            }

            unit_of_work.set_prepare_qc(justify)?;
            self.send_vote_to_leader(
                *justify.node_hash(),
                outbound,
                view_leader,
                current_view.view_id,
                signing_service,
            )
            .await?;
            Ok(Some(ConsensusWorkerStateEvent::PreCommitted))
        } else {
            // dbg!("received non justify message");
            Ok(None)
        }
    }

    async fn send_vote_to_leader(
        &self,
        node: TreeNodeHash,
        outbound: &mut TOutboundService,
        view_leader: &TOutboundService::Addr,
        view_number: ViewId,
        signing_service: &TSigningService,
    ) -> Result<(), DigitalAssetError> {
        let mut message = HotStuffMessage::vote_pre_commit(node, view_number, self.asset_public_key.clone());
        message.add_partial_sig(signing_service.sign(&self.node_id, &message.create_signature_challenge())?);
        outbound.send(self.node_id.clone(), view_leader.clone(), message).await
    }
}
