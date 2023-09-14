// Copyright 2023, The Tari Project
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

use std::{path::PathBuf, ptr};

use libc::c_int;
use log::{debug, warn, LevelFilter};
use log4rs::{
    append::{
        rolling_file::{
            policy::compound::{roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy},
            RollingFileAppender,
        },
        Append,
    },
    config::{Appender, Logger, Root},
    encode::pattern::PatternEncoder,
    Config,
};

const LOG_TARGET: &str = "chat_ffi::logging";

/// Inits logging, this function is deliberately not exposed externally in the header
///
/// ## Arguments
/// `log_path` - Path to where the log will be stored
/// `error_out` - Pointer to an int which will be modified to an error code should one occur, may not be null. Functions
/// as an out parameter.
///
/// ## Returns
/// `()` - Does not return a value, equivalent to void in C
///
/// # Safety
/// None
#[allow(clippy::too_many_lines)]
pub unsafe fn init_logging(log_path: PathBuf, error_out: *mut c_int) {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);

    let num_rolling_log_files = 2;
    let size_per_log_file_bytes: u64 = 10 * 1024 * 1024;

    let path = log_path.to_str().expect("Convert path to string");
    let encoder = PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S.%f)} [{t}] {l:5} {m}{n}");

    let mut pattern;
    let split_str: Vec<&str> = path.split('.').collect();
    if split_str.len() <= 1 {
        pattern = format!("{}{}", path, "{}");
    } else {
        pattern = split_str[0].to_string();
        for part in split_str.iter().take(split_str.len() - 1).skip(1) {
            pattern = format!("{}.{}", pattern, part);
        }

        pattern = format!("{}{}", pattern, ".{}.");
        pattern = format!("{}{}", pattern, split_str[split_str.len() - 1]);
    }
    let roller = FixedWindowRoller::builder()
        .build(pattern.as_str(), num_rolling_log_files)
        .expect("Should be able to create a Roller");
    let size_trigger = SizeTrigger::new(size_per_log_file_bytes);
    let policy = CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));

    let log_appender: Box<dyn Append> = Box::new(
        RollingFileAppender::builder()
            .encoder(Box::new(encoder))
            .append(true)
            .build(path, Box::new(policy))
            .expect("Should be able to create an appender"),
    );

    let lconfig = Config::builder()
        .appender(Appender::builder().build("logfile", log_appender))
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("comms", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("comms::noise", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("tokio_util", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("tracing", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("chat_ffi::callback_handler", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("chat_ffi", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("contacts", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("p2p", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("yamux", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("dht", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logfile")
                .additive(false)
                .build("mio", LevelFilter::Warn),
        )
        .build(Root::builder().appender("logfile").build(LevelFilter::Warn))
        .expect("Should be able to create a Config");

    match log4rs::init_config(lconfig) {
        Ok(_) => debug!(target: LOG_TARGET, "Logging started"),
        Err(_) => warn!(target: LOG_TARGET, "Logging has already been initialized"),
    }
}
