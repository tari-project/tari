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

use std::collections::HashMap;

use log::*;
use tari_common_types::types::PublicKey;
use tari_crypto::tari_utilities::hex::Hex;
use tokio::time::{sleep, Duration};

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{Committee, HotStuffMessage, HotStuffMessageType, QuorumCertificate, View, ViewId},
    services::{
        infrastructure_services::{InboundConnectionService, OutboundService},
        PayloadProvider,
        ServiceSpecification,
    },
    storage::chain::ChainDbUnitOfWork,
    workers::states::ConsensusWorkerStateEvent,
};

const LOG_TARGET: &str = "tari::dan::workers::states::decide";

// TODO: This is very similar to pre-commit, and commit state
pub struct DecideState<TSpecification: ServiceSpecification> {
    node_id: TSpecification::Addr,
    asset_public_key: PublicKey,
    committee: Committee<TSpecification::Addr>,
    received_new_view_messages: HashMap<TSpecification::Addr, HotStuffMessage<TSpecification::Payload>>,
}

impl<TSpecification: ServiceSpecification> DecideState<TSpecification> {
    pub fn new(
        node_id: TSpecification::Addr,
        asset_public_key: PublicKey,
        committee: Committee<TSpecification::Addr>,
    ) -> Self {
        Self {
            node_id,
            asset_public_key,
            committee,
            received_new_view_messages: HashMap::new(),
        }
    }

    pub async fn next_event<TUnitOfWork: ChainDbUnitOfWork>(
        &mut self,
        timeout: Duration,
        current_view: &View,
        inbound_services: &mut TSpecification::InboundConnectionService,
        outbound_service: &mut TSpecification::OutboundService,
        mut unit_of_work: TUnitOfWork,
        payload_provider: &mut TSpecification::PayloadProvider,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        self.received_new_view_messages.clear();
        let timeout = sleep(timeout);
        futures::pin_mut!(timeout);
        loop {
            tokio::select! {
                r = inbound_services.wait_for_message(HotStuffMessageType::Commit, current_view.view_id()) => {
                    let (from, message) = r?;
                    if current_view.is_leader() {
                        if let Some(event) = self.process_leader_message(current_view, message.clone(), &from, outbound_service).await?{
                          break Ok(event);
                      }
                  }
                },
              r = inbound_services.wait_for_qc(HotStuffMessageType::Prepare, current_view.view_id()) => {
                    let (from, message) = r?;
                    let leader= self.committee.leader_for_view(current_view.view_id).clone();
                      if let Some(event) = self.process_replica_message(&message, current_view, &from, &leader, &mut unit_of_work, payload_provider).await? {
                          break Ok(event);
                      }
              },
              _ = &mut timeout =>  {
                    break Ok( ConsensusWorkerStateEvent::TimedOut);
                }
            }
        }
    }

    async fn process_leader_message(
        &mut self,
        current_view: &View,
        message: HotStuffMessage<TSpecification::Payload>,
        sender: &TSpecification::Addr,
        outbound: &mut TSpecification::OutboundService,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        if !message.matches(HotStuffMessageType::Commit, current_view.view_id) {
            return Ok(None);
        }

        if self.received_new_view_messages.contains_key(sender) {
            warn!(target: LOG_TARGET, "Already received message from {:?}", &sender);
            return Ok(None);
        }

        self.received_new_view_messages.insert(sender.clone(), message);

        if self.received_new_view_messages.len() >= self.committee.consensus_threshold() {
            debug!(
                target: LOG_TARGET,
                "[DECIDE] Consensus has been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                self.committee.len()
            );

            if let Some(qc) = self.create_qc(current_view) {
                self.broadcast(outbound, qc, current_view.view_id).await?;
                return Ok(None); // Replica will move this on
            }
            warn!(target: LOG_TARGET, "committee did not agree on node");
            Ok(None)
        } else {
            debug!(
                target: LOG_TARGET,
                "[DECIDE] Consensus has NOT YET been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                self.committee.len()
            );
            Ok(None)
        }
    }

    async fn broadcast(
        &self,
        outbound: &mut TSpecification::OutboundService,
        commit_qc: QuorumCertificate,
        view_number: ViewId,
    ) -> Result<(), DigitalAssetError> {
        let message = HotStuffMessage::decide(None, Some(commit_qc), view_number, self.asset_public_key.clone());
        outbound
            .broadcast(self.node_id.clone(), self.committee.members.as_slice(), message)
            .await
    }

    fn create_qc(&self, current_view: &View) -> Option<QuorumCertificate> {
        let mut node_hash = None;
        for message in self.received_new_view_messages.values() {
            node_hash = match node_hash {
                None => message.node_hash().copied(),
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
        let mut qc = QuorumCertificate::new(HotStuffMessageType::Commit, current_view.view_id, node_hash, None);
        for message in self.received_new_view_messages.values() {
            qc.combine_sig(message.partial_sig().unwrap())
        }
        Some(qc)
    }

    async fn process_replica_message<TUnitOfWork: ChainDbUnitOfWork>(
        &mut self,
        message: &HotStuffMessage<TSpecification::Payload>,
        current_view: &View,
        from: &TSpecification::Addr,
        view_leader: &TSpecification::Addr,
        unit_of_work: &mut TUnitOfWork,
        payload_provider: &mut TSpecification::PayloadProvider,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        debug!(target: LOG_TARGET, "[DECIDE] replica message: {:?}", message);
        if let Some(justify) = message.justify() {
            if !justify.matches(HotStuffMessageType::Commit, current_view.view_id) {
                warn!(
                    target: LOG_TARGET,
                    "Wrong justify message type received, log. {}, {:?}, {}",
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
                warn!(target: LOG_TARGET, "Message not from leader");
                return Ok(None);
            }

            payload_provider.remove_payload(justify.node_hash()).await?;
            unit_of_work.commit_node(justify.node_hash())?;
            info!(target: LOG_TARGET, "Committed node: {}", justify.node_hash().to_hex());
            Ok(Some(ConsensusWorkerStateEvent::Decided))
        } else {
            warn!(target: LOG_TARGET, "received non justify message");
            Ok(None)
        }
    }
}
