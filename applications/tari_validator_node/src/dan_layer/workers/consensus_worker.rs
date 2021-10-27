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
            domain_events::ConsensusWorkerDomainEvent,
            AssetDefinition,
            ConsensusWorkerState,
            Payload,
            QuorumCertificate,
            View,
            ViewId,
        },
        services::{
            infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
            BaseNodeClient,
            CommitteeManager,
            EventsPublisher,
            PayloadProcessor,
            PayloadProvider,
            SigningService,
        },
        workers::{states, states::ConsensusWorkerStateEvent},
    },
    digital_assets_error::DigitalAssetError,
};
use log::*;
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;
use tokio::time::Duration;

const LOG_TARGET: &str = "tari::dan::consensus_worker";

pub struct ConsensusWorker<
    TInboundConnectionService,
    TOutboundService,
    TAddr,
    TPayload,
    TPayloadProvider,
    TEventsPublisher,
    TSigningService,
    TPayloadProcessor,
    TCommitteeManager,
    TBaseNodeClient,
> where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable + Clone + Send,
    TPayload: Payload,
    TPayloadProvider: PayloadProvider<TPayload>,
    TEventsPublisher: EventsPublisher<ConsensusWorkerDomainEvent>,
    TSigningService: SigningService<TAddr>,
    TPayloadProcessor: PayloadProcessor<TPayload>,
    TCommitteeManager: CommitteeManager<TAddr>,
    TBaseNodeClient: BaseNodeClient,
{
    inbound_connections: TInboundConnectionService,
    outbound_service: TOutboundService,
    state: ConsensusWorkerState,
    current_view_id: ViewId,
    committee_manager: TCommitteeManager,
    timeout: Duration,
    node_id: TAddr,
    payload_provider: TPayloadProvider,
    prepare_qc: Arc<QuorumCertificate<TPayload>>,
    events_publisher: TEventsPublisher,
    locked_qc: Arc<QuorumCertificate<TPayload>>,
    signing_service: TSigningService,
    payload_processor: TPayloadProcessor,
    asset_definition: AssetDefinition,
    base_node_client: TBaseNodeClient,
}

impl<
        TInboundConnectionService,
        TOutboundService,
        TAddr,
        TPayload,
        TPayloadProvider,
        TEventsPublisher,
        TSigningService,
        TPayloadProcessor,
        TCommitteeManager,
        TBaseNodeClient,
    >
    ConsensusWorker<
        TInboundConnectionService,
        TOutboundService,
        TAddr,
        TPayload,
        TPayloadProvider,
        TEventsPublisher,
        TSigningService,
        TPayloadProcessor,
        TCommitteeManager,
        TBaseNodeClient,
    >
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload> + 'static + Send + Sync,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable + Clone + Send + Sync,
    TPayload: Payload,
    TPayloadProvider: PayloadProvider<TPayload>,
    TEventsPublisher: EventsPublisher<ConsensusWorkerDomainEvent>,
    TSigningService: SigningService<TAddr>,
    TPayloadProcessor: PayloadProcessor<TPayload>,
    TCommitteeManager: CommitteeManager<TAddr>,
    TBaseNodeClient: BaseNodeClient,
{
    pub fn new(
        inbound_connections: TInboundConnectionService,
        outbound_service: TOutboundService,
        committee_manager: TCommitteeManager,
        node_id: TAddr,
        payload_provider: TPayloadProvider,
        events_publisher: TEventsPublisher,
        signing_service: TSigningService,
        payload_processor: TPayloadProcessor,
        // TODO: maybe make this more generic through a service
        asset_definition: AssetDefinition,
        base_node_client: TBaseNodeClient,
        timeout: Duration,
    ) -> Self {
        let prepare_qc = Arc::new(QuorumCertificate::genesis(payload_provider.create_genesis_payload()));

        Self {
            inbound_connections,
            state: ConsensusWorkerState::Starting,
            current_view_id: ViewId(0),
            timeout,
            outbound_service,
            committee_manager,
            node_id,
            locked_qc: prepare_qc.clone(),
            prepare_qc,
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
            asset_definition,
            base_node_client,
        }
    }

    fn get_current_view(&self) -> Result<View, DigitalAssetError> {
        Ok(View {
            view_id: self.current_view_id,
            is_leader: self
                .committee_manager
                .current_committee()?
                .leader_for_view(self.current_view_id) ==
                &self.node_id,
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
            let trns = self.transition(next_event)?;
            info!(target: LOG_TARGET, "Transitioning from {:?} to {:?}", trns.0, trns.1);

            self.events_publisher.publish(ConsensusWorkerDomainEvent::StateChanged {
                old: trns.0,
                new: trns.1,
            });
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
                states::Starting::new()
                    .next_event(
                        &mut self.base_node_client,
                        &self.asset_definition,
                        &mut self.committee_manager,
                        &self.node_id,
                    )
                    .await
            },
            Prepare => {
                let mut p = states::Prepare::new(self.node_id.clone(), self.locked_qc.clone());
                p.next_event(
                    &self.get_current_view()?,
                    self.timeout,
                    self.committee_manager.current_committee()?,
                    &mut self.inbound_connections,
                    &mut self.outbound_service,
                    &self.payload_provider,
                    &self.signing_service,
                    &mut self.payload_processor,
                )
                .await
            },
            PreCommit => {
                let mut state = states::PreCommitState::new(
                    self.node_id.clone(),
                    self.committee_manager.current_committee()?.clone(),
                );
                let (res, prepare_qc) = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view()?,
                        &mut self.inbound_connections,
                        &mut self.outbound_service,
                        &self.signing_service,
                    )
                    .await?;
                if let Some(prepare_qc) = prepare_qc {
                    self.prepare_qc = Arc::new(prepare_qc);
                }
                Ok(res)
            },

            Commit => {
                let mut state = states::CommitState::new(
                    self.node_id.clone(),
                    self.committee_manager.current_committee()?.clone(),
                );
                let (res, locked_qc) = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view()?,
                        &mut self.inbound_connections,
                        &mut self.outbound_service,
                        &self.signing_service,
                    )
                    .await?;
                if let Some(locked_qc) = locked_qc {
                    self.locked_qc = Arc::new(locked_qc);
                }
                Ok(res)
            },
            Decide => {
                let mut state = states::DecideState::new(
                    self.node_id.clone(),
                    self.committee_manager.current_committee()?.clone(),
                );
                state
                    .next_event(
                        self.timeout,
                        &self.get_current_view()?,
                        &mut self.inbound_connections,
                        &mut self.outbound_service,
                        &self.signing_service,
                    )
                    .await
            },
            NextView => {
                info!(
                    target: LOG_TARGET,
                    "Status: {} in mempool ",
                    self.payload_provider.get_payload_queue().await,
                );
                let mut state = states::NextViewState::new();
                state
                    .next_event(
                        &self.get_current_view()?,
                        self.prepare_qc.as_ref().clone(),
                        &mut self.outbound_service,
                        self.committee_manager.current_committee()?,
                        self.node_id.clone(),
                        shutdown,
                    )
                    .await
            },
            Idle => {
                info!(target: LOG_TARGET, "No work to do, idling");
                let state = states::IdleState::new();
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
            (Starting, Initialized) => Prepare,
            (_, NotPartOfCommittee) => Idle,
            (Idle, TimedOut) => Starting,
            (_, TimedOut) => NextView,
            (NextView, NewView { .. }) => {
                self.current_view_id = self.current_view_id.next();
                Prepare
            },
            (Prepare, Prepared) => PreCommit,
            (PreCommit, PreCommitted) => Commit,
            (Commit, Committed) => Decide,
            (Decide, Decided) => NextView,
            (Starting, BaseLayerCheckpointNotFound) => {
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
    use super::*;
    use crate::dan_layer::services::mocks::MockCommitteeManager;

    use crate::dan_layer::services::{
        infrastructure_services::mocks::{mock_outbound, MockInboundConnectionService, MockOutboundService},
        mocks::{
            mock_base_node_client,
            mock_events_publisher,
            mock_payload_processor,
            mock_signing_service,
            mock_static_payload_provider,
            MockEventsPublisher,
        },
    };

    use crate::dan_layer::models::Committee;
    use tari_shutdown::Shutdown;
    use tokio::task::JoinHandle;

    fn start_replica(
        inbound: MockInboundConnectionService<&'static str, &'static str>,
        outbound: MockOutboundService<&'static str, &'static str>,
        committee_manager: MockCommitteeManager,
        node_id: &'static str,
        shutdown_signal: ShutdownSignal,
        events_publisher: MockEventsPublisher<ConsensusWorkerDomainEvent>,
    ) -> JoinHandle<()> {
        let mut replica_a = ConsensusWorker::new(
            inbound,
            outbound,
            committee_manager,
            node_id,
            mock_static_payload_provider("Hello"),
            events_publisher,
            mock_signing_service(),
            mock_payload_processor(),
            AssetDefinition::default(),
            mock_base_node_client(),
            Duration::from_secs(5),
        );
        tokio::spawn(async move {
            let _res = replica_a.run(shutdown_signal, Some(2)).await;
        })
    }

    #[tokio::test]
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
        use crate::dan_layer::models::ConsensusWorkerState::*;
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
            ConsensusWorkerDomainEvent::StateChanged { old: _, new } => Some(new),
        });
        for (state, event) in states.iter().zip(mapped_events) {
            assert_eq!(state, event.unwrap())
        }
    }
}
