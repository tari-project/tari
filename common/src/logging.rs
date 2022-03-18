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

use std::{fs, fs::File, io::Write, path::Path};

use crate::{configuration::bootstrap::ApplicationType, ConfigError};

/// Set up application-level logging using the Log4rs configuration file specified in
pub fn initialize_logging(config_file: &Path, default: &str) -> Result<(), ConfigError> {
    println!(
        "Initializing logging according to {:?}",
        config_file.to_str().unwrap_or("[??]")
    );

    if !config_file.exists() {
        if let Some(d) = config_file.parent() {
            fs::create_dir_all(d)
                .map_err(|e| ConfigError::new("Could not create parent directory for log file", Some(e.to_string())))?
        };
        let mut file = File::create(config_file)
            .map_err(|e| ConfigError::new("Could not create default log file", Some(e.to_string())))?;
        file.write_all(default.as_ref())
            .map_err(|e| ConfigError::new("Could not create default log file", Some(e.to_string())))?;
    }

    log4rs::init_file(config_file, Default::default())
        .map_err(|e| ConfigError::new("Could not initialize logging", Some(e.to_string())))
}

/// Installs a new default logfile configuration, copied from the specified application type sample to the given path.
pub fn log_config_installer(application_type: ApplicationType, path: &Path) -> Result<(), std::io::Error> {
    use ApplicationType::*;
    let source = match application_type {
        BaseNode => todo!(),
        ConsoleWallet => include_str!("../logging/log4rs_sample_wallet.yml"),
        MiningNode => todo!(),
        MergeMiningProxy => todo!(),
        StratumTranscoder => include_str!("../logging/log4rs_sample_transcoder.yml"),
        ValidatorNode => include_str!("../logging/log4rs_sample_validator_node.yml"),
        Collectibles => include_str!("../logging/log4rs_collectibles.yml"),
    };

    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
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
