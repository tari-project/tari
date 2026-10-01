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
    configuration::utils::{print_security_warning, print_warning, sanitize_for_display, untrusted_findings},
};

/// Set up application-level logging using the Log4rs configuration file specified in `config_file`. If the file does
/// not exist it is created from `default`. `{{log_dir}}` in the file is replaced with `base_path`.
///
/// Every trust finding for the file (see [`untrusted_reasons`](crate::configuration::utils::untrusted_reasons)) is
/// printed to stderr and logged once logging is up. If another (non-root) user can control the file (see
/// [`untrusted_severe_reasons`](crate::configuration::utils::untrusted_severe_reasons)), it is not loaded and `default`
/// is used instead.
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
    let choice = choose_log_config(config_file, default)?;
    for (severe, reason) in &choice.findings {
        if *severe {
            print_security_warning(reason);
        } else {
            print_warning(reason);
        }
    }
    let ignored_message = format!(
        "Untrusted log config {} ignored and its custom settings dropped; using the built-in default logging \
         configuration",
        sanitize_for_display(&config_file.display().to_string())
    );
    if choice.refused {
        print_security_warning(&ignored_message);
    }
    init_logging_from_str(&choice.contents, base_path)?;
    for (_, reason) in &choice.findings {
        warn!(target: LOG_TARGET, "⚠️  {reason}");
    }
    if choice.refused {
        warn!(target: LOG_TARGET, "⚠️  {ignored_message}");
    }
    Ok(())
}

/// The log4rs config text to use, every trust finding for the file as `(severe, reason)`, and whether the file was
/// refused.
struct LogConfigChoice {
    contents: String,
    findings: Vec<(bool, String)>,
    refused: bool,
}

/// Chooses the log4rs config text to use. A file that another (non-root) user can control (see
/// [`untrusted_severe_reasons`](crate::configuration::utils::untrusted_severe_reasons)) is not read at all and
/// `default` is used instead. Other findings (e.g. a group-writable file under a umask of 002, or a root-owned
/// read-only file) are only reported.
fn choose_log_config(config_file: &Path, default: &str) -> Result<LogConfigChoice, ConfigError> {
    // One pass decides both what is reported and whether the file is refused
    let findings = untrusted_findings(config_file);
    if findings.iter().any(|(severe, _)| *severe) {
        return Ok(LogConfigChoice {
            contents: default.to_string(),
            findings,
            refused: true,
        });
    }
    let mut file =
        File::open(config_file).map_err(|e| ConfigError::new("Could not locate file: {}", Some(e.to_string())))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| ConfigError::new("Could not read file: {}", Some(e.to_string())))?;
    Ok(LogConfigChoice {
        contents,
        findings,
        refused: false,
    })
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
        let choice = choose_log_config(&path, "default").unwrap();
        assert_eq!(choice.contents, "from file");
        assert!(choice.findings.is_empty(), "{:?}", choice.findings);
        assert!(!choice.refused);

        // Group-writable (umask 002) and owned by us: loaded, but warned about
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        let choice = choose_log_config(&path, "default").unwrap();
        assert_eq!(choice.contents, "from file");
        assert!(!choice.findings.is_empty());
        assert!(!choice.refused);

        // World-writable: the file is not read and the default is used
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        let choice = choose_log_config(&path, "default").unwrap();
        assert_eq!(choice.contents, "default");
        assert!(!choice.findings.is_empty());
        assert!(choice.refused);
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
