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

use log::*;
use tari_common_types::types::PublicKey;
use tari_shutdown::ShutdownSignal;
use tokio::time::Duration;

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{domain_events::ConsensusWorkerDomainEvent, AssetDefinition, ConsensusWorkerState, View, ViewId},
    services::{CheckpointManager, CommitteeManager, EventsPublisher, PayloadProvider, ServiceSpecification},
    storage::{
        chain::ChainDbUnitOfWork,
        state::{StateDbUnitOfWork, StateDbUnitOfWorkImpl, StateDbUnitOfWorkReader},
        DbFactory,
    },
    workers::{states, states::ConsensusWorkerStateEvent},
};

const LOG_TARGET: &str = "tari::dan::consensus_worker";

pub struct ConsensusWorker<TSpecification: ServiceSpecification> {
    inbound_connections: TSpecification::InboundConnectionService,
    outbound_service: TSpecification::OutboundService,
    state: ConsensusWorkerState,
    current_view_id: ViewId,
    committee_manager: TSpecification::CommitteeManager,
    timeout: Duration,
    node_address: TSpecification::Addr,
    payload_provider: TSpecification::PayloadProvider,
    events_publisher: TSpecification::EventsPublisher,
    signing_service: TSpecification::SigningService,
    payload_processor: TSpecification::PayloadProcessor,
    asset_definition: AssetDefinition,
    base_node_client: TSpecification::BaseNodeClient,
    db_factory: TSpecification::DbFactory,
    chain_storage_service: TSpecification::ChainStorageService,
    state_db_unit_of_work: Option<StateDbUnitOfWorkImpl<TSpecification::StateDbBackendAdapter>>,
    checkpoint_manager: TSpecification::CheckpointManager,
    validator_node_client_factory: TSpecification::ValidatorNodeClientFactory,
}

impl<TSpecification: ServiceSpecification<Addr = PublicKey>> ConsensusWorker<TSpecification> {
    pub fn new(
        inbound_connections: TSpecification::InboundConnectionService,
        outbound_service: TSpecification::OutboundService,
        committee_manager: TSpecification::CommitteeManager,
        node_id: TSpecification::Addr,
        payload_provider: TSpecification::PayloadProvider,
        events_publisher: TSpecification::EventsPublisher,
        signing_service: TSpecification::SigningService,
        payload_processor: TSpecification::PayloadProcessor,
        asset_definition: AssetDefinition,
        base_node_client: TSpecification::BaseNodeClient,
        timeout: Duration,
        db_factory: TSpecification::DbFactory,
        chain_storage_service: TSpecification::ChainStorageService,
        checkpoint_manager: TSpecification::CheckpointManager,
        validator_node_client_factory: TSpecification::ValidatorNodeClientFactory,
    ) -> Self {
        Self {
            inbound_connections,
            state: ConsensusWorkerState::Starting,
            current_view_id: ViewId(0),
            timeout,
            outbound_service,
            committee_manager,
            node_address: node_id,
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
            asset_definition,
            base_node_client,
            db_factory,
            chain_storage_service,
            state_db_unit_of_work: None,
            checkpoint_manager,
            validator_node_client_factory,
        }
    }

    fn get_current_view(&self) -> Result<View, DigitalAssetError> {
        Ok(View {
            view_id: self.current_view_id,
            is_leader: self
                .committee_manager
                .current_committee()?
                .leader_for_view(self.current_view_id) ==
                &self.node_address,
        })
    }

    pub async fn run(
        &mut self,
        shutdown: ShutdownSignal,
        max_views_to_process: Option<u64>,
    ) -> Result<(), DigitalAssetError> {
        let starting_view = self.current_view_id;
        loop {
            if let Some(max) = max_views_to_process {
                if max <= self.current_view_id.0 - starting_view.0 {
                    break;
                }
            }
            let next_event = self.next_state_event(&shutdown).await?;
            if next_event.must_shutdown() {
                info!(
                    target: LOG_TARGET,
                    "Consensus worker is shutting down because {}",
                    next_event.shutdown_reason().unwrap_or_default()
                );
                break;
            }
            let (from, to) = self.transition(next_event)?;
            debug!(target: LOG_TARGET, "Transitioning from {:?} to {:?}", from, to);

            self.events_publisher
                .publish(ConsensusWorkerDomainEvent::StateChanged { from, to });
        }

        Ok(())
    }

    async fn next_state_event(
        &mut self,
        shutdown: &ShutdownSignal,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        use ConsensusWorkerState::*;
        match &mut self.state {
            Starting => {
                states::Starting::default()
                    .next_event(
                        &mut self.base_node_client,
                        &self.asset_definition,
                        &mut self.committee_manager,
                        &self.db_factory,
                        &self.node_address,
                    )
                    .await
            },
            Synchronizing => {
                states::Synchronizing::new()
                    .next_event(
                        &mut self.base_node_client,
                        &self.asset_definition,
                        &self.db_factory,
                        &self.validator_node_client_factory,
                        &self.node_address,
                    )
                    .await
            },
            Prepare => {
                let db = self
                    .db_factory
                    .get_or_create_chain_db(&self.asset_definition.public_key)?;
                let mut unit_of_work = db.new_unit_of_work();
                let mut state_tx = self
                    .db_factory
                    .get_state_db(&self.asset_definition.public_key)?
                    .ok_or(DigitalAssetError::MissingDatabase)?
                    .new_unit_of_work(self.current_view_id.as_u64());

                let mut prepare =
                    states::Prepare::new(self.node_address.clone(), self.asset_definition.public_key.clone());
                let res = prepare
                    .next_event(
                        &self.get_current_view()?,
                        self.timeout,
                        &self.asset_definition,
                        self.committee_manager.current_committee()?,
                        &self.inbound_connections,
                        &mut self.outbound_service,
                        &mut self.payload_provider,
                        &self.signing_service,
                        &mut self.payload_processor,
                        &self.chain_storage_service,
                        unit_of_work.clone(),
                        &mut state_tx,
                        &self.db_factory,
                    )
                    .await?;
                // Will only be committed in DECIDE
                self.state_db_unit_of_work = Some(state_tx);
                unit_of_work.commit()?;
                Ok(res)
            },
            PreCommit => {
                let db = self
                    .db_factory
                    .get_or_create_chain_db(&self.asset_definition.public_key)?;
                let mut unit_of_work = db.new_unit_of_work();
                let mut state = states::PreCommitState::new(
                    self.node_address.clone(),
                    self.committee_manager.current_committee()?.clone(),
                    self.asset_definition.public_key.clone(),
                );
                let res = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view()?,
                        &self.inbound_connections,
                        &mut self.outbound_service,
                        &self.signing_service,
                        unit_of_work.clone(),
                    )
                    .await?;
                unit_of_work.commit()?;
                Ok(res)
            },

            Commit => {
                let db = self
                    .db_factory
                    .get_or_create_chain_db(&self.asset_definition.public_key)?;
                let mut unit_of_work = db.new_unit_of_work();
                let mut state = states::CommitState::new(
                    self.node_address.clone(),
                    self.asset_definition.public_key.clone(),
                    self.committee_manager.current_committee()?.clone(),
                );
                let res = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view()?,
                        &mut self.inbound_connections,
                        &mut self.outbound_service,
                        &self.signing_service,
                        unit_of_work.clone(),
                    )
                    .await?;

                unit_of_work.commit()?;

                Ok(res)
            },
            Decide => {
                let db = self
                    .db_factory
                    .get_or_create_chain_db(&self.asset_definition.public_key)?;
                let mut unit_of_work = db.new_unit_of_work();
                let mut state = states::DecideState::new(
                    self.node_address.clone(),
                    self.asset_definition.public_key.clone(),
                    self.committee_manager.current_committee()?.clone(),
                );
                let res = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view()?,
                        &mut self.inbound_connections,
                        &mut self.outbound_service,
                        unit_of_work.clone(),
                        &mut self.payload_provider,
                    )
                    .await?;
                unit_of_work.commit()?;
                if let Some(mut state_tx) = self.state_db_unit_of_work.take() {
                    state_tx.commit()?;
                    self.checkpoint_manager
                        .create_checkpoint(
                            state_tx.calculate_root()?,
                            self.committee_manager.current_committee()?.members.clone(),
                        )
                        .await?;
                } else {
                    // technically impossible
                    error!(target: LOG_TARGET, "No state unit of work was present");
                    return Err(DigitalAssetError::InvalidLogicPath {
                        reason: "Tried to commit state after DECIDE, but no state tx was present".to_string(),
                    });
                }

                Ok(res)
            },
            NextView => {
                info!(
                    target: LOG_TARGET,
                    "Status: {} in mempool ",
                    self.payload_provider.get_payload_queue().await,
                );
                self.state_db_unit_of_work = None;
                let mut state = states::NextViewState::default();
                state
                    .next_event(
                        &self.get_current_view()?,
                        &self.db_factory,
                        &mut self.outbound_service,
                        self.committee_manager.current_committee()?,
                        self.node_address.clone(),
                        &self.asset_definition,
                        &self.payload_provider,
                        shutdown,
                    )
                    .await
            },
            Idle => {
                info!(target: LOG_TARGET, "No work to do, idling");
                let state = states::IdleState::default();
                state.next_event().await
            },
        }
    }

    fn transition(
        &mut self,
        event: ConsensusWorkerStateEvent,
    ) -> Result<(ConsensusWorkerState, ConsensusWorkerState), DigitalAssetError> {
        use ConsensusWorkerState::*;
        use ConsensusWorkerStateEvent::*;
        let from = self.state;
        self.state = match (&self.state, event) {
            (Starting, Initialized) => Synchronizing,
            (Synchronizing, Synchronized) => NextView,
            (_, NotPartOfCommittee) => Idle,
            (Idle, TimedOut) => Starting,
            (_, TimedOut) => {
                warn!(target: LOG_TARGET, "State timed out");
                self.current_view_id = self.current_view_id.saturating_sub(1.into());
                NextView
            },
            (NextView, NewView { new_view }) => {
                self.current_view_id = new_view;
                Prepare
            },
            (Prepare, Prepared) => PreCommit,
            (PreCommit, PreCommitted) => Commit,
            (Commit, Committed) => Decide,
            (Decide, Decided) => NextView,
            (_, BaseLayerCheckpointNotFound | BaseLayerAssetRegistrationNotFound) => {
                unimplemented!("Base layer checkpoint not found!")
            },
            (s, e) => {
                dbg!(&s);
                dbg!(&e);
                unimplemented!("State machine transition not implemented")
            },
        };
        Ok((from, self.state))
    }
}

#[cfg(test)]
mod test {
    use tari_shutdown::Shutdown;
    use tokio::task::JoinHandle;

    use super::*;
    use crate::{
        models::{Committee, ConsensusWorkerState::*},
        services::{
            infrastructure_services::mocks::{mock_outbound, MockInboundConnectionService, MockOutboundService},
            mocks::{mock_events_publisher, MockCommitteeManager, MockEventsPublisher},
        },
    };

    fn start_replica(
        _inbound: MockInboundConnectionService<&'static str, &'static str>,
        _outbound: MockOutboundService<&'static str, &'static str>,
        _committee_manager: MockCommitteeManager,
        _node_id: &'static str,
        _shutdown_signal: ShutdownSignal,
        _events_publisher: MockEventsPublisher<ConsensusWorkerDomainEvent>,
    ) -> JoinHandle<()> {
        todo!()
        // let mut replica_a = ConsensusWorker::new(inbound, outbound, committee_manager, node_id,
        // mock_static_payload_provider("Hello"), events_publisher, mock_signing_service(), mock_payload_processor(),
        // AssetDefinition::default(), mock_base_node_client(), Duration::from_secs(5),
        // , ); tokio::spawn(async move {
        //     let _res = replica_a.run(shutdown_signal, Some(2)).await;
        // })
    }

    #[tokio::test]
    #[ignore]
    async fn test_simple_case() {
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();

        let committee = Committee::new(vec!["A", "B"]);
        let mut outbound = mock_outbound(committee.members.clone());
        let committee_manager = MockCommitteeManager { committee };

        let inbound_a = outbound.take_inbound(&"A").unwrap();
        let inbound_b = outbound.take_inbound(&"B").unwrap();
        // let inbound_c = outbound.take_inbound(&"C").unwrap();
        // let inbound_d = outbound.take_inbound(&"D").unwrap();

        let events = [
            mock_events_publisher(),
            mock_events_publisher(),
            mock_events_publisher(),
            mock_events_publisher(),
        ];

        let task_a = start_replica(
            inbound_a,
            outbound.clone(),
            committee_manager.clone(),
            "A",
            signal.clone(),
            events[0].clone(),
        );
        let task_b = start_replica(
            inbound_b,
            outbound.clone(),
            committee_manager,
            "B",
            signal.clone(),
            events[1].clone(),
        );
        // let task_c = start_replica(
        //     inbound_c,
        //     outbound.clone(),
        //     committee.clone(),
        //     "C",
        //     signal.clone(),
        //     events[2].clone(),
        // );
        // let task_d = start_replica(
        //     inbound_d,
        //     outbound.clone(),
        //     committee.clone(),
        //     "D",
        //     signal.clone(),
        //     events[3].clone(),
        // );
        shutdown.trigger();
        task_a.await.unwrap();
        task_b.await.unwrap();
        // task_c.await.unwrap();
        // task_d.await.unwrap();

        // assert_eq!(events[0].to_vec(), vec![ConsensusWorkerDomainEvent::StateChanged {
        //     old: Starting,
        // new: Prepare
        // }]);

        assert_state_change(&events[0].to_vec(), vec![
            Prepare, NextView, Prepare, PreCommit, Commit, Decide, NextView, Prepare, PreCommit, Commit, Decide,
            NextView,
        ]);
        assert_state_change(&events[1].to_vec(), vec![
            Prepare, NextView, Prepare, PreCommit, Commit, Decide, NextView, Prepare, PreCommit, Commit, Decide,
            NextView,
        ]);
    }

    fn assert_state_change(events: &[ConsensusWorkerDomainEvent], states: Vec<ConsensusWorkerState>) {
        dbg!(events);
        let mapped_events = events.iter().map(|e| match e {
            ConsensusWorkerDomainEvent::StateChanged { from: _, to: new } => Some(new),
        });
        for (state, event) in states.iter().zip(mapped_events) {
            assert_eq!(state, event.unwrap())
        }
    }
}
