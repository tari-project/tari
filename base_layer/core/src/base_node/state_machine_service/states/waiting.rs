// Copyright 2020. The Tari Project
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

use crate::base_node::state_machine_service::states::{BlockSyncStrategy, HeaderSync, HorizonStateSync, StateEvent};
use log::info;
use std::time::Duration;
use tokio::time::delay_for;

const LOG_TARGET: &str = "c::bn::state_machine_service::states::waiting";

/// A time-out state for the base node. It will do nothing in this state; and return a Continue event once the
/// timeout is complete.
#[derive(Clone, Debug, PartialEq)]
pub struct Waiting {
    timeout: Duration,
}

impl Waiting {
    pub async fn next_event(&self) -> StateEvent {
        info!(
            target: LOG_TARGET,
            "The base node has started a WAITING state for {} seconds",
            self.timeout.as_secs()
        );
        delay_for(self.timeout).await;
        info!(
            target: LOG_TARGET,
            "The base node waiting state has completed. Resuming normal operations"
        );
        StateEvent::Continue
    }
}

impl Default for Waiting {
    /// A default timeout of 1 minute applies
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
        }
    }
}

/// Moving from state BlockSyncStrategy -> Waiting.
impl From<BlockSyncStrategy> for Waiting {
    fn from(_: BlockSyncStrategy) -> Self {
        Default::default()
    }
}

/// Moving from state HorizonStateSync -> Waiting. A timeout of 1 minute applies
impl From<HeaderSync> for Waiting {
    fn from(_: HeaderSync) -> Self {
        Default::default()
    }
}

/// Moving from state HorizonStateSync -> Waiting. A timeout of 1 minute applies
impl From<HorizonStateSync> for Waiting {
    fn from(_: HorizonStateSync) -> Self {
        Default::default()
    }
}
