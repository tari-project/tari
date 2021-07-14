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
        models::{HotStuffMessage, Payload},
        services::infrastructure_services::{InboundConnectionService, NodeAddressable},
        workers::states::ConsensusWorkerStateEvent,
    },
    digital_assets_error::DigitalAssetError,
};
use std::marker::PhantomData;
use tokio::time::{delay_for, Duration};

pub struct PreCommitState<TAddr, TPayload, TInboundConnectionService>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TPayload: Payload,
{
    phantom_inbound: PhantomData<TInboundConnectionService>,
    ta: PhantomData<TAddr>,
    p_p: PhantomData<TPayload>,
}

impl<TAddr, TPayload, TInboundConnectionService> PreCommitState<TAddr, TPayload, TInboundConnectionService>
where
    TInboundConnectionService: InboundConnectionService<TAddr, TPayload>,
    TAddr: NodeAddressable,
    TPayload: Payload,
{
    pub fn new() -> Self {
        Self {
            phantom_inbound: PhantomData,
            ta: PhantomData,
            p_p: PhantomData,
        }
    }

    pub async fn next_event(
        &self,
        timeout: Duration,
        inbound_services: &mut TInboundConnectionService,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        let mut next_event_result = ConsensusWorkerStateEvent::Errored {
            reason: "loop ended without setting this event".to_string(),
        };

        loop {
            tokio::select! {
                 (from, message) = self.wait_for_message(inbound_services) => {
                    dbg!("Received message: ", &message);
                    }
            _ = delay_for(timeout) =>  {
                          // TODO: perhaps this should be from the time the state was entered
                          next_event_result = ConsensusWorkerStateEvent::TimedOut;
                          break;
                      }
                  }
        }
        Ok(next_event_result)
    }

    async fn wait_for_message(
        &self,
        inbound_connection: &mut TInboundConnectionService,
    ) -> (TAddr, HotStuffMessage<TPayload>) {
        inbound_connection.receive_message().await
    }
}
