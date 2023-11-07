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

use std::sync::{Arc, Mutex};

use futures::StreamExt;
use minotari_wallet::output_manager_service::{
    error::OutputManagerError,
    handle::{OutputManagerRequest, OutputManagerResponse},
    service::Balance,
};
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;

/// This macro unlocks a Mutex or RwLock. If the lock is poisoned (i.e. panic while unlocked) the last value
/// before the panic is used.
macro_rules! acquire_lock {
        ($e:expr, $m:ident) => {
            match $e.$m() {
                Ok(lock) => lock,
                Err(poisoned) => {
                    log::warn!(target: "wallet", "Lock has been POISONED and will be silently recovered");
                    poisoned.into_inner()
                },
            }
        };
        ($e:expr) => {
            acquire_lock!($e, lock)
        };
    }

#[derive(Clone, Debug)]
pub struct ResponseState {
    balance: Arc<Mutex<Balance>>,
}

impl ResponseState {
    pub fn new() -> Self {
        Self {
            balance: Arc::new(Mutex::new(Balance::zero())),
        }
    }

    /// Set the mock server balance response
    pub fn set_balance(&mut self, balance: Balance) {
        let mut lock = acquire_lock!(self.balance);
        *lock = balance;
    }

    /// Get the mock server balance value
    pub fn get_balance(&mut self) -> Balance {
        let lock = acquire_lock!(self.balance);
        (*lock).clone()
    }
}

pub struct MockOutputManagerService {
    request_stream: Option<Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    state: ResponseState,
    shutdown_signal: Option<ShutdownSignal>,
}

impl MockOutputManagerService {
    pub fn new(
        request_stream: Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            request_stream: Some(request_stream),
            state: ResponseState::new(),
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn run(mut self) -> Result<(), OutputManagerError> {
        let shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Output Manager Service initialized without shutdown signal");

        let mut request_stream = self
            .request_stream
            .take()
            .expect("Output Manager Service initialized without request_stream")
            .take_until(shutdown_signal);

        while let Some(request_context) = request_stream.next().await {
            // Incoming requests
            let (request, reply_tx) = request_context.split();
            let response = self.handle_request(request);
            let _result = reply_tx.send(response);
        }

        Ok(())
    }

    fn handle_request(&mut self, request: OutputManagerRequest) -> Result<OutputManagerResponse, OutputManagerError> {
        match request {
            OutputManagerRequest::GetBalance => Ok(OutputManagerResponse::Balance(self.state.get_balance())),
            _ => Err(OutputManagerError::InvalidResponseError(format!(
                "Request '{}' not defined for MockOutputManagerService!",
                request
            ))),
        }
    }

    /// Returns a clone of the response state to enable updating after the service started
    pub fn get_response_state(&mut self) -> ResponseState {
        self.state.clone()
    }
}
