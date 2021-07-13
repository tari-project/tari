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
        models::{Committee, QuorumCertificate, View, ViewId},
        services::{
            infrastructure_services::{InboundConnectionService, NodeAddressable, OutboundService},
            BftReplicaService,
            MempoolService,
        },
        workers::{
            states,
            states::{ConsensusWorkerStateEvent, Prepare, Starting},
        },
    },
    digital_assets_error::DigitalAssetError,
};
use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::time::Duration;

const LOG_TARGET: &str = "tari::dan::consensus_worker";

pub struct ConsensusWorker<TMempoolService, TBftReplicaService, TInboundConnectionService, TOutboundService, TAddr>
where
    TMempoolService: MempoolService,
    TBftReplicaService: BftReplicaService,
    TInboundConnectionService: InboundConnectionService + Clone,
    TOutboundService: OutboundService<TAddr>,
    TAddr: NodeAddressable + Clone + Send,
{
    mempool_service: TMempoolService,
    bft_replica_service: TBftReplicaService,
    inbound_connections: TInboundConnectionService,
    outbound_service: TOutboundService,
    state: ConsensusWorkerState,
    current_view_id: ViewId,
    committee: Committee<TAddr>,
    timeout: Duration,
    node_id: TAddr,
}

#[derive(Debug, Clone, Copy)]
pub enum ConsensusWorkerState {
    Starting,
    Prepare,
    NextView,
}

impl<TMempoolService, TBftReplicaService, TInboundConnectionService, TOutboundService, TAddr>
    ConsensusWorker<TMempoolService, TBftReplicaService, TInboundConnectionService, TOutboundService, TAddr>
where
    TMempoolService: MempoolService,
    TBftReplicaService: BftReplicaService,
    TInboundConnectionService: InboundConnectionService + Clone + 'static + Send + Sync,
    TOutboundService: OutboundService<TAddr>,
    TAddr: NodeAddressable + Clone + Send + Sync,
{
    pub fn new(
        mempool_service: TMempoolService,
        bft_replica_service: TBftReplicaService,
        inbound_connections: TInboundConnectionService,
        outbound_service: TOutboundService,
        committee: Committee<TAddr>,
        node_id: TAddr,
    ) -> Self {
        Self {
            mempool_service,
            bft_replica_service,
            inbound_connections,
            state: ConsensusWorkerState::Starting,
            current_view_id: ViewId(0),
            timeout: Duration::from_secs(10),
            outbound_service,
            committee,
            node_id,
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
        max_views_to_process: Option<usize>,
    ) -> Result<(), DigitalAssetError> {
        use ConsensusWorkerState::*;

        let mut views_processed = 0;
        loop {
            if let Some(max) = max_views_to_process {
                if max <= views_processed {
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
            dbg!(&trns);
            info!(target: LOG_TARGET, "Transitioning from {:?} to {:?}", trns.0, trns.1);
            views_processed += 1;
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
                let mut p = states::Prepare::new(self.inbound_connections.clone());
                p.next_event(&self.get_current_view(), self.timeout, shutdown).await
            },
            NextView => {
                let mut state = states::NextViewState::new();
                let prepare_qc = QuorumCertificate::new();
                state
                    .next_event(
                        &self.get_current_view(),
                        prepare_qc,
                        &mut self.outbound_service,
                        &self.committee,
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
            (_, TimedOut) => {
                dbg!("timing out?");
                NextView
            },
            (NextView, NewView { .. }) => {
                self.current_view_id = self.current_view_id.next();
                Prepare
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
    use crate::dan_layer::services::{
        infrastructure_services::mocks::mock_inbound,
        mocks::{mock_bft, mock_mempool},
    };

    use crate::dan_layer::services::infrastructure_services::mocks::{
        mock_outbound,
        MockInboundConnectionService,
        MockOutboundService,
    };
    use futures::task;
    use std::collections::HashMap;
    use tari_shutdown::Shutdown;
    use tokio::task::JoinHandle;

    fn start_replica(
        inbound: MockInboundConnectionService,
        outbound: MockOutboundService<&'static str>,
        committee: Committee<&'static str>,
        node_id: &'static str,
        shutdown_signal: ShutdownSignal,
    ) -> JoinHandle<()> {
        let mut replica_a = ConsensusWorker::new(mock_mempool(), mock_bft(), inbound, outbound, committee, node_id);
        tokio::spawn(async move {
            let res = replica_a.run(shutdown_signal, Some(10)).await;
        })
    }

    #[tokio::test(threaded_scheduler)]
    async fn test_simple_case() {
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();

        let committee = Committee::new(vec!["A", "B", "C", "D"]);
        let mut outbound = mock_outbound(committee.members.clone());

        let inbound_a = outbound.take_inbound(&"A").unwrap();
        let inbound_b = outbound.take_inbound(&"B").unwrap();
        let inbound_c = outbound.take_inbound(&"C").unwrap();
        let inbound_d = outbound.take_inbound(&"D").unwrap();

        let task_a = start_replica(inbound_a, outbound.clone(), committee.clone(), "A", signal.clone());
        let task_b = start_replica(inbound_b, outbound.clone(), committee.clone(), "B", signal.clone());
        let task_c = start_replica(inbound_c, outbound.clone(), committee.clone(), "C", signal.clone());
        let task_d = start_replica(inbound_d, outbound.clone(), committee.clone(), "D", signal.clone());
        shutdown.trigger().unwrap();
        task_a.await.unwrap();
        task_b.await.unwrap();
        task_c.await.unwrap();
        task_d.await.unwrap();
    }
}
