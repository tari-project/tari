// Copyright 2023. The Tari Project
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

use futures::{pin_mut, StreamExt};
use log::{error, info};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;

use crate::ledger::handle::{LedgerServiceError, LedgerServiceRequest, LedgerServiceResponse};

const LOG_TARGET: &str = "hardware::ledger_service";

pub struct LedgerWalletService {
    request_stream:
        Option<reply_channel::Receiver<LedgerServiceRequest, Result<LedgerServiceResponse, LedgerServiceError>>>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl LedgerWalletService {
    pub fn new(
        request_stream: reply_channel::Receiver<
            LedgerServiceRequest,
            Result<LedgerServiceResponse, LedgerServiceError>,
        >,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            request_stream: Some(request_stream),
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn start(mut self) -> Result<(), LedgerServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Ledger Wallet Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let shutdown = self
            .shutdown_signal
            .take()
            .expect("Ledger Wallet Service initialized without shutdown signal");
        pin_mut!(shutdown);

        loop {
            tokio::select! {
                Some(request_context) = request_stream.next() => {
                    // handle incoming requests
                },
            }
        }
        info!(target: LOG_TARGET, "Ledger Wallet Service ended");
        Ok(())
    }
}
