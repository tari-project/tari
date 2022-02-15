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

use std::sync::{Arc, Mutex};

use futures::StreamExt;
use log::*;
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::ShutdownSignal;
use tari_wallet::output_manager_service::{
    error::OutputManagerError,
    handle::{OutputManagerEvent, OutputManagerHandle, OutputManagerRequest, OutputManagerResponse},
    storage::models::DbUnblindedOutput,
};
use tokio::sync::{broadcast, broadcast::Sender, oneshot};

const LOG_TARGET: &str = "wallet::output_manager_service_mock";

pub fn make_output_manager_service_mock(
    shutdown_signal: ShutdownSignal,
) -> (OutputManagerServiceMock, OutputManagerHandle) {
    let (sender, receiver) = reply_channel::unbounded();
    let (publisher, _) = broadcast::channel(100);
    let output_manager_handle = OutputManagerHandle::new(sender, publisher.clone());
    let mock = OutputManagerServiceMock::new(publisher, receiver, shutdown_signal);
    (mock, output_manager_handle)
}

pub struct OutputManagerServiceMock {
    _event_publisher: Sender<Arc<OutputManagerEvent>>,
    request_stream: Option<Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    shutdown_signal: ShutdownSignal,
    state: OutputManagerMockState,
}

impl OutputManagerServiceMock {
    pub fn new(
        event_publisher: Sender<Arc<OutputManagerEvent>>,
        request_stream: Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            _event_publisher: event_publisher,
            request_stream: Some(request_stream),
            shutdown_signal,
            state: OutputManagerMockState::new(),
        }
    }

    pub fn get_state(&self) -> OutputManagerMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "Starting Mock OutputManager Service");
        let mut shutdown = self.shutdown_signal.clone();
        let mut request_stream = self.request_stream.take().unwrap();

        loop {
            tokio::select! {
                Some(request_context) = request_stream.next() => {
                    let (request, reply_tx) = request_context.split();
                    self.handle_request(request, reply_tx);
                },
                 _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "OutputManager service mock shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    fn handle_request(
        &self,
        request: OutputManagerRequest,
        reply_tx: oneshot::Sender<Result<OutputManagerResponse, OutputManagerError>>,
    ) {
        info!(target: LOG_TARGET, "Handling Request: {}", request);
        match request {
            OutputManagerRequest::ScanForRecoverableOutputs {
                outputs: requested_outputs,
                tx_id: _tx_id,
            } => {
                let lock = acquire_lock!(self.state.recoverable_outputs);
                let outputs = (*lock)
                    .clone()
                    .into_iter()
                    .filter_map(|dbuo| {
                        if requested_outputs.iter().any(|ro| dbuo.commitment == ro.commitment) {
                            Some(dbuo.unblinded_output)
                        } else {
                            None
                        }
                    })
                    .collect();

                let _ = reply_tx
                    .send(Ok(OutputManagerResponse::RewoundOutputs(outputs)))
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
            },
            OutputManagerRequest::ScanOutputs {
                outputs: _to,
                tx_id: _tx_id,
            } => {
                let lock = acquire_lock!(self.state.one_sided_payments);
                let outputs = (*lock).clone();
                let _ = reply_tx
                    .send(Ok(OutputManagerResponse::ScanOutputs(
                        outputs.into_iter().map(|dbuo| dbuo.unblinded_output).collect(),
                    )))
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
            },
            _ => panic!("Output Manager Service Mock does not support this call"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputManagerMockState {
    pub recoverable_outputs: Arc<Mutex<Vec<DbUnblindedOutput>>>,
    pub one_sided_payments: Arc<Mutex<Vec<DbUnblindedOutput>>>,
}

impl OutputManagerMockState {
    pub fn new() -> Self {
        Self {
            recoverable_outputs: Arc::new(Mutex::new(Vec::new())),
            one_sided_payments: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_recoverable_outputs(&self, outputs: Vec<DbUnblindedOutput>) {
        let mut lock = acquire_lock!(self.recoverable_outputs);
        *lock = outputs;
    }

    pub fn _set_one_sided_payments(&self, outputs: Vec<DbUnblindedOutput>) {
        let mut lock = acquire_lock!(self.one_sided_payments);
        *lock = outputs;
    }
}

impl Default for OutputManagerMockState {
    fn default() -> Self {
        Self::new()
    }
}
