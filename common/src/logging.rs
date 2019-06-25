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

use std::{path::Path, sync::Once};

/// This function initializes global logging for tari applications.
///
/// The `log4rs_config_path` argument should point to the log4rs configuration file. Absolute or
/// relative paths are supported. If you call this function more than once, an error will be returned.
///
/// Logging configuration is kept in a separate file (before the main Config file) because we'll want to log messages
/// about the configuration and this won't be possible unless the logger is fully bootstrapped.
pub fn initialize_logger<P>(log4rs_config_path: P) -> Result<(), log4rs::Error>
where P: AsRef<Path> {
    log4rs::init_file(log4rs_config_path, Default::default())
}

/// Initialize the system logger for tests. This function searches up the directory tree
/// until it finds a `.git` subdirectory and then points to the `log4rs-debug.yml` in the
/// `tari_common` crate. Therefore, this function should never be used in a prebuilt binary
/// or outside of tests and examples.
///
/// Calling this function multiple times will have no effect.
pub fn initialize_logger_for_test() {
    use std::env::current_dir;
    static INIT_LOGGER: Once = Once::new();
    INIT_LOGGER.call_once(|| {
        let mut working_dir = current_dir().unwrap();
        while !working_dir.join(".git").exists() {
            if !working_dir.pop() {
                panic!("Unable to locate log4rs configuration file.");
            }
        }
        working_dir.push("common/");
        log4rs::init_file(working_dir.join("log4rs-debug.yml"), Default::default()).unwrap();
    });
}

#[cfg(test)]
mod test {
    use super::initialize_logger_for_test;
    use log::{debug, error, info, warn};
    use std::{thread, time::Duration};

    #[test]
    fn logging_from_self() {
        initialize_logger_for_test();
        debug!(target: "comms::p2p::inbound", "Demo inbound log message (to network)");
        warn!(target: "stdout", "Logging from main thread (to stdout)");
        debug!(target: "stdout", "Logging from main thread (ignored)");
        warn!(target: "base_layer::transaction", "Info on main thread (to base)");
    }

    #[test]
    fn logging_from_multi_threads() {
        initialize_logger_for_test();
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
