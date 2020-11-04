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

use futures::future;
use tari_core::base_node::{state_machine_service::states::StatusInfo, StateMachineHandle};
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::sync::{broadcast, watch};

pub struct MockBaseNodeStateMachine {
    status_receiver: watch::Receiver<StatusInfo>,
    status_sender: watch::Sender<StatusInfo>,
}

impl MockBaseNodeStateMachine {
    pub fn new() -> Self {
        let (status_sender, status_receiver) = tokio::sync::watch::channel(StatusInfo::new());
        Self {
            status_receiver,
            status_sender,
        }
    }

    pub fn publish_status(&mut self, status: StatusInfo) {
        let _ = self.status_sender.broadcast(status);
    }

    pub fn get_initializer(&self) -> MockBaseNodeStateMachineInitializer {
        MockBaseNodeStateMachineInitializer {
            status_receiver: self.status_receiver.clone(),
        }
    }
}

pub struct MockBaseNodeStateMachineInitializer {
    status_receiver: watch::Receiver<StatusInfo>,
}

impl ServiceInitializer for MockBaseNodeStateMachineInitializer {
    type Future = future::Ready<Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        let (state_event_publisher, _) = broadcast::channel(10);

        let handle = StateMachineHandle::new(
            state_event_publisher,
            self.status_receiver.clone(),
            context.get_shutdown_signal(),
        );
        context.register_handle(handle);
        future::ready(Ok(()))
    }
}
