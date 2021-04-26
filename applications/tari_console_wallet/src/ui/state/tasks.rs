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

use crate::ui::{state::UiTransactionSendStatus, UiError};
use futures::StreamExt;
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::tari_amount::MicroTari;
use tari_wallet::transaction_service::handle::{TransactionEvent, TransactionServiceHandle};
use tokio::sync::watch;

const LOG_TARGET: &str = "wallet::console_wallet::tasks ";

pub async fn send_transaction_task(
    public_key: CommsPublicKey,
    amount: MicroTari,
    message: String,
    fee_per_gram: MicroTari,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionSendStatus>,
)
{
    let _ = result_tx.broadcast(UiTransactionSendStatus::Initiated);
    let mut event_stream = transaction_service_handle.get_event_stream_fused();
    let mut send_direct_received_result = (false, false);
    let mut send_saf_received_result = (false, false);
    match transaction_service_handle
        .send_transaction(public_key, amount, fee_per_gram, message)
        .await
    {
        Err(e) => {
            let _ = result_tx.broadcast(UiTransactionSendStatus::Error(UiError::from(e).to_string()));
        },
        Ok(our_tx_id) => {
            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => match &*event {
                        TransactionEvent::TransactionDiscoveryInProgress(tx_id) => {
                            if our_tx_id == *tx_id {
                                let _ = result_tx.broadcast(UiTransactionSendStatus::DiscoveryInProgress);
                            }
                        },
                        TransactionEvent::TransactionDirectSendResult(tx_id, result) => {
                            if our_tx_id == *tx_id {
                                send_direct_received_result = (true, *result);
                                if send_saf_received_result.0 {
                                    break;
                                }
                            }
                        },
                        TransactionEvent::TransactionStoreForwardSendResult(tx_id, result) => {
                            if our_tx_id == *tx_id {
                                send_saf_received_result = (true, *result);
                                if send_direct_received_result.0 {
                                    break;
                                }
                            }
                        },
                        TransactionEvent::TransactionCompletedImmediately(tx_id) => {
                            if our_tx_id == *tx_id {
                                let _ = result_tx.broadcast(UiTransactionSendStatus::TransactionComplete);
                                return;
                            }
                        },
                        _ => (),
                    },
                    Err(e) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
                        break;
                    },
                }
            }

            if send_direct_received_result.1 {
                let _ = result_tx.broadcast(UiTransactionSendStatus::SentDirect);
            } else if send_saf_received_result.1 {
                let _ = result_tx.broadcast(UiTransactionSendStatus::SentViaSaf);
            } else {
                let _ = result_tx.broadcast(UiTransactionSendStatus::Error(
                    "Transaction could not be sent".to_string(),
                ));
            }
        },
    }
}

pub async fn send_one_sided_transaction_task(
    public_key: CommsPublicKey,
    amount: MicroTari,
    message: String,
    fee_per_gram: MicroTari,
    mut transaction_service_handle: TransactionServiceHandle,
    result_tx: watch::Sender<UiTransactionSendStatus>,
)
{
    let _ = result_tx.broadcast(UiTransactionSendStatus::Initiated);
    let mut event_stream = transaction_service_handle.get_event_stream_fused();
    match transaction_service_handle
        .send_one_sided_transaction(public_key, amount, fee_per_gram, message)
        .await
    {
        Err(e) => {
            let _ = result_tx.broadcast(UiTransactionSendStatus::Error(UiError::from(e).to_string()));
        },
        Ok(our_tx_id) => {
            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => {
                        if let TransactionEvent::TransactionCompletedImmediately(tx_id) = &*event {
                            if our_tx_id == *tx_id {
                                let _ = result_tx.broadcast(UiTransactionSendStatus::TransactionComplete);
                                return;
                            }
                        }
                    },
                    Err(e) => {
                        log::warn!(target: LOG_TARGET, "Error reading from event broadcast channel {:?}", e);
                        break;
                    },
                }
            }

            let _ = result_tx.broadcast(UiTransactionSendStatus::Error(
                "One-sided transaction could not be sent".to_string(),
            ));
        },
    }
}
