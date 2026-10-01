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

use std::{
    fs,
    fs::{File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::Path,
};

use log::warn;
use log4rs::config::RawConfig;

use crate::{
    ConfigError,
    LOG_TARGET,
    configuration::utils::{print_warning, sanitize_for_display, untrusted_reasons},
};

/// Set up application-level logging using the Log4rs configuration file specified in `config_file`. If the file does
/// not exist it is created from `default`. `{{log_dir}}` in the file is replaced with `base_path`.
///
/// If the file could have been written by another user (see [`untrusted_reasons`]), it is not loaded: the reasons are
/// printed to stderr, `default` is used instead, and the reasons are also logged once logging is up.
pub fn initialize_logging(config_file: &Path, base_path: &Path, default: &str) -> Result<(), ConfigError> {
    println!(
        "Initializing logging according to {:?}",
        config_file.to_str().unwrap_or("[??]")
    );

    if !config_file.exists() {
        if let Some(d) = config_file.parent() {
            fs::create_dir_all(d)
                .map_err(|e| ConfigError::new("Could not create parent directory for log file", Some(e.to_string())))?
        };
        // `create_new` never follows or truncates a file (or symlink) that appeared after the `exists` check. If one
        // did appear, it is treated as an existing file below.
        match OpenOptions::new().write(true).create_new(true).open(config_file) {
            Ok(mut file) => file
                .write_all(default.as_ref())
                .map_err(|e| ConfigError::new("Could not create default log file", Some(e.to_string())))?,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {},
            Err(e) => {
                return Err(ConfigError::new(
                    "Could not create default log file",
                    Some(e.to_string()),
                ));
            },
        }
    }

    // Always print the findings to stderr first: no logger exists yet, and the log config being checked could itself
    // silence them. Once logging is up they are also logged.
    let (contents, untrusted) = choose_log_config(config_file, default)?;
    for reason in &untrusted {
        print_warning(reason);
    }
    if !untrusted.is_empty() {
        print_warning("Untrusted log config ignored; using the built-in default logging configuration");
    }
    init_logging_from_str(&contents, base_path)?;
    for reason in &untrusted {
        warn!(target: LOG_TARGET, "⚠️  {reason}");
    }
    if !untrusted.is_empty() {
        warn!(
            target: LOG_TARGET,
            "⚠️  Untrusted log config {} ignored; using the built-in default logging configuration",
            sanitize_for_display(&config_file.display().to_string())
        );
    }
    Ok(())
}

/// Returns the log4rs config text to use and the reasons the file at `config_file` cannot be trusted. An untrusted
/// file (see [`untrusted_reasons`]) is not read at all; `default` is used instead.
fn choose_log_config(config_file: &Path, default: &str) -> Result<(String, Vec<String>), ConfigError> {
    let untrusted = untrusted_reasons(config_file);
    if !untrusted.is_empty() {
        return Ok((default.to_string(), untrusted));
    }
    let mut file =
        File::open(config_file).map_err(|e| ConfigError::new("Could not locate file: {}", Some(e.to_string())))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| ConfigError::new("Could not read file: {}", Some(e.to_string())))?;
    Ok((contents, untrusted))
}

/// Substitutes `{{log_dir}}` in a log4rs config and starts log4rs with it.
fn init_logging_from_str(contents: &str, base_path: &Path) -> Result<(), ConfigError> {
    let contents = substitute_log_dir(contents, base_path)?;

    let config: RawConfig = serde_yaml::from_str(&contents).map_err(|e| {
        ConfigError::new(
            "Could not parse the contents of the log file as yaml",
            Some(e.to_string()),
        )
    })?;
    log4rs::init_raw_config(config)
        .map_err(|e| ConfigError::new("Could not initialize logging", Some(e.to_string())))?;

    Ok(())
}

/// Replaces `{{log_dir}}` in the log4rs config with `base_path`. The placeholder sits inside double-quoted YAML
/// scalars, so the path must not be able to end the scalar early: backslashes are turned into `/` (log4rs wants unix
/// paths anyway) and `"` is escaped as `\"`.
fn substitute_log_dir(contents: &str, base_path: &Path) -> Result<String, ConfigError> {
    let replace_str = base_path
        .to_str()
        .ok_or_else(|| {
            ConfigError::new(
                "Could not replace {{log_dir}} variable from the log4rs config",
                Some(format!("base path {} is not valid UTF-8", base_path.display())),
            )
        })?
        // log4rs requires the path to be in a unix format regardless of the system it's running on
        .replace('\\', "/")
        .replace('"', "\\\"");

    Ok(contents.replace("{{log_dir}}", &replace_str))
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
    use std::path::Path;

    use super::substitute_log_dir;

    #[cfg(unix)]
    #[test]
    fn untrusted_log_config_is_ignored() {
        use std::os::unix::fs::PermissionsExt;

        use super::choose_log_config;

        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("log4rs.yml");
        std::fs::write(&path, "from file").unwrap();

        // Trusted: the file is loaded
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (contents, reasons) = choose_log_config(&path, "default").unwrap();
        assert_eq!(contents, "from file");
        assert!(reasons.is_empty(), "{reasons:?}");

        // World-writable: the file is not read and the default is used
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        let (contents, reasons) = choose_log_config(&path, "default").unwrap();
        assert_eq!(contents, "default");
        assert!(!reasons.is_empty());
    }

    #[test]
    fn log_dir_is_escaped_for_yaml() {
        let yaml = "path: \"{{log_dir}}/log/base_node.log\"\n";
        let contents = substitute_log_dir(yaml, Path::new("/tmp/we\"ird\\dir")).unwrap();
        let value: serde_yaml::Value = serde_yaml::from_str(&contents).unwrap();
        assert_eq!(
            value.get("path").and_then(|v| v.as_str()).unwrap(),
            "/tmp/we\"ird/dir/log/base_node.log"
        );
    }

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
