// Copyright 2019. The Tari Project
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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::sync::Once;

/// Initialize the system logger. This function initializes and exposes the global logging instance. You can call
/// this function multiple times; additional calls will have no effect.
///
/// The logger is configured by the `log4rs.yml` file (release mode), or `log4rs-debug.yml` (debug mode). Obviously,
/// when binaries are released, users will be able to configure the logging in any way they see fit by editing the
/// (usually) `log4rs.yml` file.
///
/// Logging configuration is kept in a separate file (before the main Config file) because we'll want to log messages
/// about the configuration and this won't be possible unless the logger is fully bootstrapped.
pub fn initialize_logger() {
    static INIT_LOGGER: Once = Once::new();
    INIT_LOGGER.call_once(|| {
        #[cfg(debug_assertions)]
        log4rs::init_file("log4rs-debug.yml", Default::default()).unwrap();
        #[cfg(not(debug_assertions))]
        log4rs::init_file("log4rs.yml", Default::default()).unwrap();
    });
}

#[cfg(test)]
mod test {
    use super::initialize_logger;
    use log::{debug, error, info, warn};
    use std::{thread, time::Duration};

    #[test]
    fn logging_from_self() {
        initialize_logger();
        debug!(target: "comms::p2p::inbound", "Demo inbound log message (to network)");
        warn!(target: "stdout", "Logging from main thread (to stdout)");
        debug!(target: "stdout", "Logging from main thread (ignored)");
        warn!(target: "base_layer::transaction", "Info on main thread (to base)");
    }

    #[test]
    fn logging_from_multi_threads() {
        initialize_logger();
        thread::spawn(move || {
            warn!(target: "stdout", "Hi from thread A (to stdout)");
            debug!("Default message (to base)");
            warn!(target: "comms::message", "Thread A message (to network)");
            info!(target: "base_layer::blocks", "Thread A message (to base - ignored)");
        });

        thread::spawn(move || {
            debug!(target: "stdout", "Hi from thread B (ignored)");
            error!(target: "stdout", "Hi from thread B (to stdout)");
            error!(target: "base", "Error from thread B (to base)");
            error!(target: "comms", "Error from thread B (to netowrk)");
        });

        thread::sleep(Duration::from_millis(100));
    }
}
