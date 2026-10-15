// Copyright 2024. The Tari Project
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

use log::*;
use tari_shutdown::Shutdown;
use tokio::runtime::Handle;

const LOG_TARGET: &str = "wallet::console_wallet::signal_handler";

/// Spawns a background task on the given runtime handle that listens for termination signals
/// (SIGINT, SIGTERM on Unix; Ctrl+C on Windows) and triggers the provided [`Shutdown`].
pub fn spawn_signal_handler(shutdown: Shutdown, handle: &Handle) {
    let mut shutdown_signal = shutdown.to_signal();
    handle.spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let mut sigint = match signal(SignalKind::interrupt()) {
                Ok(s) => s,
                Err(e) => {
                    error!(target: LOG_TARGET, "Failed to install SIGINT handler: {e}");
                    return;
                },
            };
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    error!(target: LOG_TARGET, "Failed to install SIGTERM handler: {e}");
                    return;
                },
            };

            tokio::select! {
                biased;
                _ = shutdown_signal.wait() => {
                    trace!(target: LOG_TARGET, "Shutdown already triggered elsewhere; exiting signal handler");
                }
                _ = sigint.recv() => {
                    info!(target: LOG_TARGET, "Termination signal SIGINT received, triggering shutdown");
                    shutdown.trigger();
                }
                _ = sigterm.recv() => {
                    info!(target: LOG_TARGET, "Termination signal SIGTERM received, triggering shutdown");
                    shutdown.trigger();
                }
            }
        }

        #[cfg(windows)]
        {
            tokio::select! {
                biased;
                _ = shutdown_signal.wait() => {
                    trace!(target: LOG_TARGET, "Shutdown already triggered elsewhere; exiting signal handler");
                }
                res = tokio::signal::ctrl_c() => {
                    match res {
                        Ok(()) => {
                            info!(target: LOG_TARGET, "Ctrl+C received, triggering shutdown");
                            shutdown.trigger();
                        }
                        Err(e) => {
                            error!(target: LOG_TARGET, "Error listening for Ctrl+C: {e}");
                        }
                    }
                }
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            shutdown_signal.wait().await;
        }
    });
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tari_shutdown::Shutdown;

    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn test_signal_handler_triggers_shutdown_on_sigterm() {
        let shutdown = Shutdown::new();
        let mut signal = shutdown.to_signal();
        let handle = tokio::runtime::Handle::current();

        spawn_signal_handler(shutdown, &handle);

        tokio::time::sleep(Duration::from_millis(50)).await;

        unsafe extern "C" {
            fn raise(sig: std::ffi::c_int) -> std::ffi::c_int;
        }

        unsafe {
            raise(15);
        }

        tokio::time::timeout(Duration::from_secs(5), signal.wait())
            .await
            .expect("shutdown should be triggered when SIGTERM is received");

        assert!(signal.is_triggered());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_signal_handler_triggers_shutdown_on_sigint() {
        let shutdown = Shutdown::new();
        let mut signal = shutdown.to_signal();
        let handle = tokio::runtime::Handle::current();

        spawn_signal_handler(shutdown, &handle);

        tokio::time::sleep(Duration::from_millis(50)).await;

        unsafe extern "C" {
            fn raise(sig: std::ffi::c_int) -> std::ffi::c_int;
        }

        unsafe {
            raise(2);
        }

        tokio::time::timeout(Duration::from_secs(5), signal.wait())
            .await
            .expect("shutdown should be triggered when SIGINT is received");

        assert!(signal.is_triggered());
    }

    #[tokio::test]
    async fn test_signal_handler_exits_when_already_shutdown() {
        let shutdown = Shutdown::new();
        shutdown.trigger();
        let handle = tokio::runtime::Handle::current();

        spawn_signal_handler(shutdown, &handle);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
