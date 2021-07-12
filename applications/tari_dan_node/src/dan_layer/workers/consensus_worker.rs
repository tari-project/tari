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
        models::View,
        services::{infrastructure_services::InboundConnectionService, BftReplicaService, MempoolService},
        workers::{
            states,
            states::{ConsensusWorkerStateEvent, Prepare, Starting, State},
        },
    },
    digital_assets_error::DigitalAssetError,
};
use log::*;
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "tari::dan::consensus_worker";

pub struct ConsensusWorker<TMempoolService, TBftReplicaService, TInboundConnectionService>
where
    TMempoolService: MempoolService,
    TBftReplicaService: BftReplicaService,
    TInboundConnectionService: InboundConnectionService + Clone,
{
    mempool_service: TMempoolService,
    bft_replica_service: TBftReplicaService,
    inbound_connections: TInboundConnectionService,
    state: ConsensusWorkerState,
    current_view: Option<View>,
}

pub enum ConsensusWorkerState {
    Starting(Starting),
    Prepare(Box<dyn State + Send + Sync>),
}

impl<TMempoolService, TBftReplicaService, TInboundConnectionService>
    ConsensusWorker<TMempoolService, TBftReplicaService, TInboundConnectionService>
where
    TMempoolService: MempoolService,
    TBftReplicaService: BftReplicaService,
    TInboundConnectionService: InboundConnectionService + Clone + 'static + Send + Sync,
{
    pub fn new(
        mempool_service: TMempoolService,
        bft_replica_service: TBftReplicaService,
        inbound_connections: TInboundConnectionService,
    ) -> Self {
        Self {
            mempool_service,
            bft_replica_service,
            inbound_connections,
            state: ConsensusWorkerState::Starting(Starting {}),
            current_view: None,
        }
    }

    pub async fn run(&mut self, shutdown: ShutdownSignal) -> Result<(), DigitalAssetError> {
        self.current_view = Some(self.bft_replica_service.current_view());
        use ConsensusWorkerState::*;

        loop {
            let next_event = self.next_state_event(&shutdown).await?;
            if next_event.must_shutdown() {
                info!(
                    target: LOG_TARGET,
                    "Consensus worker is shutting down because {}",
                    next_event.shutdown_reason().unwrap_or_default()
                );
                break;
            }
            self.transition(next_event)?
        }

        Ok(())
    }

    async fn next_state_event(
        &mut self,
        shutdown: &ShutdownSignal,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        use ConsensusWorkerState::*;
        match &mut self.state {
            Starting(s) => s.next_event().await,
            Prepare(p) => {
                p.next_event(self.current_view.as_ref().expect("Need to handle option"), shutdown)
                    .await
            },
        }
    }

    fn transition(&mut self, event: ConsensusWorkerStateEvent) -> Result<(), DigitalAssetError> {
        use ConsensusWorkerState::*;
        self.state = match (&self.state, event) {
            (Starting(_), Initialized) => Prepare(Box::new(states::Prepare::new(self.inbound_connections.clone()))),
            _ => {
                unimplemented!("State machine transition not implemented")
            },
        };
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::dan_layer::services::{
        infrastructure_services::mocks::mock_inbound,
        mocks::{mock_bft, mock_mempool},
    };

    use futures::task;
    use tari_shutdown::Shutdown;

    #[tokio::test(threaded_scheduler)]
    async fn test_simple_case() {
        let mut replica = ConsensusWorker::new(mock_mempool(), mock_bft(), mock_inbound());
        let mut shutdown = Shutdown::new();
        let signal = shutdown.to_signal();

        let task = tokio::spawn(async move {
            let res = replica.run(signal).await;
        });
        shutdown.trigger().unwrap();
        task.await.unwrap()
    }
}
