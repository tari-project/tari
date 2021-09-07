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
        models::{View, ViewId},
        workers::ConsensusWorker,
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;

use tari_shutdown::ShutdownSignal;

// #[async_trait]
// pub trait State {
//     async fn next_event(
//         &mut self,
//         current_view: &View,
//         shutdown: &ShutdownSignal,
//     ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError>;
// }
use crate::dan_layer::models::QuorumCertificate;

mod commit_state;
mod decide_state;
mod next_view;
mod pre_commit_state;
mod prepare;
mod starting;

pub use commit_state::CommitState;
pub use decide_state::DecideState;
pub use next_view::NextViewState;
pub use pre_commit_state::PreCommitState;
pub use prepare::Prepare;
pub use starting::Starting;

#[derive(Debug, PartialEq)]
pub enum ConsensusWorkerStateEvent {
    Initialized,
    Errored { reason: String },
    Prepared,
    PreCommitted,
    Committed,
    Decided,
    ShutdownReceived,
    TimedOut,
    NewView { new_view: ViewId },
}

impl ConsensusWorkerStateEvent {
    pub fn must_shutdown(&self) -> bool {
        match self {
            ConsensusWorkerStateEvent::Errored { .. } => true,
            _ => false,
        }
    }

    pub fn shutdown_reason(&self) -> Option<&str> {
        match self {
            ConsensusWorkerStateEvent::Errored { reason } => Some(reason.as_str()),
            _ => None,
        }
    }
}
