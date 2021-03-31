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

use futures::{channel::mpsc::Receiver, future::Fuse as FutureFuse, stream::Fuse, StreamExt};
use log::*;
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::{error::WalletError, tasks::wallet_recovery::WalletRecoveryEvent};
use tokio::task::JoinHandle;

const LOG_TARGET: &str = "wallet_ffi";

pub async fn recovery_event_monitoring(
    mut event_stream: Fuse<Receiver<WalletRecoveryEvent>>,
    mut recovery_join_handle: FutureFuse<JoinHandle<Result<(), WalletError>>>,
    recovery_progress_callback: unsafe extern "C" fn(u64, u64),
)
{
    loop {
        futures::select! {
            event = event_stream.select_next_some() => {
                match event {
                    WalletRecoveryEvent::ConnectedToBaseNode(pk) => {
                       unsafe {
                            (recovery_progress_callback)(0u64,1u64);
                       }
                       info!(target: LOG_TARGET, "Connected to base node: {}", pk.to_hex());
                    },
                    WalletRecoveryEvent::Progress(current, total) => {
                           unsafe {
                                (recovery_progress_callback)(current,total);
                            }
                            info!(target: LOG_TARGET, "Recovery progress: {}/{}", current, total);
                            if current == total {
                                info!(target: LOG_TARGET, "Recovery complete: {}/{}", current, total);
                                break;
                            }
                    },
                    WalletRecoveryEvent::Completed(num_utxos, total_amount) => {
                        debug!(target: LOG_TARGET, "Recovery complete. Num UTXOs: {}, Amount: {}", num_utxos, total_amount);
                        break;
                    },
                }
            },
            recovery_result = recovery_join_handle => {
                match recovery_result {
                    Err(e) => {
                        unsafe {
                            (recovery_progress_callback)(0u64,0u64);
                        }
                        error!(target: LOG_TARGET, "Recovery error: {}", e);
                        break;
                    },
                    Ok(r) => {
                        if let Err(e) = r {
                            unsafe {
                                (recovery_progress_callback)(0u64,0u64);
                            }
                            error!(target: LOG_TARGET, "Recovery error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    }
}
