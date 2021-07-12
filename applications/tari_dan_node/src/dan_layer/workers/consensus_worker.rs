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


use crate::dan_layer::services::{MempoolService, BftReplicaService};
use tari_shutdown::ShutdownSignal;
use crate::digital_assets_error::DigitalAssetError;
use crate::dan_layer::workers::states::{Starting, ConsensusWorkerStateEvent, Prepare};
use log::*;
use crate::dan_layer::workers::states;

const LOG_TARGET: &str = "tari::dan::consensus_worker";

pub struct ConsensusWorker<TMempoolService: MempoolService, TBftReplicaService: BftReplicaService> {
  mempool_service: TMempoolService,
    bft_replica_service: TBftReplicaService,
    state: ConsensusWorkerState
}

pub enum ConsensusWorkerState {
    Starting(Starting),
    Prepare(Prepare),
}



impl<TMempoolService:MempoolService, TBftReplicaService:BftReplicaService> ConsensusWorker<TMempoolService, TBftReplicaService> {

    pub fn new(mempool_service: TMempoolService, bft_replica_service: TBftReplicaService) -> Self {
        Self {
            mempool_service,
            bft_replica_service,
            state : ConsensusWorkerState::Starting(Starting{})
        }
    }

    pub async fn run(&mut self, shutdown: ShutdownSignal) -> Result<(), DigitalAssetError>{
        let view = self.bft_replica_service.current_view();
use ConsensusWorkerState::*;


        loop {
            let next_event = self.next_state_event().await?;
            if next_event.must_shutdown() {
                info!(target: LOG_TARGET, "Consensus worker is shutting down because {}", next_event.shutdown_reason().unwrap_or_default());
                break;
            }
            self.transition(next_event)?
        }

        Ok(())
    }

    async fn next_state_event(&self) -> Result<ConsensusWorkerStateEvent, DigitalAssetError>{
        use ConsensusWorkerState::*;
        match &self.state {
            Starting(s) => s.next_event().await,
            Prepare(p) => p.next_event().await,
        }
    }

    fn transition(&mut self, event: ConsensusWorkerStateEvent) -> Result<(), DigitalAssetError> {
        use ConsensusWorkerState::*;
        self.state = match (&self.state, event) {
            (Starting(_), Initialized) => Prepare(states::Prepare{}),
            _ => {
                unimplemented!("State machine transition not implemented")
            }
        };
        Ok(())
    }

}
