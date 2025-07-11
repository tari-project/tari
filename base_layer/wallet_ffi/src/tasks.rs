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

use std::ffi::c_void;

use log::*;
use minotari_wallet::utxo_scanner_service::handle::UtxoScannerEvent;
use tokio::sync::broadcast;

use crate::callback_handler::Context;

const LOG_TARGET: &str = "wallet_ffi";

/// Events that the recovery process will report via the callback
enum RecoveryEvent {
    Progress,            // 3
    Completed,           // 4
    ScanningRoundFailed, // 5
}

#[allow(clippy::too_many_lines)]
pub async fn recovery_event_monitoring(
    mut event_stream: broadcast::Receiver<UtxoScannerEvent>,
    recovery_progress_callback: unsafe extern "C" fn(context: *mut c_void, u8, u64, u64),
    context: Context,
) {
    loop {
        match event_stream.recv().await {
            Ok(UtxoScannerEvent::Progress {
                current_height: current,
                tip_height: total,
                ..
            }) => {
                unsafe {
                    (recovery_progress_callback)(context.0, RecoveryEvent::Progress as u8, current, total);
                }
                info!(target: LOG_TARGET, "Recovery progress: {}/{}", current, total);
            },
            Ok(UtxoScannerEvent::Completed {
                final_height,
                time_taken: elapsed,
                num_recovered,
                value_recovered,
                ..
            }) => {
                let rate = (final_height as f32) * 1000f32 / (elapsed.as_millis() as f32);
                info!(
                    target: LOG_TARGET,
                    "Recovery complete! Scanned {} blocks in {:.2?} ({:.2?} blocks/s)",
                    final_height,
                    elapsed,
                    rate,
                );
                unsafe {
                    (recovery_progress_callback)(
                        context.0,
                        RecoveryEvent::Completed as u8,
                        num_recovered,
                        value_recovered.as_u64(),
                    );
                }
                break;
            },
            Ok(UtxoScannerEvent::ScanningRoundFailed {
                num_retries,
                retry_limit,
                error,
            }) => {
                unsafe {
                    (recovery_progress_callback)(
                        context.0,
                        RecoveryEvent::ScanningRoundFailed as u8,
                        num_retries as u64,
                        retry_limit as u64,
                    );
                }
                info!(
                    target: LOG_TARGET,
                    "UTXO Scanning round failed on retry {} of {}: {}", num_retries, retry_limit, error
                );
            },
            Err(broadcast::error::RecvError::Closed) => {
                break;
            },
            Err(e) => {
                // Event lagging
                warn!(target: LOG_TARGET, "{}", e);
            },
        }
    }
}
