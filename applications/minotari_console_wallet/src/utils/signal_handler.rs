// Copyright 2026. The Tari Project
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
use tokio::{runtime::Handle, task::JoinHandle};

const LOG_TARGET: &str = "wallet::console_wallet::signal_handler";

/// Spawns a background task on the given runtime handle that listens for termination signals
/// (SIGINT, SIGTERM on Unix; Ctrl+C on Windows) and triggers the provided [`Shutdown`].
pub fn spawn_signal_handler(shutdown: Shutdown, handle: &Handle) -> JoinHandle<()> {
    let mut shutdown_signal = shutdown.to_signal();
    let _enter = handle.enter();

    #[cfg(unix)]
    let streams = {
        use tokio::signal::unix::{SignalKind, signal};
        match (signal(SignalKind::interrupt()), signal(SignalKind::terminate())) {
            (Ok(sigint), Ok(sigterm)) => Some((sigint, sigterm)),
            (Err(e), _) => {
                error!(target: LOG_TARGET, "Failed to install SIGINT handler: {e}");
                None
            },
            (_, Err(e)) => {
                error!(target: LOG_TARGET, "Failed to install SIGTERM handler: {e}");
                None
            },
        }
    };

    handle.spawn(async move {
        #[cfg(unix)]
        {
            let Some((mut sigint, mut sigterm)) = streams else {
                return;
            };

            tokio::select! {
                biased;
                _ = shutdown_signal.wait() => {
                    debug!(target: LOG_TARGET, "Shutdown already triggered elsewhere; exiting signal handler");
                    return;
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

            // Keep listening after the first signal. If a second signal arrives before the process exits,
            // immediately terminate with exit code 130 (128 + SIGINT) so a stalled `wait_until_shutdown()`
            // drain in a terminal can be interrupted.
            tokio::select! {
                _ = sigint.recv() => {
                    warn!(target: LOG_TARGET, "Second SIGINT received; forcing immediate exit");
                    std::process::exit(130);
                }
                _ = sigterm.recv() => {
                    warn!(target: LOG_TARGET, "Second SIGTERM received; forcing immediate exit");
                    std::process::exit(130);
                }
            }
        }

        #[cfg(windows)]
        {
            tokio::select! {
                biased;
                _ = shutdown_signal.wait() => {
                    debug!(target: LOG_TARGET, "Shutdown already triggered elsewhere; exiting signal handler");
                    return;
                }
                res = tokio::signal::ctrl_c() => {
                    match res {
                        Ok(()) => {
                            info!(target: LOG_TARGET, "Ctrl+C received, triggering shutdown");
                            shutdown.trigger();
                        }
                        Err(e) => {
                            error!(target: LOG_TARGET, "Error listening for Ctrl+C: {e}");
                            return;
                        }
                    }
                }
            }

            if let Ok(()) = tokio::signal::ctrl_c().await {
                warn!(target: LOG_TARGET, "Second Ctrl+C received; forcing immediate exit");
                std::process::exit(130);
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            shutdown_signal.wait().await;
        }
    })
}

#[cfg(all(test, unix))]
pub(crate) mod unix_test_signal {
    pub const SIGINT: std::ffi::c_int = 2;
    pub const SIGTERM: std::ffi::c_int = 15;

    pub fn raise_signal(sig: std::ffi::c_int) {
        unsafe extern "C" {
            fn raise(sig: std::ffi::c_int) -> std::ffi::c_int;
        }
        // SAFETY: `raise` is a standard POSIX C library function that sends `sig` to the calling thread/process,
        // where the tokio signal handler installed by `spawn_signal_handler` is already listening.
        let rc = unsafe { raise(sig) };
        assert_eq!(rc, 0, "raise({sig}) failed");
    }
}

#[cfg(all(test, unix))]
pub(crate) static SIGNAL_TEST_MUTEX: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tari_shutdown::Shutdown;

    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn test_signal_handler_triggers_shutdown_on_sigterm() {
        let _guard = SIGNAL_TEST_MUTEX.lock().await;
        let shutdown = Shutdown::new();
        let mut signal = shutdown.to_signal();
        let handle = tokio::runtime::Handle::current();

        let join_handle = spawn_signal_handler(shutdown, &handle);

        unix_test_signal::raise_signal(unix_test_signal::SIGTERM);

        tokio::time::timeout(Duration::from_secs(5), signal.wait())
            .await
            .expect("shutdown should be triggered when SIGTERM is received");

        assert!(signal.is_triggered());
        assert!(
            !join_handle.is_finished(),
            "signal handler task should remain active after the first signal to catch a second signal"
        );
        join_handle.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_signal_handler_triggers_shutdown_on_sigint() {
        let _guard = SIGNAL_TEST_MUTEX.lock().await;
        let shutdown = Shutdown::new();
        let mut signal = shutdown.to_signal();
        let handle = tokio::runtime::Handle::current();

        let join_handle = spawn_signal_handler(shutdown, &handle);

        unix_test_signal::raise_signal(unix_test_signal::SIGINT);

        tokio::time::timeout(Duration::from_secs(5), signal.wait())
            .await
            .expect("shutdown should be triggered when SIGINT is received");

        assert!(signal.is_triggered());
        assert!(
            !join_handle.is_finished(),
            "signal handler task should remain active after the first signal to catch a second signal"
        );
        join_handle.abort();
    }

    #[tokio::test]
    async fn test_signal_handler_exits_when_already_shutdown() {
        let shutdown = Shutdown::new();
        shutdown.trigger();
        let handle = tokio::runtime::Handle::current();

        let join_handle = spawn_signal_handler(shutdown, &handle);
        tokio::time::timeout(Duration::from_secs(5), join_handle)
            .await
            .expect("signal handler task should exit promptly when already shut down")
            .expect("signal handler task should not panic");
    }
}
