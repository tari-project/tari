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

use crate::{
    transaction_service::{
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceHandle},
    },
    types::ValidationRetryStrategy,
};
use futures::StreamExt;
use log::*;

const LOG_TARGET: &str = "wallet::transaction_service::tasks::start_tx_validation_and_broadcast";

pub async fn start_transaction_validation_and_broadcast_protocols(
    mut handle: TransactionServiceHandle,
    retry_strategy: ValidationRetryStrategy,
) -> Result<(), TransactionServiceError>
{
    let mut event_stream = handle.get_event_stream_fused();
    let our_id = handle.validate_transactions(retry_strategy).await?;

    // Now that its started we will spawn an task to monitor the event bus and when its successful we will start the
    // Broadcast protocols

    tokio::spawn(async move {
        while let Some(event_item) = event_stream.next().await {
            if let Ok(event) = event_item {
                match (*event).clone() {
                    TransactionEvent::TransactionValidationSuccess(_id) => {
                        info!(
                            target: LOG_TARGET,
                            "Transaction Validation success, restarting broadcast protocols"
                        );
                        if let Err(e) = handle.restart_broadcast_protocols().await {
                            error!(
                                target: LOG_TARGET,
                                "Error restarting transaction broadcast protocols: {:?}", e
                            );
                        }
                    },
                    TransactionEvent::TransactionValidationFailure(id) => {
                        if our_id == id {
                            error!(target: LOG_TARGET, "Transaction Validation failed!");
                            break;
                        }
                    },
                    _ => (),
                }
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Error reading from Transaction Service Event Stream"
                );
                break;
            }
        }
    });

    Ok(())
}
