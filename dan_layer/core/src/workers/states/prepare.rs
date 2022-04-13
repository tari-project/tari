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
use tokio::time::{sleep, Duration};

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{
        AssetDefinition,
        Committee,
        HotStuffMessage,
        HotStuffMessageType,
        HotStuffTreeNode,
        QuorumCertificate,
        TreeNodeHash,
        View,
        ViewId,
    },
    services::{
        infrastructure_services::{InboundConnectionService, OutboundService},
        PayloadProcessor,
        PayloadProvider,
        ServiceSpecification,
        SigningService,
    },
    storage::{chain::ChainDbUnitOfWork, state::StateDbUnitOfWork, ChainStorageService, DbFactory, StorageError},
    workers::states::ConsensusWorkerStateEvent,
};

const LOG_TARGET: &str = "tari::dan::workers::states::prepare";

pub struct Prepare<TSpecification: ServiceSpecification> {
    node_id: TSpecification::Addr,
    asset_public_key: PublicKey,
    received_new_view_messages: HashMap<TSpecification::Addr, HotStuffMessage<TSpecification::Payload>>,
}

impl<TSpecification: ServiceSpecification> Prepare<TSpecification> {
    pub fn new(node_id: TSpecification::Addr, asset_public_key: PublicKey) -> Self {
        Self {
            node_id,
            asset_public_key,
            received_new_view_messages: HashMap::new(),
        }
    }

    pub async fn next_event<TChainDbUnitOfWork: ChainDbUnitOfWork, TStateDbUnitOfWork: StateDbUnitOfWork>(
        &mut self,
        current_view: &View,
        timeout: Duration,
        asset_definition: &AssetDefinition,
        committee: &Committee<TSpecification::Addr>,
        inbound_services: &TSpecification::InboundConnectionService,
        outbound_service: &mut TSpecification::OutboundService,
        payload_provider: &mut TSpecification::PayloadProvider,
        signing_service: &TSpecification::SigningService,
        payload_processor: &mut TSpecification::PayloadProcessor,
        chain_storage_service: &TSpecification::ChainStorageService,
        mut chain_tx: TChainDbUnitOfWork,
        state_tx: &mut TStateDbUnitOfWork,
        db_factory: &TSpecification::DbFactory,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        self.received_new_view_messages.clear();
        let timeout = sleep(timeout);
        futures::pin_mut!(timeout);
        debug!(target: LOG_TARGET, "[PREPARE] Current view: {}", current_view);

        if current_view.is_leader() {
            debug!(
                target: LOG_TARGET,
                "Waiting for NewView (view_id = {}) messages",
                current_view.view_id()
            );
        } else {
            debug!(
                target: LOG_TARGET,
                "Waiting for Prepare (view_id = {}) messages",
                current_view.view_id()
            );
        }
        loop {
            tokio::select! {
                r = inbound_services.wait_for_message(HotStuffMessageType::NewView, current_view.view_id())  => {
                    let (from, message) = r?;
                    debug!(target: LOG_TARGET, "Received leader message (is_leader = {:?})", current_view.is_leader());
                    if current_view.is_leader() {
                        if let Some(event) = self.process_leader_message(
                            current_view,
                            message.clone(),
                            &from,
                            asset_definition,
                            committee,
                            payload_provider,
                            payload_processor,
                            outbound_service,
                            db_factory,
                        ).await? {
                            break Ok(event)
                        }
                    }
                },
                r = inbound_services.wait_for_message(HotStuffMessageType::Prepare, current_view.view_id()) => {
                    let (from, message) = r?;
                    debug!(target: LOG_TARGET, "Received replica message");
                    if let Some(event) = self.process_replica_message(
                        &message,
                        current_view,
                        &from,
                        committee.leader_for_view(current_view.view_id),
                        outbound_service,
                        signing_service,
                        payload_processor,
                        payload_provider,
                        &mut chain_tx,
                        chain_storage_service,
                        state_tx,
                    ).await? {
                        break Ok(event);
                    }

                },
                _ = &mut timeout => {
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
        asset_definition: &AssetDefinition,
        committee: &Committee<TSpecification::Addr>,
        payload_provider: &TSpecification::PayloadProvider,
        payload_processor: &mut TSpecification::PayloadProcessor,
        outbound: &mut TSpecification::OutboundService,
        db_factory: &TSpecification::DbFactory,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        debug!(
            target: LOG_TARGET,
            "Received message as leader:{:?} for view:{}",
            message.message_type(),
            message.view_number()
        );

        // TODO: This might need to be checked in the QC rather
        if self.received_new_view_messages.contains_key(sender) {
            println!("Already received message from {:?}", sender);
            return Ok(None);
        }

        self.received_new_view_messages.insert(sender.clone(), message);

        if self.received_new_view_messages.len() >= committee.consensus_threshold() {
            debug!(
                target: LOG_TARGET,
                "[PREPARE] Consensus has been reached with {:?} out of {} votes",
                self.received_new_view_messages.len(),
                committee.len()
            );
            let high_qc = self.find_highest_qc();

            let temp_state_tx = db_factory
                .get_or_create_state_db(&self.asset_public_key)?
                .new_unit_of_work(current_view.view_id.as_u64());
            let proposal = self
                .create_proposal(
                    *high_qc.node_hash(),
                    asset_definition,
                    payload_provider,
                    payload_processor,
                    current_view.view_id,
                    temp_state_tx,
                )
                .await?;
            self.broadcast_proposal(outbound, committee, proposal, high_qc, current_view.view_id)
                .await?;
            Ok(None) // Will move to pre-commit when it receives the message as a replica
        } else {
            debug!(
                target: LOG_TARGET,
                "[PREPARE] Consensus has NOT YET been reached with {} out of {} votes",
                self.received_new_view_messages.len(),
                committee.len()
            );
            Ok(None)
        }
    }

    async fn process_replica_message<TChainDbUnitOfWork: ChainDbUnitOfWork, TStateDbUnitOfWork: StateDbUnitOfWork>(
        &self,
        message: &HotStuffMessage<TSpecification::Payload>,
        current_view: &View,
        from: &TSpecification::Addr,
        view_leader: &TSpecification::Addr,
        outbound: &mut TSpecification::OutboundService,
        signing_service: &TSpecification::SigningService,
        payload_processor: &mut TSpecification::PayloadProcessor,
        payload_provider: &mut TSpecification::PayloadProvider,
        chain_tx: &mut TChainDbUnitOfWork,
        chain_storage_service: &TSpecification::ChainStorageService,
        state_tx: &mut TStateDbUnitOfWork,
    ) -> Result<Option<ConsensusWorkerStateEvent>, DigitalAssetError> {
        debug!(
            target: LOG_TARGET,
            "Received message as replica:{:?} for view:{}",
            message.message_type(),
            message.view_number()
        );
        if message.node().is_none() {
            unimplemented!("Empty message");
        }
        if from != view_leader {
            println!("Message not from leader");
            return Ok(None);
        }
        let node = message.node().unwrap();
        let justify = message
            .justify()
            .ok_or(DigitalAssetError::PreparePhaseNoQuorumCertificate)?;

        // The genesis does not extend any node
        if !current_view.view_id().is_genesis() {
            if !self.does_extend(node, justify.node_hash()) {
                return Err(DigitalAssetError::PreparePhaseCertificateDoesNotExtendNode);
            }

            if !self.is_safe_node(node, justify, chain_tx)? {
                return Err(DigitalAssetError::PreparePhaseNodeNotSafe);
            }
        }

        debug!(
            target: LOG_TARGET,
            "[PREPARE] Processing prepared payload for view {}",
            current_view.view_id()
        );

        let state_root = payload_processor
            .process_payload(node.payload(), state_tx.clone())
            .await?;

        if state_root != *node.state_root() {
            warn!(
                target: LOG_TARGET,
                "Calculated state root did not match the state root provided by the leader: Expected: {:?} Leader \
                 provided:{:?}",
                state_root,
                node.state_root()
            );
            return Ok(None);
        }

        debug!(
            target: LOG_TARGET,
            "[PREPARE] Merkle root matches payload for view {}. Adding node '{}'",
            current_view.view_id(),
            node.hash()
        );

        chain_storage_service
            .add_node::<TChainDbUnitOfWork>(node, chain_tx.clone())
            .await?;

        payload_provider.reserve_payload(node.payload(), node.hash()).await?;
        self.send_vote_to_leader(
            *node.hash(),
            outbound,
            view_leader,
            current_view.view_id,
            signing_service,
        )
        .await?;
        Ok(Some(ConsensusWorkerStateEvent::Prepared))
    }

    fn find_highest_qc(&self) -> QuorumCertificate {
        let mut max_qc = None;
        for message in self.received_new_view_messages.values() {
            match &max_qc {
                None => max_qc = message.justify().cloned(),
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

    async fn create_proposal<TStateDbUnitOfWork: StateDbUnitOfWork>(
        &self,
        parent: TreeNodeHash,
        asset_definition: &AssetDefinition,
        payload_provider: &TSpecification::PayloadProvider,
        payload_processor: &mut TSpecification::PayloadProcessor,
        view_id: ViewId,
        state_db: TStateDbUnitOfWork,
    ) -> Result<HotStuffTreeNode<TSpecification::Payload>, DigitalAssetError> {
        debug!(target: LOG_TARGET, "Creating new proposal for {}", view_id);

        // TODO: Artificial delay here to set the block time
        sleep(Duration::from_secs(10)).await;

        if view_id.is_genesis() {
            let payload = payload_provider.create_genesis_payload(asset_definition);
            let state_root = payload_processor.process_payload(&payload, state_db).await?;
            Ok(HotStuffTreeNode::genesis(payload, state_root))
        } else {
            let payload = payload_provider.create_payload().await?;

            let state_root = payload_processor.process_payload(&payload, state_db).await?;
            Ok(HotStuffTreeNode::from_parent(
                parent,
                payload,
                state_root,
                view_id.as_u64() as u32,
            ))
        }
    }

    async fn broadcast_proposal(
        &self,
        outbound: &mut TSpecification::OutboundService,
        committee: &Committee<TSpecification::Addr>,
        proposal: HotStuffTreeNode<TSpecification::Payload>,
        high_qc: QuorumCertificate,
        view_number: ViewId,
    ) -> Result<(), DigitalAssetError> {
        let message = HotStuffMessage::prepare(proposal, Some(high_qc), view_number, self.asset_public_key.clone());
        outbound
            .broadcast(self.node_id.clone(), committee.members.as_slice(), message)
            .await
    }

    fn does_extend(&self, node: &HotStuffTreeNode<TSpecification::Payload>, from: &TreeNodeHash) -> bool {
        from == node.parent()
    }

    fn is_safe_node<TUnitOfWork: ChainDbUnitOfWork>(
        &self,
        node: &HotStuffTreeNode<TSpecification::Payload>,
        quorum_certificate: &QuorumCertificate,
        chain_tx: &mut TUnitOfWork,
    ) -> Result<bool, StorageError> {
        let locked_qc = chain_tx.get_locked_qc()?;
        Ok(self.does_extend(node, locked_qc.node_hash()) || quorum_certificate.view_number() > locked_qc.view_number())
    }

    async fn send_vote_to_leader(
        &self,
        node: TreeNodeHash,
        outbound: &mut TSpecification::OutboundService,
        view_leader: &TSpecification::Addr,
        view_number: ViewId,
        signing_service: &TSpecification::SigningService,
    ) -> Result<(), DigitalAssetError> {
        // TODO: Only send node hash, not the full node
        let mut message = HotStuffMessage::vote_prepare(node, view_number, self.asset_public_key.clone());
        message.add_partial_sig(signing_service.sign(&self.node_id, &message.create_signature_challenge())?);
        outbound.send(self.node_id.clone(), view_leader.clone(), message).await
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    use crate::{
        models::{AssetDefinition, Committee, HotStuffMessage, QuorumCertificate, TreeNodeHash, View, ViewId},
        services::{
            infrastructure_services::{mocks::mock_outbound, OutboundService},
            mocks::{
                mock_payload_processor,
                mock_signing_service,
                mock_static_payload_provider,
                MockChainStorageService,
                MockServiceSpecification,
            },
        },
        storage::{mocks::MockDbFactory, DbFactory},
        workers::states::{ConsensusWorkerStateEvent, Prepare},
    };

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn basic_test_as_leader() {
        // let mut inbound = mock_inbound();
        // let mut sender = inbound.create_sender();
        let locked_qc = QuorumCertificate::genesis(TreeNodeHash::zero());
        let mut rng = rand::thread_rng();
        let (_, asset_public_key) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, address_a) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, address_b) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, address_c) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, address_d) = RistrettoPublicKey::random_keypair(&mut rng);

        let mut state = Prepare::<MockServiceSpecification>::new(address_b.clone(), asset_public_key.clone());
        let current_view = View {
            view_id: ViewId(1),
            is_leader: true,
        };
        let timeout = Duration::from_secs(10);
        let asset_definition = AssetDefinition::default();
        let committee = Committee::new(vec![
            address_a.clone(),
            address_b.clone(),
            address_c.clone(),
            address_d.clone(),
        ]);
        let mut outbound = mock_outbound(committee.members.clone());
        let mut outbound2 = outbound.clone();
        let inbound = outbound.take_inbound(&address_b).unwrap();
        let mut payload_provider = mock_static_payload_provider();
        let signing_service = mock_signing_service();
        let mut payload_processor = mock_payload_processor();
        let chain_storage_service = MockChainStorageService::default();
        let db_factory = MockDbFactory::default();
        let chain_db = db_factory.get_or_create_chain_db(&asset_public_key.clone()).unwrap();
        let chain_tx = chain_db.new_unit_of_work();
        let mut state_tx = db_factory
            .get_or_create_state_db(&asset_public_key)
            .unwrap()
            .new_unit_of_work(current_view.view_id.as_u64());

        let task = state.next_event(
            &current_view,
            timeout,
            &asset_definition,
            &committee,
            &inbound,
            &mut outbound,
            &mut payload_provider,
            &signing_service,
            &mut payload_processor,
            &chain_storage_service,
            chain_tx,
            &mut state_tx,
            &db_factory,
        );

        outbound2
            .send(
                address_a.clone(),
                address_b.clone(),
                HotStuffMessage::new_view(locked_qc.clone(), ViewId(0), asset_public_key.clone()),
            )
            .await
            .unwrap();

        outbound2
            .send(
                address_c.clone(),
                address_b.clone(),
                HotStuffMessage::new_view(locked_qc.clone(), ViewId(0), asset_public_key.clone()),
            )
            .await
            .unwrap();

        outbound2
            .send(
                address_d.clone(),
                address_b.clone(),
                HotStuffMessage::new_view(locked_qc.clone(), ViewId(0), asset_public_key.clone()),
            )
            .await
            .unwrap();

        let event = task.await.unwrap();
        assert_eq!(event, ConsensusWorkerStateEvent::Prepared);
    }
}
