// Copyright 2019. The Tari Project
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

use crate::base_node::states::{listening::Listening, InitialSync, StateEvent, StateEvent::FatalError};
use log::*;

const LOG_TARGET: &str = "base_node::fetching_horizon_state";

pub struct FetchHorizonState;

impl FetchHorizonState {
    pub async fn next_event(&mut self) -> StateEvent {
        info!(target: LOG_TARGET, "Starting synchronization of pruning horizon state");
        FatalError("Unimplemented".into())
    }
}

/// State management for InitialSync -> FetchingHorizonState. This is the typical transition for a new node joining
/// the network
impl From<InitialSync> for FetchHorizonState {
    fn from(_old: InitialSync) -> Self {
        unimplemented!()
    }
}

/// State management for Listening -> FetchingHorizonState. This can occur if a node has been disconnected from the
/// network for a long time.
impl From<Listening> for FetchHorizonState {
    fn from(_old: Listening) -> Self {
        unimplemented!()
    }
}
