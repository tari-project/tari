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
            Committee,
            ConsensusWorkerState,
            Payload,
            QuorumCertificate,
            View,
            ViewId,
        },
        services::{
            infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
            BftReplicaService,
            EventsPublisher,
            MempoolService,
            PayloadProcessor,
            PayloadProvider,
            SigningService,
        },
        workers::{
            states,
            states::{ConsensusWorkerStateEvent, Prepare, Starting},
        },
    },
    digital_assets_error::DigitalAssetError,
};
use log::*;
use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};
use tari_shutdown::ShutdownSignal;
use tokio::time::Duration;

const LOG_TARGET: &str = "tari::dan::consensus_worker";

pub struct ConsensusWorker<
    TBftReplicaService,
    TInboundConnectionService,
    TOutboundService,
    TAddr,
    TPayload,
    TPayloadProvider,
    TEventsPublisher,
    TSigningService,
    TPayloadProcessor,
> where
    TBftReplicaService: BftReplicaService,
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable + Clone + Send,
    TPayload: Payload,
    TPayloadProvider: PayloadProvider<TPayload>,
    TEventsPublisher: EventsPublisher<ConsensusWorkerDomainEvent>,
    TSigningService: SigningService<TAddr>,
    TPayloadProcessor: PayloadProcessor<TPayload>,
{
    bft_replica_service: TBftReplicaService,
    inbound_connections: TInboundConnectionService,
    outbound_service: TOutboundService,
    state: ConsensusWorkerState,
    current_view_id: ViewId,
    committee: Committee<TAddr>,
    timeout: Duration,
    node_id: TAddr,
    payload_provider: TPayloadProvider,
    prepare_qc: Arc<QuorumCertificate<TPayload>>,
    events_publisher: TEventsPublisher,
    locked_qc: Arc<QuorumCertificate<TPayload>>,
    signing_service: TSigningService,
    payload_processor: TPayloadProcessor,
}

impl<
        TBftReplicaService,
        TInboundConnectionService,
        TOutboundService,
        TAddr,
        TPayload,
        TPayloadProvider,
        TEventsPublisher,
        TSigningService,
        TPayloadProcessor,
    >
    ConsensusWorker<
        TBftReplicaService,
        TInboundConnectionService,
        TOutboundService,
        TAddr,
        TPayload,
        TPayloadProvider,
        TEventsPublisher,
        TSigningService,
        TPayloadProcessor,
    >
where
    TBftReplicaService: BftReplicaService,
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload> + 'static + Send + Sync,
    TOutboundService: OutboundService<TAddr, TPayload>,
    TAddr: NodeAddressable + Clone + Send + Sync,
    TPayload: Payload,
    TPayloadProvider: PayloadProvider<TPayload>,
    TEventsPublisher: EventsPublisher<ConsensusWorkerDomainEvent>,
    TSigningService: SigningService<TAddr>,
    TPayloadProcessor: PayloadProcessor<TPayload>,
{
    pub fn new(
        bft_replica_service: TBftReplicaService,
        inbound_connections: TInboundConnectionService,
        outbound_service: TOutboundService,
        committee: Committee<TAddr>,
        node_id: TAddr,
        payload_provider: TPayloadProvider,
        events_publisher: TEventsPublisher,
        signing_service: TSigningService,
        payload_processor: TPayloadProcessor,
        timeout: Duration,
    ) -> Self {
        let prepare_qc = Arc::new(QuorumCertificate::genesis(payload_provider.create_genesis_payload()));

        Self {
            bft_replica_service,
            inbound_connections,
            state: ConsensusWorkerState::Starting,
            current_view_id: ViewId(0),
            timeout,
            outbound_service,
            committee,
            node_id,
            locked_qc: prepare_qc.clone(),
            prepare_qc,
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
        }
    }

    fn get_current_view(&self) -> View {
        View {
            view_id: self.current_view_id,
            is_leader: self.committee.leader_for_view(self.current_view_id) == &self.node_id,
        }
    }

    pub async fn run(
        &mut self,
        shutdown: ShutdownSignal,
        max_views_to_process: Option<u64>,
    ) -> Result<(), DigitalAssetError> {
        use ConsensusWorkerState::*;

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
            Starting => states::Starting {}.next_event().await,
            Prepare => {
                let mut p = states::Prepare::new(self.node_id.clone(), self.locked_qc.clone());
                p.next_event(
                    &self.get_current_view(),
                    self.timeout,
                    &self.committee,
                    &mut self.inbound_connections,
                    &mut self.outbound_service,
                    &self.payload_provider,
                    &self.signing_service,
                )
                .await
            },
            PreCommit => {
                let mut state = states::PreCommitState::new(self.node_id.clone(), self.committee.clone());
                let (res, prepare_qc) = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view(),
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
                let mut state = states::CommitState::new(self.node_id.clone(), self.committee.clone());
                let (res, locked_qc) = state
                    .next_event(
                        self.timeout,
                        &self.get_current_view(),
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
                let mut state = states::DecideState::new(self.node_id.clone(), self.committee.clone());
                state
                    .next_event(
                        self.timeout,
                        &self.get_current_view(),
                        &mut self.inbound_connections,
                        &mut self.outbound_service,
                        &self.signing_service,
                        &mut self.payload_processor,
                    )
                    .await
            },
            NextView => {
                println!("Status: {} in mempool ", self.payload_provider.get_payload_queue(),);
                let mut state = states::NextViewState::new();
                state
                    .next_event(
                        &self.get_current_view(),
                        self.prepare_qc.as_ref().clone(),
                        &mut self.outbound_service,
                        &self.committee,
                        self.node_id.clone(),
                        shutdown,
                    )
                    .await
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
            (_, TimedOut) => NextView,
            (NextView, NewView { .. }) => {
                self.current_view_id = self.current_view_id.next();
                Prepare
            },
            (Prepare, Prepared) => PreCommit,
            (PreCommit, PreCommitted) => Commit,
            (Commit, Committed) => Decide,
            (Decide, Decided) => NextView,
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
    use crate::dan_layer::services::{
        infrastructure_services::mocks::mock_inbound,
        mocks::{mock_bft, mock_mempool},
    };

    use crate::dan_layer::services::{
        infrastructure_services::mocks::{mock_outbound, MockInboundConnectionService, MockOutboundService},
        mocks::{
            mock_events_publisher,
            mock_payload_processor,
            mock_signing_service,
            mock_static_payload_provider,
            MockEventsPublisher,
        },
    };
    use futures::task;
    use std::collections::HashMap;
    use tari_shutdown::Shutdown;
    use tokio::task::JoinHandle;

    fn start_replica(
        inbound: MockInboundConnectionService<&'static str, &'static str>,
        outbound: MockOutboundService<&'static str, &'static str>,
        committee: Committee<&'static str>,
        node_id: &'static str,
        shutdown_signal: ShutdownSignal,
        events_publisher: MockEventsPublisher<ConsensusWorkerDomainEvent>,
    ) -> JoinHandle<()> {
        let mut replica_a = ConsensusWorker::new(
            mock_bft(),
            inbound,
            outbound,
            committee,
            node_id,
            mock_static_payload_provider("Hello"),
            events_publisher,
            mock_signing_service(),
            mock_payload_processor(),
            Duration::from_secs(5),
        );
        tokio::spawn(async move {
            let res = replica_a.run(shutdown_signal, Some(2)).await;
        })
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_simple_case() {
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();

        let committee = Committee::new(vec!["A", "B"]);
        let mut outbound = mock_outbound(committee.members.clone());

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
            committee.clone(),
            "A",
            signal.clone(),
            events[0].clone(),
        );
        let task_b = start_replica(
            inbound_b,
            outbound.clone(),
            committee.clone(),
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
        let mapped_events = events.iter().filter_map(|e| match e {
            ConsensusWorkerDomainEvent::StateChanged { old, new } => Some(new),
            _ => None,
        });
        for (state, event) in states.iter().zip(mapped_events) {
            assert_eq!(state, event)
        }
    }
}
