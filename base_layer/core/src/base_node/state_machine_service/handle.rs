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

use crate::base_node::state_machine_service::states::{StateEvent, StatusInfo};
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, watch};

#[derive(Clone)]
pub struct StateMachineHandle {
    state_change_event_subscriber: broadcast::Sender<Arc<StateEvent>>,
    status_event_receiver: watch::Receiver<StatusInfo>,
    shutdown_signal: ShutdownSignal,
}

impl StateMachineHandle {
    pub fn new(
        state_change_event_subscriber: broadcast::Sender<Arc<StateEvent>>,
        status_event_receiver: watch::Receiver<StatusInfo>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            state_change_event_subscriber,
            status_event_receiver,
            shutdown_signal,
        }
    }

    /// This clones the receiver end of the channel and gives out a copy to the caller
    /// This allows multiple subscribers to this channel by only keeping one channel and cloning the receiver for every
    /// caller.
    pub fn get_state_change_event_stream(&self) -> broadcast::Receiver<Arc<StateEvent>> {
        self.state_change_event_subscriber.subscribe()
    }

    /// This clones the receiver end of the channel and gives out a copy to the caller
    /// This allows multiple subscribers to this channel by only keeping one channel and cloning the receiver for every
    /// caller.
    pub fn get_status_info_watch(&self) -> watch::Receiver<StatusInfo> {
        self.status_event_receiver.clone()
    }

    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown_signal.clone()
    }
}
