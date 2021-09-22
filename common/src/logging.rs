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

// use log::LevelFilter;
// use simplelog::*;
use std::{fs, fs::File, io::Write, path::Path};

/// Set up application-level logging using the Log4rs configuration file specified in
pub fn initialize_logging(config_file: &Path, base_path: &Path) -> bool {
    println!(
        "Initializing logging according to {:?}",
        config_file.to_str().unwrap_or("[??]")
    );

    let current_working_dir = std::env::current_dir().unwrap_or_default();

    if std::env::set_current_dir(&base_path).is_err() {
        println!(
            "Logging initialized in {}, could not initialize in {}.",
            &current_working_dir.display(),
            &base_path.display()
        );
    };

    if let Err(e) = log4rs::init_file(config_file, Default::default()) {
        println!("We couldn't load a logging configuration file. {}", e.to_string());
        return false;
    }

    if std::env::set_current_dir(&current_working_dir).is_err() {
        println!(
            "Working directory could not be changed back to {} after logging has been initialized. New working \
             directory is {}",
            &current_working_dir.display(),
            &std::env::current_dir().unwrap_or_default().display()
        );
    };

    // simplelog config - perhaps for future use
    // let config = ConfigBuilder::new()
    //     .set_thread_level(LevelFilter::Error)
    //     .set_time_to_local(true)
    //     .set_time_format_str("%H:%M")
    //     .build();
    //
    // let network_config = ConfigBuilder::new()
    //     .set_thread_level(LevelFilter::Error)
    //     .set_time_to_local(true)
    //     .build();
    //
    // let log_level = env::var("RUST_LOG").unwrap_or("Info".to_string());
    // CombinedLogger::init(vec![
    //     TermLogger::new(
    //         LevelFilter::Warn,
    //         ConfigBuilder::new()
    //             .set_thread_level(LevelFilter::Error)
    //             .set_time_to_local(true)
    //             .set_time_format_str("%H:%M")
    //             .build(),
    //         TerminalMode::Mixed,
    //         ColorChoice::Auto,
    //     ),
    //     WriteLogger::new(
    //         LevelFilter::from_str(log_level.as_str()).unwrap_or(LevelFilter::Info),
    //         ConfigBuilder::new()
    //             .set_thread_level(LevelFilter::Error)
    //             .add_filter_ignore_str("comms")
    //             .add_filter_ignore_str("p2p")
    //             .add_filter_ignore_str("yamux")
    //             .add_filter_ignore_str("mio")
    //             .build(),
    //         File::create("log/log.log").unwrap(),
    //     ),
    //     WriteLogger::new(
    //         LevelFilter::from_str(log_level.as_str()).unwrap_or(LevelFilter::Info),
    //         ConfigBuilder::new()
    //             .set_thread_level(LevelFilter::Error)
    //             .add_filter_allow_str("comms")
    //             .add_filter_allow_str("p2p")
    //             .add_filter_allow_str("yamux")
    //             .add_filter_allow_str("mio")
    //             .build(),
    //         File::create("log/network.log").unwrap(),
    //     ),
    // ])
    // .unwrap();

    true
}

/// Installs a new default logfile configuration, copied from `log4rs_sample_base_node.yml` to the given path.
pub fn install_default_base_node_logfile_config(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../logging/log4rs_sample_base_node.yml");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    // Note: `fs::write(path, source)` did not work as expected, as the file name was not changed
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

/// Installs a new default logfile configuration, copied from `log4rs_sample_wallet.yml` to the given path.
pub fn install_default_wallet_logfile_config(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../logging/log4rs_sample_wallet.yml");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    // Note: `fs::write(path, source)` did not work as expected, as the file name was not changed
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

/// Installs a new default logfile configuration, copied from `log4rs_sample_proxy.yml` to the given path.
pub fn install_default_merge_mining_proxy_logfile_config(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../logging/log4rs_sample_proxy.yml");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    // Note: `fs::write(path, source)` did not work as expected, as the file name was not changed
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

/// Installs a new default logfile configuration, copied from `log4rs_sample_transcoder.yml` to the given path.
pub fn install_default_stratum_transcoder_logfile_config(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../logging/log4rs_sample_transcoder.yml");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    // Note: `fs::write(path, source)` did not work as expected, as the file name was not changed
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

/// Installs a new default logfile configuration, copied from `log4rs_sample_wallet.yml` to the given path.
pub fn install_default_mining_node_logfile_config(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../logging/log4rs_sample_mining_node.yml");
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    // Note: `fs::write(path, source)` did not work as expected, as the file name was not changed
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

/// Log an error if an `Err` is returned from the `$expr`. If the given expression is `Ok(v)`,
/// `Some(v)` is returned, otherwise `None` is returned (same as `Result::ok`).
/// Useful in cases where the error should be logged and ignored.
/// instead of writing `if let Err(err) = my_error_call() { error!(...) }`, you can write
/// `log_if_error!(my_error_call())`
///
/// ```edition2018
/// # use tari_common::log_if_error;
/// let opt = log_if_error!(level: debug, target: "docs", "Error sending reply: {}", Result::<(), _>::Err("this will be logged"));
/// assert_eq!(opt, None);
/// ```
#[macro_export]
macro_rules! log_if_error {
    (level:$level:tt, target: $target:expr, $msg:expr, $expr:expr $(,)*) => {{
        match $expr {
            Ok(v) => Some(v),
            Err(err) => {
                log::$level!(target: $target, $msg, err);
                None
            }
        }
    }};
    (level:$level:tt, $msg:expr, $expr:expr $(,)*) => {{
        log_if_error!(level:$level, target: "$crate", $msg, $expr)
    }};
     (target: $target:expr, $msg:expr, $expr:expr $(,)*) => {{
        log_if_error!(level:warn, target: $target, $msg, $expr)
    }};
    ($msg:expr, $expr:expr $(,)*) => {{
        log_if_error!(level:warn, target: "$crate", $msg, $expr)
    }};
}

/// See [log_if_error!](./log_if_error.macro.html).
///
/// ```edition2018
/// # use tari_common::log_if_error_fmt;
/// let opt = log_if_error_fmt!(level: debug, target: "docs", "Error sending reply - custom: {}", Result::<(), _>::Err(()), "this is logged");
/// assert_eq!(opt, None);
/// ```
#[macro_export]
macro_rules! log_if_error_fmt {
    (level: $level:tt, target: $target:expr, $msg:expr, $expr:expr, $($args:tt)+) => {{
        match $expr {
            Ok(v) => Some(v),
            Err(_) => {
                log::$level!(target: $target, $msg, $($args)+);
                None
            }
        }
    }};
}

#[cfg(test)]
mod test {
    #[test]
    fn log_if_error() {
        let err = Result::<(), _>::Err("What a shame");
        let opt = log_if_error!("Error: {}", err);
        assert!(opt.is_none());

        let opt = log_if_error!(level: trace, "Error: {}", err);
        assert!(opt.is_none());

        let opt = log_if_error!(level: trace, "Error: {}", Result::<_, &str>::Ok("answer"));
        assert_eq!(opt, Some("answer"));
    }
}
