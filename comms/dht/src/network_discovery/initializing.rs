//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::network_discovery::state_machine::{NetworkDiscoveryContext, StateEvent};
use log::*;
use std::time::Duration;
use tari_comms::connectivity::ConnectivityError;

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug)]
pub struct Initializing<'a> {
    context: &'a mut NetworkDiscoveryContext,
}

impl<'a> Initializing<'a> {
    pub fn new(context: &'a mut NetworkDiscoveryContext) -> Self {
        Self { context }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        let connectivity = &mut self.context.connectivity;
        debug!(target: LOG_TARGET, "Waiting for this node to come online...");
        while let Err(err) = connectivity.wait_for_connectivity(Duration::from_secs(10)).await {
            match err {
                ConnectivityError::OnlineWaitTimeout => {
                    debug!(target: LOG_TARGET, "Still waiting for this node to come online...");
                },
                _ => {
                    return err.into();
                },
            }
        }

        debug!(target: LOG_TARGET, "Node is online. Starting network discovery");
        StateEvent::Initialized
    }
}
