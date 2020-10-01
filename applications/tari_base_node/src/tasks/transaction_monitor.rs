//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use futures::{
    future,
    stream::{Fuse, StreamExt},
    FutureExt,
};
use log::*;
use std::time::Duration;
use tari_comms::types::CommsPublicKey;
use tari_core::base_node::StateMachineHandle;
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerEventReceiver, OutputManagerHandle},
        protocols::txo_validation_protocol::{TxoValidationRetry, TxoValidationType},
    },
    transaction_service::handle::TransactionServiceHandle,
};
use tokio::{task, time};

const LOG_TARGET: &str = "c::bn::transaction_protocols_and_utxo_validation";

/// Asynchronously start transaction protocols and TXO validation
/// ## Parameters
/// `state_machine` - A handle to the state machine
/// `transaction_service_handle` - A handle to the transaction service
/// `oms_handle` - A handle to the output manager service
/// `base_node_query_timeout` - A time after which queries to the base node times out
/// `base_node_public_key` - The base node's public key
/// `interrupt_signal` - terminate this task if we receive an interrupt signal
pub fn spawn_transaction_protocols_and_utxo_validation(
    state_machine: StateMachineHandle,
    transaction_service_handle: TransactionServiceHandle,
    oms_handle: OutputManagerHandle,
    base_node_query_timeout: Duration,
    base_node_public_key: CommsPublicKey,
    interrupt_signal: ShutdownSignal,
)
{
    task::spawn(async move {
        let transaction_task_fut = start_transaction_protocols_and_utxo_validation(
            state_machine,
            transaction_service_handle,
            oms_handle,
            base_node_query_timeout,
            base_node_public_key,
        );
        futures::pin_mut!(transaction_task_fut);
        // Exit on shutdown
        future::select(transaction_task_fut, interrupt_signal).await;
    });
}

/// Asynchronously start transaction protocols and TXO validation
/// ## Parameters
/// `state_machine` - A handle to the state machine
/// `transaction_service_handle` - A handle to the transaction service
/// `oms_handle` - A handle to the output manager service
/// `base_node_query_timeout` - A time after which queries to the base node times out
/// `base_node_public_key` - The base node's public key
pub async fn start_transaction_protocols_and_utxo_validation(
    state_machine: StateMachineHandle,
    mut transaction_service_handle: TransactionServiceHandle,
    mut oms_handle: OutputManagerHandle,
    base_node_query_timeout: Duration,
    base_node_public_key: CommsPublicKey,
)
{
    let mut status_watch = state_machine.get_status_info_watch();
    debug!(
        target: LOG_TARGET,
        "Waiting for initial sync before restarting transaction protocols and performing UTXO validation."
    );
    loop {
        let bootstrapped = match status_watch.recv().await {
            None => false,
            Some(s) => s.bootstrapped,
        };

        if bootstrapped {
            debug!(
                target: LOG_TARGET,
                "Initial sync achieved and starting with transaction and UTXO validation protocols.",
            );
            let _ = transaction_service_handle
                .restart_broadcast_protocols()
                .await
                .map_err(|e| {
                    error!(
                        target: LOG_TARGET,
                        "Problem restarting broadcast protocols in the Transaction Service: {}", e
                    );
                    e
                });

            let _ = transaction_service_handle
                .restart_transaction_protocols()
                .await
                .map_err(|e| {
                    error!(
                        target: LOG_TARGET,
                        "Problem restarting transaction negotiation protocols in the Transaction Service: {}", e
                    );
                    e
                });

            let _ = oms_handle
                .set_base_node_public_key(base_node_public_key.clone())
                .await
                .map_err(|e| {
                    error!(
                        target: LOG_TARGET,
                        "Problem with Output Manager Service setting the base node public key: {}", e
                    );
                    e
                });

            loop {
                let _ = oms_handle
                    .validate_txos(TxoValidationType::Unspent, TxoValidationRetry::UntilSuccess)
                    .await
                    .map_err(|e| {
                        error!(
                            target: LOG_TARGET,
                            "Problem starting UTXO validation protocols in the Output Manager Service: {}", e
                        );
                        e
                    });

                trace!(target: LOG_TARGET, "Attempting UTXO validation for Unspent Outputs.",);
                if monitor_validation_protocol(base_node_query_timeout * 2, oms_handle.get_event_stream_fused()).await {
                    break;
                }
            }

            loop {
                let _ = oms_handle
                    .validate_txos(TxoValidationType::Invalid, TxoValidationRetry::UntilSuccess)
                    .await
                    .map_err(|e| {
                        error!(
                            target: LOG_TARGET,
                            "Problem starting Invalid TXO validation protocols in the Output Manager Service: {}", e
                        );
                        e
                    });

                trace!(
                    target: LOG_TARGET,
                    "Attempting Invalid TXO validation for Unspent Outputs.",
                );
                if monitor_validation_protocol(base_node_query_timeout * 2, oms_handle.get_event_stream_fused()).await {
                    break;
                }
            }

            loop {
                let _ = oms_handle
                    .validate_txos(TxoValidationType::Spent, TxoValidationRetry::UntilSuccess)
                    .await
                    .map_err(|e| {
                        error!(
                            target: LOG_TARGET,
                            "Problem starting STXO validation protocols in the Output Manager Service: {}", e
                        );
                        e
                    });

                trace!(target: LOG_TARGET, "Attempting STXO validation for Unspent Outputs.",);
                if monitor_validation_protocol(base_node_query_timeout * 2, oms_handle.get_event_stream_fused()).await {
                    break;
                }
            }

            break;
        }
    }
    debug!(
        target: LOG_TARGET,
        "Restarting of transaction protocols and performing UTXO validation concluded."
    );
}

/// Monitors the TXO validation protocol events
/// /// ## Paramters
/// `event_timeout` - A time after which waiting for the next event from the output manager service times out
/// `event_stream` - The TXO validation protocol subscriber event stream
///
/// ##Returns
/// bool indicating success or failure
async fn monitor_validation_protocol(
    event_timeout: Duration,
    mut event_stream: Fuse<OutputManagerEventReceiver>,
) -> bool
{
    let mut success = false;
    loop {
        let mut delay = time::delay_for(event_timeout).fuse();
        futures::select! {
            event = event_stream.select_next_some() => {
                match event.unwrap() {
                    // Restart the protocol if aborted (due to 'BaseNodeNotSynced')
                    OutputManagerEvent::TxoValidationAborted(s) => {
                        trace!(
                            target: LOG_TARGET,
                            "UTXO validation event 'TxoValidationAborted' ({}), restarting.", s,
                        );
                        break;
                    },
                    // Restart the protocol if failure
                    OutputManagerEvent::TxoValidationFailure(s) => {
                        trace!(
                            target: LOG_TARGET,
                            "UTXO validation event 'TxoValidationFailure' ({}), restarting.", s,
                        );
                        break;
                    },
                    // Exit upon success
                    OutputManagerEvent::TxoValidationSuccess(s) => {
                        trace!(
                            target: LOG_TARGET,
                            "UTXO validation event 'TxoValidationSuccess' ({}), success.", s,
                        );
                        success = true;
                        break;
                    },
                    // Wait for the next event if timed out (several can be attempted)
                    OutputManagerEvent::TxoValidationTimedOut(s) => {
                        trace!(
                            target: LOG_TARGET,
                            "UTXO validation event 'TxoValidationTimedOut' ({}), waiting.", s,
                        );
                        continue;
                    }
                    // Wait for the next event upon an error
                    OutputManagerEvent::Error(s) => {
                        trace!(
                            target: LOG_TARGET,
                            "UTXO validation event 'Error({})', waiting.", s,
                        );
                        continue;
                    },
                    // Wait for the next event upon anything else
                    _ => {
                        trace!(
                            target: LOG_TARGET,
                            "UTXO validation unknown event, waiting.",
                        );
                        continue;
                    },
                }
            },
            // Restart the protocol if it timed out
            () = delay => {
                trace!(
                    target: LOG_TARGET,
                    "UTXO validation protocol timed out, restarting.",
                );
                break;
            },
        }
    }
    success
}
