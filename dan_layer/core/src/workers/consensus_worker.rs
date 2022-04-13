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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use log::*;
use tari_common_types::types::PublicKey;
use tari_shutdown::ShutdownSignal;
use tokio::time::Duration;

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{domain_events::ConsensusWorkerDomainEvent, AssetDefinition, ConsensusWorkerState, View, ViewId},
    services::{CheckpointManager, CommitteeManager, EventsPublisher, PayloadProvider, ServiceSpecification},
    storage::{
        chain::{ChainDb, ChainDbUnitOfWork},
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
        stop: Arc<AtomicBool>,
    ) -> Result<(), DigitalAssetError> {
        let chain_db = self
            .db_factory
            .get_or_create_chain_db(&self.asset_definition.public_key)?;
        self.current_view_id = chain_db
            .get_tip_node()?
            .map(|n| ViewId(u64::from(n.height())))
            .unwrap_or_else(|| ViewId(0));
        info!(
            target: LOG_TARGET,
            "Consensus worker started for asset '{}'. Tip: {}", self.asset_definition.public_key, self.current_view_id
        );
        let starting_view = self.current_view_id;
        while !stop.load(Ordering::Relaxed) {
            if let Some(max) = max_views_to_process {
                if max <= self.current_view_id.0 - starting_view.0 {
                    break;
                }
            }
            let mut processor = ConsensusWorkerProcessor {
                worker: self,
                chain_db: &chain_db,
                shutdown: &shutdown,
            };
            let next_event = processor.next_state_event().await?;
            if next_event.must_shutdown() {
                info!(
                    target: LOG_TARGET,
                    "Consensus worker is shutting down because {}",
                    next_event.shutdown_reason().unwrap_or_default()
                );
                break;
            }
            let (from, to) = self.transition(next_event)?;
            debug!(
                target: LOG_TARGET,
                "Transitioning from {:?} to {:?} ({})", from, to, self.current_view_id
            );

            self.events_publisher
                .publish(ConsensusWorkerDomainEvent::StateChanged { from, to });
        }

        Ok(())
    }
}

struct ConsensusWorkerProcessor<'a, T: ServiceSpecification> {
    worker: &'a mut ConsensusWorker<T>,
    chain_db: &'a ChainDb<T::ChainDbBackendAdapter>,
    shutdown: &'a ShutdownSignal,
}

impl<'a, T: ServiceSpecification<Addr = PublicKey>> ConsensusWorkerProcessor<'a, T> {
    async fn next_state_event(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        use ConsensusWorkerState::{Commit, Decide, Idle, NextView, PreCommit, Prepare, Starting, Synchronizing};
        match &mut self.worker.state {
            Starting => self.starting().await,
            Synchronizing => self.synchronizing().await,
            Prepare => self.prepare().await,
            PreCommit => self.pre_commit().await,
            Commit => self.commit().await,
            Decide => self.decide().await,
            NextView => self.next_view().await,
            Idle => self.idle().await,
        }
    }

    async fn starting(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        states::Starting::<T>::new()
            .next_event(
                &mut self.worker.base_node_client,
                &self.worker.asset_definition,
                &mut self.worker.committee_manager,
                &self.worker.db_factory,
                &self.worker.node_address,
            )
            .await
    }

    async fn synchronizing(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        states::Synchronizing::<T>::new()
            .next_event(
                &mut self.worker.base_node_client,
                &self.worker.asset_definition,
                &self.worker.db_factory,
                &self.worker.validator_node_client_factory,
                &self.worker.node_address,
            )
            .await
    }

    async fn prepare(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut unit_of_work = self.chain_db.new_unit_of_work();
        let mut state_tx = self
            .worker
            .db_factory
            .get_state_db(&self.worker.asset_definition.public_key)?
            .ok_or(DigitalAssetError::MissingDatabase)?
            .new_unit_of_work(self.worker.current_view_id.as_u64());

        let mut prepare = states::Prepare::<T>::new(
            self.worker.node_address.clone(),
            self.worker.asset_definition.public_key.clone(),
        );
        let res = prepare
            .next_event(
                &self.worker.get_current_view()?,
                self.worker.timeout,
                &self.worker.asset_definition,
                self.worker.committee_manager.current_committee()?,
                &self.worker.inbound_connections,
                &mut self.worker.outbound_service,
                &mut self.worker.payload_provider,
                &self.worker.signing_service,
                &mut self.worker.payload_processor,
                &self.worker.chain_storage_service,
                unit_of_work.clone(),
                &mut state_tx,
                &self.worker.db_factory,
            )
            .await?;
        // Will only be committed in DECIDE
        self.worker.state_db_unit_of_work = Some(state_tx);
        unit_of_work.commit()?;
        Ok(res)
    }

    async fn pre_commit(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut unit_of_work = self.chain_db.new_unit_of_work();
        let mut state = states::PreCommitState::<T>::new(
            self.worker.node_address.clone(),
            self.worker.committee_manager.current_committee()?.clone(),
            self.worker.asset_definition.public_key.clone(),
        );
        let res = state
            .next_event(
                self.worker.timeout,
                &self.worker.get_current_view()?,
                &self.worker.inbound_connections,
                &mut self.worker.outbound_service,
                &self.worker.signing_service,
                unit_of_work.clone(),
            )
            .await?;
        unit_of_work.commit()?;
        Ok(res)
    }

    async fn commit(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut unit_of_work = self.chain_db.new_unit_of_work();
        let mut state = states::CommitState::<T>::new(
            self.worker.node_address.clone(),
            self.worker.asset_definition.public_key.clone(),
            self.worker.committee_manager.current_committee()?.clone(),
        );
        let res = state
            .next_event(
                self.worker.timeout,
                &self.worker.get_current_view()?,
                &mut self.worker.inbound_connections,
                &mut self.worker.outbound_service,
                &self.worker.signing_service,
                unit_of_work.clone(),
            )
            .await?;
        unit_of_work.commit()?;
        Ok(res)
    }

    async fn decide(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut unit_of_work = self.chain_db.new_unit_of_work();
        let mut state = states::DecideState::<T>::new(
            self.worker.node_address.clone(),
            self.worker.asset_definition.public_key.clone(),
            self.worker.committee_manager.current_committee()?.clone(),
        );
        let res = state
            .next_event(
                self.worker.timeout,
                &self.worker.get_current_view()?,
                &mut self.worker.inbound_connections,
                &mut self.worker.outbound_service,
                unit_of_work.clone(),
                &mut self.worker.payload_provider,
            )
            .await?;
        unit_of_work.commit()?;
        if let Some(mut state_tx) = self.worker.state_db_unit_of_work.take() {
            state_tx.commit()?;
            self.worker
                .checkpoint_manager
                .create_checkpoint(
                    state_tx.calculate_root()?,
                    self.worker.committee_manager.current_committee()?.members.clone(),
                )
                .await?;
            Ok(res)
        } else {
            // technically impossible
            error!(target: LOG_TARGET, "No state unit of work was present");
            Err(DigitalAssetError::InvalidLogicPath {
                reason: "Tried to commit state after DECIDE, but no state tx was present".to_string(),
            })
        }
    }

    async fn next_view(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        info!(
            target: LOG_TARGET,
            "Status: {} in mempool ",
            self.worker.payload_provider.get_payload_queue().await,
        );
        self.worker.state_db_unit_of_work = None;
        let mut state = states::NextViewState::<T>::new();
        state
            .next_event(
                &self.worker.get_current_view()?,
                &self.worker.db_factory,
                &mut self.worker.outbound_service,
                self.worker.committee_manager.current_committee()?,
                self.worker.node_address.clone(),
                &self.worker.asset_definition,
                &self.worker.payload_provider,
                self.shutdown,
            )
            .await
    }

    async fn idle(&mut self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        info!(target: LOG_TARGET, "No work to do, idling");
        let state = states::IdleState::default();
        state.next_event().await
    }
}

impl<TSpecification: ServiceSpecification<Addr = PublicKey>> ConsensusWorker<TSpecification> {
    fn transition(
        &mut self,
        event: ConsensusWorkerStateEvent,
    ) -> Result<(ConsensusWorkerState, ConsensusWorkerState), DigitalAssetError> {
        use ConsensusWorkerState::{Commit, Decide, Idle, NextView, PreCommit, Prepare, Starting, Synchronizing};
        #[allow(clippy::enum_glob_use)]
        use ConsensusWorkerStateEvent::*;
        let from = self.state;
        self.state = match (&self.state, event) {
            (Starting, Initialized) => Synchronizing,
            (Synchronizing, Synchronized) => NextView,
            (_, NotPartOfCommittee) => Idle,
            (Idle, TimedOut) => Starting,
            (_, TimedOut) => {
                warn!(target: LOG_TARGET, "State timed out for {}", self.current_view_id);
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
                println!("{:?}", s);
                println!("{:?}", e);
                unimplemented!("State machine transition not implemented")
            },
        };
        Ok((from, self.state))
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::ristretto::RistrettoPublicKey;
    use tari_shutdown::Shutdown;
    use tokio::task::JoinHandle;

    use super::*;
    use crate::{
        models::{
            Committee,
            ConsensusWorkerState::{Commit, Decide, NextView, PreCommit, Prepare},
            TariDanPayload,
        },
        services::{
            infrastructure_services::mocks::{mock_outbound, MockInboundConnectionService, MockOutboundService},
            mocks::{
                create_public_key,
                mock_base_node_client,
                mock_checkpoint_manager,
                mock_events_publisher,
                mock_payload_processor,
                mock_signing_service,
                mock_static_payload_provider,
                MockChainStorageService,
                MockCommitteeManager,
                MockEventsPublisher,
                MockServiceSpecification,
                MockValidatorNodeClientFactory,
            },
        },
        storage::mocks::MockDbFactory,
    };

    fn start_replica(
        inbound: MockInboundConnectionService<RistrettoPublicKey, TariDanPayload>,
        outbound: MockOutboundService<RistrettoPublicKey, TariDanPayload>,
        committee_manager: MockCommitteeManager,
        node_id: RistrettoPublicKey,
        shutdown_signal: ShutdownSignal,
        events_publisher: MockEventsPublisher<ConsensusWorkerDomainEvent>,
    ) -> JoinHandle<()> {
        let payload_provider = mock_static_payload_provider();
        let signing_service = mock_signing_service();
        let payload_processor = mock_payload_processor();
        let asset_definition = AssetDefinition::default();
        let base_node_client = mock_base_node_client();
        let timeout = Duration::from_secs(5);
        let db_factory = MockDbFactory::default();
        let chain_storage_service = MockChainStorageService::default();
        let checkpoint_manager = mock_checkpoint_manager();
        let validator_node_client_factory = MockValidatorNodeClientFactory::default();
        let mut replica_a = ConsensusWorker::<MockServiceSpecification>::new(
            inbound,
            outbound,
            committee_manager,
            node_id,
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
            asset_definition,
            base_node_client,
            timeout,
            db_factory,
            chain_storage_service,
            checkpoint_manager,
            validator_node_client_factory,
        );
        tokio::spawn(async move {
            let max_views_to_process = Some(2);
            let stop = Arc::new(AtomicBool::default());
            let _res = replica_a.run(shutdown_signal, max_views_to_process, stop).await;
        })
    }

    #[tokio::test]
    #[ignore]
    async fn test_simple_case() {
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();

        let address_a = create_public_key();
        let address_b = create_public_key();

        let committee = Committee::new(vec![address_a.clone(), address_b.clone()]);
        let mut outbound = mock_outbound(committee.members.clone());
        let committee_manager = MockCommitteeManager { committee };

        let inbound_a = outbound.take_inbound(&address_a.clone()).unwrap();
        let inbound_b = outbound.take_inbound(&address_b.clone()).unwrap();
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
            address_a,
            signal.clone(),
            events[0].clone(),
        );
        let task_b = start_replica(
            inbound_b,
            outbound.clone(),
            committee_manager,
            address_b,
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
        println!("{:?}", events);
        let mapped_events = events.iter().map(|e| match e {
            ConsensusWorkerDomainEvent::StateChanged { from: _, to: new } => Some(new),
        });
        for (state, event) in states.iter().zip(mapped_events) {
            assert_eq!(state, event.unwrap())
        }
    }
}
