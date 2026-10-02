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
    path::{Component, Path, PathBuf},
};

use log::warn;
use log4rs::config::RawConfig;

use crate::{
    ConfigError,
    LOG_TARGET,
    configuration::utils::{
        file_findings,
        print_security_warning,
        print_warning,
        sanitize_for_display,
        untrusted_path_findings,
    },
};

/// Set up application-level logging using the Log4rs configuration file specified in `config_file`. If the file does
/// not exist it is created from `default`. `{{log_dir}}` in the file is replaced with `base_path`.
///
/// Every trust finding for the file (see [`untrusted_reasons`](crate::configuration::utils::untrusted_reasons)) is
/// printed to stderr and logged once logging is up. If another (non-root) user can control the file (see
/// [`untrusted_severe_reasons`](crate::configuration::utils::untrusted_severe_reasons)), or one of its appenders
/// writes outside `base_path`, it is not loaded and `default` is used instead.
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
    let choice = choose_log_config(config_file, base_path, default)?;
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
    init_logging_from_yaml(&choice.contents)?;
    for (_, reason) in &choice.findings {
        warn!(target: LOG_TARGET, "⚠️  {reason}");
    }
    if choice.refused {
        warn!(target: LOG_TARGET, "⚠️  {ignored_message}");
    }
    Ok(())
}

/// The log4rs config to use (with `{{log_dir}}` already substituted), every finding about the file as
/// `(severe, reason)`, and whether the file was refused in favour of the built-in default.
struct LogConfigChoice {
    contents: String,
    findings: Vec<(bool, String)>,
    refused: bool,
}

/// Chooses the log4rs config to use. The file is refused, and `default` used instead, if:
/// - another (non-root) user can control it or a directory on its path (severe findings, see
///   [`untrusted_severe_reasons`]); the file's own owner and mode are taken from the open descriptor, so the file that
///   is checked is the file that is read, or
/// - any appender writes outside `base_path` (see [`escaping_appender_paths`]).
///
/// Other findings (e.g. a file writable only by the user's private group, or a root-owned read-only file) are only
/// reported. The built-in default is never refused. `base_path` is made absolute and cleaned (so `..` in it is
/// resolved) before it is substituted for `{{log_dir}}`.
///
/// [`untrusted_severe_reasons`]: crate::configuration::utils::untrusted_severe_reasons
fn choose_log_config(config_file: &Path, base_path: &Path, default: &str) -> Result<LogConfigChoice, ConfigError> {
    let base = LogDirBase::new(base_path)?;
    let path_findings = untrusted_path_findings(config_file);
    let mut findings = Vec::new();
    let mut file_contents = None;
    if path_findings.iter().any(|(severe, _)| *severe) {
        findings = path_findings;
    } else {
        let (mut file, metadata) = open_log_config(config_file)?;
        findings.extend(file_findings(config_file, &metadata));
        findings.extend(path_findings);
        if !findings.iter().any(|(severe, _)| *severe) {
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|e| ConfigError::new("Could not read file: {}", Some(e.to_string())))?;
            file_contents = Some(contents);
        }
    }

    if let Some(contents) = file_contents {
        let contents = substitute_log_dir(&contents, &base.lexical)?;
        let escapes = escaping_appender_paths(&contents, &base)?;
        if escapes.is_empty() {
            return Ok(LogConfigChoice {
                contents,
                findings,
                refused: false,
            });
        }
        findings.extend(escapes.into_iter().map(|reason| (true, reason)));
    }

    // The built-in default is compiled in and only uses `{{log_dir}}`-relative paths, so it is not checked: refusing it
    // would leave nothing to start with
    let contents = substitute_log_dir(default, &base.lexical)?;
    Ok(LogConfigChoice {
        contents,
        findings,
        refused: true,
    })
}

/// Opens the log config for reading. On Unix the symlink chain is resolved first and the resolved target is opened
/// with `O_NOFOLLOW`, so the descriptor (and its metadata) belongs to a regular file, not a link swapped in later.
#[cfg(unix)]
fn open_log_config(config_file: &Path) -> Result<(File, fs::Metadata), ConfigError> {
    use std::os::unix::fs::OpenOptionsExt;

    let target = fs::canonicalize(config_file)
        .map_err(|e| ConfigError::new("Could not locate file: {}", Some(e.to_string())))?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&target)
        .map_err(|e| ConfigError::new("Could not open file: {}", Some(e.to_string())))?;
    let metadata = file
        .metadata()
        .map_err(|e| ConfigError::new("Could not read file metadata: {}", Some(e.to_string())))?;
    Ok((file, metadata))
}

#[cfg(not(unix))]
fn open_log_config(config_file: &Path) -> Result<(File, fs::Metadata), ConfigError> {
    let file =
        File::open(config_file).map_err(|e| ConfigError::new("Could not locate file: {}", Some(e.to_string())))?;
    let metadata = file
        .metadata()
        .map_err(|e| ConfigError::new("Could not read file metadata: {}", Some(e.to_string())))?;
    Ok((file, metadata))
}

/// The log directory in the two forms used for containment checks.
struct LogDirBase {
    /// Absolute: on Unix the canonical path if it exists (so `..` after a symlink resolves the way the OS does),
    /// otherwise lexically normalized (see [`lexical_normalize`]). This is what `{{log_dir}}` is replaced with.
    lexical: PathBuf,
    /// Fully resolved (see [`resolve_for_containment`]), if possible.
    canonical: Option<PathBuf>,
}

impl LogDirBase {
    fn new(base_path: &Path) -> Result<Self, ConfigError> {
        if base_path.to_str().is_none() {
            return Err(ConfigError::new(
                "Could not replace {{log_dir}} variable from the log4rs config",
                Some(format!("base path {} is not valid UTF-8", base_path.display())),
            ));
        }
        let absolute = if base_path.is_absolute() {
            base_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|dir| dir.join(base_path))
                .unwrap_or_else(|_| base_path.to_path_buf())
        };
        // On Windows `canonicalize` returns a `\\?\` verbatim path, which log4rs cannot use, so only resolve on Unix
        let canonical_base = if cfg!(unix) {
            fs::canonicalize(&absolute).ok()
        } else {
            None
        };
        let lexical = canonical_base.unwrap_or_else(|| lexical_normalize(&absolute));
        let canonical = resolve_for_containment(&lexical);
        Ok(Self { lexical, canonical })
    }
}

/// Normalizes a path without touching the file system, using the platform's own path rules (`Path::components`):
/// `.` is dropped and `..` removes the previous normal segment. A prefix (drive, UNC share or verbatim prefix) and the
/// root are kept and never popped past; a `..` at the start of a relative path is kept. So `\\` is a separator only
/// on Windows, and `\\server\share\..` stays at the share root.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let mut rooted = false;
    let mut normal_segments = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                result.push(component.as_os_str());
                rooted = true;
            },
            Component::CurDir => {},
            Component::ParentDir => {
                if normal_segments > 0 {
                    result.pop();
                    normal_segments = normal_segments.saturating_sub(1);
                } else if !rooted {
                    result.push("..");
                } else {
                    // `..` at the root (or share root) stays there
                }
            },
            Component::Normal(segment) => {
                result.push(segment);
                normal_segments = normal_segments.saturating_add(1);
            },
        }
    }
    result
}

/// Returns one message per log4rs appender path that is outside the log directory: each appender's `path` and its
/// rolling policy's `policy.roller.pattern`. A path is outside if it contains `$ENV{` or `${` (log4rs expands
/// environment variables at build time, so the raw string says nothing about where it writes), or has a `..`
/// component. Otherwise it is inside if its parent directory, made absolute against the current directory (as log4rs
/// does) and cleaned, starts with the log directory, or if the parent's canonical form (or that of its nearest
/// existing ancestor) starts with the log directory's canonical form. The lexical check keeps `{{log_dir}}/log/...`
/// valid when the operator made `log` a symlink to another disk. `contents` must already have `{{log_dir}}`
/// substituted.
fn escaping_appender_paths(contents: &str, base: &LogDirBase) -> Result<Vec<String>, ConfigError> {
    let value: serde_yaml::Value = serde_yaml::from_str(contents).map_err(|e| {
        ConfigError::new(
            "Could not parse the contents of the log file as yaml",
            Some(e.to_string()),
        )
    })?;
    let mut escapes = Vec::new();
    let Some(appenders) = value.get("appenders").and_then(|a| a.as_mapping()) else {
        return Ok(escapes);
    };
    for (name, appender) in appenders {
        let name = name.as_str().unwrap_or("?");
        let mut paths = Vec::new();
        if let Some(path) = appender.get("path").and_then(|p| p.as_str()) {
            paths.push(path);
        }
        if let Some(pattern) = appender
            .get("policy")
            .and_then(|p| p.get("roller"))
            .and_then(|r| r.get("pattern"))
            .and_then(|p| p.as_str())
        {
            paths.push(pattern);
        }
        for path in paths {
            if !is_inside_log_dir(path, base) {
                escapes.push(format!(
                    "Log appender '{}' writes to {}, outside the log directory {}",
                    sanitize_for_display(name),
                    sanitize_for_display(path),
                    sanitize_for_display(&base.lexical.display().to_string())
                ));
            }
        }
    }
    Ok(escapes)
}

/// See [`escaping_appender_paths`].
fn is_inside_log_dir(path: &str, base: &LogDirBase) -> bool {
    if path.contains("$ENV{") || path.contains("${") {
        return false;
    }
    // Reject `..` whichever separator it hides behind, so no platform difference in what counts as a separator can
    // be used to climb out
    if path.split(['/', '\\']).any(|segment| segment == "..") {
        return false;
    }
    let path = Path::new(path);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let absolute = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(parent),
            Err(_) => return false,
        }
    };
    if lexical_normalize(&absolute).starts_with(&base.lexical) {
        return true;
    }
    match (&base.canonical, resolve_for_containment(&absolute)) {
        (Some(base), Some(resolved)) => resolved.starts_with(base),
        _ => false,
    }
}

/// Returns an absolute, canonical form of `path` for containment checks: relative paths are joined to the current
/// directory, the nearest existing ancestor is canonicalized and the not-yet-existing rest is appended. Returns `None`
/// if the path has a `..` component or cannot be resolved.
fn resolve_for_containment(path: &Path) -> Option<PathBuf> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    for ancestor in absolute.ancestors() {
        if let Ok(canonical) = fs::canonicalize(ancestor) {
            let rest = absolute.strip_prefix(ancestor).ok()?;
            return Some(canonical.join(rest));
        }
    }
    None
}

/// Starts log4rs with a YAML config that already has `{{log_dir}}` substituted.
fn init_logging_from_yaml(contents: &str) -> Result<(), ConfigError> {
    let config: RawConfig = serde_yaml::from_str(contents).map_err(|e| {
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
    use std::path::{Path, PathBuf};

    use super::substitute_log_dir;

    #[cfg(unix)]
    #[test]
    fn untrusted_log_config_is_ignored() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        use super::choose_log_config;
        use crate::configuration::utils::is_private_group;

        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("log4rs.yml");
        std::fs::write(&path, "from file").unwrap();

        // Trusted: the file is loaded (read through the checked descriptor)
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let choice = choose_log_config(&path, dir.path(), "default").unwrap();
        assert_eq!(choice.contents, "from file");
        assert!(choice.findings.is_empty(), "{:?}", choice.findings);
        assert!(!choice.refused);

        // Group-writable: loaded (and warned about) only if the group is the user's private group
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        let private = is_private_group(std::fs::metadata(&path).unwrap().gid());
        let choice = choose_log_config(&path, dir.path(), "default").unwrap();
        assert!(!choice.findings.is_empty());
        assert_eq!(choice.refused, !private, "{:?}", choice.findings);

        // World-writable: the file is not read and the default is used
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        let choice = choose_log_config(&path, dir.path(), "default").unwrap();
        assert_eq!(choice.contents, "default");
        assert!(!choice.findings.is_empty());
        assert!(choice.refused);
    }

    #[cfg(unix)]
    #[test]
    fn log_config_writing_outside_the_log_dir_is_refused() {
        use std::os::unix::fs::PermissionsExt;

        use super::choose_log_config;

        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("log4rs.yml");
        let default = "appenders:\n  stdout:\n    kind: console\n";

        std::fs::write(
            &path,
            "appenders:\n  evil:\n    kind: file\n    path: /tmp/evil\nroot:\n  appenders: [evil]\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let choice = choose_log_config(&path, dir.path(), default).unwrap();
        assert!(choice.refused);
        assert_eq!(choice.contents, default);
        assert!(choice.findings.iter().any(|(severe, r)| *severe && r.contains("evil")));

        std::fs::write(
            &path,
            "appenders:\n  good:\n    kind: rolling_file\n    path: \"{{log_dir}}/log/app.log\"\n    policy:\n      \
             kind: compound\n      roller:\n        kind: fixed_window\n        pattern: \
             \"{{log_dir}}/log/app.{}.log\"\n",
        )
        .unwrap();
        let choice = choose_log_config(&path, dir.path(), default).unwrap();
        assert!(!choice.refused, "{:?}", choice.findings);

        // Environment variables are expanded by log4rs, so they are never trusted
        std::fs::write(
            &path,
            "appenders:\n  env:\n    kind: file\n    path: \"$ENV{HOME}/.bashrc\"\n",
        )
        .unwrap();
        let choice = choose_log_config(&path, dir.path(), default).unwrap();
        assert!(choice.refused);
        std::fs::write(
            &path,
            "appenders:\n  env:\n    kind: file\n    path: \"${HOME}/x.log\"\n",
        )
        .unwrap();
        let choice = choose_log_config(&path, dir.path(), default).unwrap();
        assert!(choice.refused);

        // {{log_dir}}/../x is refused
        std::fs::write(
            &path,
            "appenders:\n  up:\n    kind: file\n    path: \"{{log_dir}}/../x.log\"\n",
        )
        .unwrap();
        let choice = choose_log_config(&path, dir.path(), default).unwrap();
        assert!(choice.refused);

        // A rolling pattern that escapes is refused too
        std::fs::write(
            &path,
            "appenders:\n  sneaky:\n    kind: rolling_file\n    path: \"{{log_dir}}/log/app.log\"\n    policy:\n      \
             roller:\n        pattern: \"{{log_dir}}/../../elsewhere/app.{}.log\"\n",
        )
        .unwrap();
        let choice = choose_log_config(&path, dir.path(), default).unwrap();
        assert!(choice.refused);
    }

    #[test]
    fn lexical_normalize_uses_platform_path_rules() {
        use super::lexical_normalize;

        let norm = |p: &str| lexical_normalize(Path::new(p));
        assert_eq!(norm("/a/./b/../c"), PathBuf::from("/a/c"));
        assert_eq!(norm("/../a"), PathBuf::from("/a"));
        assert_eq!(norm("a/../../b"), PathBuf::from("../b"));
        // On Unix `\` is an ordinary character, not a separator
        #[cfg(unix)]
        assert_eq!(norm("/tmp/q\\..\\x"), PathBuf::from("/tmp/q\\..\\x"));
    }

    #[cfg(windows)]
    #[test]
    fn lexical_normalize_keeps_windows_prefixes() {
        use super::lexical_normalize;

        let norm = |p: &str| lexical_normalize(Path::new(p));
        assert_eq!(norm(r"C:\cwd\..\data"), PathBuf::from(r"C:\data"));
        assert_eq!(norm(r"C:\cwd\.\data\logs"), PathBuf::from(r"C:\cwd\data\logs"));
        assert_eq!(norm(r"\\server\share\a\..\b"), PathBuf::from(r"\\server\share\b"));
        assert_eq!(norm(r"\\server\share\.."), PathBuf::from(r"\\server\share\"));
    }

    #[cfg(unix)]
    #[test]
    fn backslash_dotdot_cannot_climb_out_of_the_log_dir() {
        use super::{LogDirBase, is_inside_log_dir};

        let base = LogDirBase::new(Path::new("/home/u/.tari/mainnet")).unwrap();
        assert!(!is_inside_log_dir(
            "/tmp/q\\..\\..\\home\\u\\.tari\\mainnet/evil",
            &base
        ));
        assert!(is_inside_log_dir("/home/u/.tari/mainnet/log/app.log", &base));
    }

    #[cfg(unix)]
    #[test]
    fn base_path_resolves_dotdot_after_a_symlink_physically() {
        use std::os::unix::fs::symlink;

        use super::LogDirBase;

        // `<dir>/a/link/../b` with `link -> <dir>/x/y` is `<dir>/x/b` to the OS, not `<dir>/a/b`
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::create_dir_all(dir.path().join("x/y")).unwrap();
        std::fs::create_dir_all(dir.path().join("x/b")).unwrap();
        symlink(dir.path().join("x/y"), dir.path().join("a/link")).unwrap();
        let base = LogDirBase::new(&dir.path().join("a/link/../b")).unwrap();
        assert_eq!(base.lexical, std::fs::canonicalize(dir.path().join("x/b")).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_base_path_is_an_error() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

        use super::{LogDirBase, choose_log_config};

        let bad = Path::new(OsStr::from_bytes(b"/tmp/log\xff"));
        let err = LogDirBase::new(bad).err().expect("an error");
        assert!(err.to_string().contains("not valid UTF-8"), "{err}");
        assert!(choose_log_config(Path::new("/nonexistent/log4rs.yml"), bad, "default").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_log_dir_and_dotdot_base_path_are_accepted() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        use super::choose_log_config;

        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let base = dir.path().join("base");
        let other_disk = dir.path().join("other_disk");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&other_disk).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
        // `<base>/log` is the operator's own symlink to another disk
        symlink(&other_disk, base.join("log")).unwrap();

        let path = base.join("log4rs.yml");
        let yaml = "appenders:\n  app:\n    kind: file\n    path: \"{{log_dir}}/log/app.log\"\n";
        std::fs::write(&path, yaml).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let choice = choose_log_config(&path, &base, yaml).unwrap();
        assert!(!choice.refused, "{:?}", choice.findings);

        // A base path with `..` in it
        let dotted = base.join("..").join("base");
        let choice = choose_log_config(&path, &dotted, yaml).unwrap();
        assert!(!choice.refused, "{:?}", choice.findings);

        // The default is used, not refused, even when the user's file is
        std::fs::write(&path, "appenders:\n  evil:\n    kind: file\n    path: /tmp/evil\n").unwrap();
        let choice = choose_log_config(&path, &dotted, yaml).unwrap();
        assert!(choice.refused);
        assert!(choice.contents.contains("/log/app.log"));
    }

    #[test]
    fn embedded_default_log_configs_stay_in_the_log_dir() {
        use super::{LogDirBase, escaping_appender_paths, substitute_log_dir};

        let dir = tempfile::tempdir().unwrap();
        let base = LogDirBase::new(dir.path()).unwrap();
        for (name, yaml) in [
            (
                "node",
                include_str!("../../applications/minotari_node/log4rs_sample.yml"),
            ),
            (
                "wallet",
                include_str!("../../applications/minotari_console_wallet/log4rs_sample.yml"),
            ),
            (
                "mm proxy",
                include_str!("../../applications/minotari_merge_mining_proxy/log4rs_sample.yml"),
            ),
            (
                "miner",
                include_str!("../../applications/minotari_miner/log4rs_sample.yml"),
            ),
            (
                "peer sync",
                include_str!("../../applications/minotari_peer_sync/log4rs_sample.yml"),
            ),
            ("cucumber", include_str!("../../integration_tests/log4rs/cucumber.yml")),
        ] {
            let contents = substitute_log_dir(yaml, &base.lexical).unwrap();
            let escapes = escaping_appender_paths(&contents, &base).unwrap();
            assert!(escapes.is_empty(), "{name}: {escapes:?}");
        }
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
