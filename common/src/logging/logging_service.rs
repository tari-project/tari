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

use crate::logging::log_levels::LogLevel;
use std::{
    fmt::{Display, Error, Formatter},
    sync::mpsc,
    thread,
};
use term::stderr;

/// LogMessage is a container for Tari log messages. You typically don't create these yourself; they are created by
/// the `log` functions in `Logger`
pub struct LogMessage {
    level: LogLevel,
    message: String,
}

impl Display for LogMessage {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let msg = format!("{:?} - {}", self.level, self.message);
        f.write_str(&msg)
    }
}

/// Threadsafe Tari Logging service.
///
/// Only one of these should be created per running process:
///
/// ```edition2018
/// # use tari_common::{LoggingService, LogLevel};
/// let logging_service = LoggingService::new();
///   let logger = logging_service.new_logger();
///   logger.log(LogLevel::Debug, "Test message");
/// ```
///
/// Underneath the hood, Logging service creates an mpsc channel. It holds onto the tx instance (in the form of a
/// `Logger`) which is used to create additional `Logger` instances (in other threads). Any log message send from any
/// Logger will be received and collected by `LoggingService` and forwarded to the configured Logging output channel(s).
pub struct LoggingService {
    logger: Logger,
}

impl LoggingService {
    pub fn new() -> LoggingService {
        let (tx, rx) = mpsc::channel();
        let logger = Logger { tx };
        let logging_service = LoggingService { logger };
        LoggingService::start(rx);
        logging_service
    }

    fn start(rx: mpsc::Receiver<LogMessage>) {
        thread::spawn(|| {
            for msg in rx {
                println!("{}", msg);
            }
        });
    }

    pub fn new_logger(&self) -> Logger {
        self.logger.clone()
    }

    pub fn log(&self, level: LogLevel, message: &str) {
        self.logger.log(level, message);
    }
}

#[derive(Clone)]
pub struct Logger {
    tx: mpsc::Sender<LogMessage>,
}

impl Logger {
    pub fn log(&self, level: LogLevel, message: &str) {
        let msg = LogMessage {
            level,
            message: message.into(),
        };
        self.tx.send(msg).unwrap_or_else(|_| {
            if let Some(mut stderr) = stderr() {
                let err = format!("Error: Could not write message to log: {}\n", message);
                let _ = stderr.write(err.as_bytes());
            }
        })
    }
}

#[cfg(test)]
mod test {
    use crate::logging::{
        log_levels::LogLevel,
        logging_service::LoggingService,
    };
    use std::thread;

    #[test]
    fn logging_from_self() {
        let logging_service = LoggingService::new();
        logging_service.log(LogLevel::Debug, "Logging from self");
    }

    #[test]
    fn logging_from_one_thread() {
        let logging_service = LoggingService::new();
        let logger_1 = logging_service.new_logger();
        let logger_2 = logging_service.new_logger();
        logger_1.log(LogLevel::Debug, "Logging from main thread 1");
        logger_2.log(LogLevel::Warn, "Logging from main thread 2");
    }

    #[test]
    fn logging_from_multi_threads() {
        let logging_service = LoggingService::new();
        let logger_1 = logging_service.new_logger();
        thread::spawn(move || {
            logger_1.log(LogLevel::Debug, "Hi from thread 1");
        });

        let logger_2 = logging_service.new_logger();
        thread::spawn(move || {
            logger_2.log(LogLevel::Debug, "Hi from thread 2");
        });
    }
}
