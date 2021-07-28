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

use futures::StreamExt;
use log::*;
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::{error::WalletError, utxo_scanner_service::handle::UtxoScannerEvent};
use tokio::{sync::broadcast, task::JoinHandle};

const LOG_TARGET: &str = "wallet_ffi";

/// Events that the recovery process will report via the callback
enum RecoveryEvent {
    ConnectingToBaseNode,       // 0
    ConnectedToBaseNode,        // 1
    ConnectionToBaseNodeFailed, // 2
    Progress,                   // 3
    Completed,                  // 4
    ScanningRoundFailed,        // 5
    RecoveryFailed,             // 6
}

pub async fn recovery_event_monitoring(
    mut event_stream: broadcast::Receiver<UtxoScannerEvent>,
    recovery_join_handle: JoinHandle<Result<(), WalletError>>,
    recovery_progress_callback: unsafe extern "C" fn(u8, u64, u64),
) {
    while let Some(event) = event_stream.next().await {
        match event {
            Ok(UtxoScannerEvent::ConnectingToBaseNode(peer)) => {
                unsafe {
                    (recovery_progress_callback)(RecoveryEvent::ConnectingToBaseNode as u8, 0u64, 0u64);
                }
                info!(
                    target: LOG_TARGET,
                    "Attempting connection to base node {}",
                    peer.to_hex(),
                );
            },
            Ok(UtxoScannerEvent::ConnectedToBaseNode(pk, elapsed)) => {
                unsafe {
                    (recovery_progress_callback)(RecoveryEvent::ConnectedToBaseNode as u8, 0u64, 1u64);
                }
                info!(
                    target: LOG_TARGET,
                    "Connected to base node {} in {:.2?}",
                    pk.to_hex(),
                    elapsed
                );
            },
            Ok(UtxoScannerEvent::ConnectionFailedToBaseNode {
                peer,
                num_retries,
                retry_limit,
                error,
            }) => {
                unsafe {
                    (recovery_progress_callback)(
                        RecoveryEvent::ConnectionToBaseNodeFailed as u8,
                        num_retries as u64,
                        retry_limit as u64,
                    );
                }
                warn!(
                    target: LOG_TARGET,
                    "Failed to connect to base node {} with error {}",
                    peer.to_hex(),
                    error
                );
            },
            Ok(UtxoScannerEvent::Progress {
                current_block: current,
                current_chain_height: total,
            }) => {
                unsafe {
                    (recovery_progress_callback)(RecoveryEvent::Progress as u8, current, total);
                }
                info!(target: LOG_TARGET, "Recovery progress: {}/{}", current, total);
            },
            Ok(UtxoScannerEvent::Completed {
                number_scanned: num_scanned,
                number_received: num_utxos,
                value_received: total_amount,
                time_taken: elapsed,
            }) => {
                info!(
                    target: LOG_TARGET,
                    "Recovery complete! Scanned = {} in {:.2?} ({} utxos/s), Recovered {} worth {}",
                    num_scanned,
                    elapsed,
                    num_scanned / elapsed.as_secs(),
                    num_utxos,
                    total_amount
                );
                unsafe {
                    (recovery_progress_callback)(RecoveryEvent::Completed as u8, num_scanned, u64::from(total_amount));
                }
                break;
            },
            Ok(UtxoScannerEvent::ScanningRoundFailed {
                num_retries,
                retry_limit,
            }) => {
                unsafe {
                    (recovery_progress_callback)(
                        RecoveryEvent::ScanningRoundFailed as u8,
                        num_retries as u64,
                        retry_limit as u64,
                    );
                }
                info!(
                    target: LOG_TARGET,
                    "UTXO Scanning round failed on retry {} of {}", num_retries, retry_limit
                );
            },
            Ok(UtxoScannerEvent::ScanningFailed) => {
                unsafe {
                    (recovery_progress_callback)(RecoveryEvent::RecoveryFailed as u8, 0u64, 0u64);
                }
                warn!(target: LOG_TARGET, "UTXO Scanner failed and exited",);
            },
            Err(e) => {
                // Event lagging
                warn!(target: LOG_TARGET, "{}", e);
            },
        }
    }

    let recovery_result = recovery_join_handle.await;
    match recovery_result {
        Ok(Ok(_)) => {},
        Ok(Err(e)) => {
            unsafe {
                (recovery_progress_callback)(RecoveryEvent::RecoveryFailed as u8, 0u64, 1u64);
            }
            error!(target: LOG_TARGET, "Recovery error: {:?}", e);
        },
        Err(e) => {
            unsafe {
                (recovery_progress_callback)(RecoveryEvent::RecoveryFailed as u8, 1u64, 0u64);
            }
            error!(target: LOG_TARGET, "Recovery error: {}", e);
        },
    }
}
