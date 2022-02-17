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

use std::sync::Arc;

use futures::StreamExt;
use log::*;
use tari_common_types::transaction::TxId;
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tari_shutdown::ShutdownSignal;
use tari_wallet::transaction_service::{
    error::TransactionServiceError,
    handle::{TransactionEvent, TransactionServiceHandle, TransactionServiceRequest, TransactionServiceResponse},
};
use tokio::sync::{broadcast, broadcast::Sender, oneshot};

const LOG_TARGET: &str = "wallet::transaction_service_mock";

pub fn make_transaction_service_mock(
    shutdown_signal: ShutdownSignal,
) -> (TransactionServiceMock, TransactionServiceHandle) {
    let (sender, receiver) = reply_channel::unbounded();
    let (publisher, _) = broadcast::channel(100);
    let transaction_handle = TransactionServiceHandle::new(sender, publisher.clone());
    let mock = TransactionServiceMock::new(publisher, receiver, shutdown_signal);
    (mock, transaction_handle)
}

pub struct TransactionServiceMock {
    _event_publisher: Sender<Arc<TransactionEvent>>,
    request_stream:
        Option<Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>>,
    shutdown_signal: ShutdownSignal,
}

impl TransactionServiceMock {
    pub fn new(
        event_publisher: Sender<Arc<TransactionEvent>>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            _event_publisher: event_publisher,
            request_stream: Some(request_stream),
            shutdown_signal,
        }
    }

    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "Starting Mock Transaction Service");

        let mut shutdown = self.shutdown_signal.clone();
        let mut request_stream = self.request_stream.take().unwrap();

        loop {
            tokio::select! {
                Some(request_context) = request_stream.next() => {
                    let (request, reply_tx) = request_context.split();
                    Self::handle_request(request, reply_tx);
                },
                 _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Transaction service mock shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    fn handle_request(
        request: TransactionServiceRequest,
        reply_tx: oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
    ) {
        info!(target: LOG_TARGET, "Handling Request: {}", request);

        match request {
            TransactionServiceRequest::ImportUtxoWithStatus { .. } => {
                let _ = reply_tx
                    .send(Ok(TransactionServiceResponse::UtxoImported(TxId::from(42u64))))
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
            },
            _ => panic!("Transaction Service Mock does not support this call"),
        }
    }
}
