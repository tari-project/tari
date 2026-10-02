// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt,
    fmt::Display,
    fs,
    fs::OpenOptions,
    io::{ErrorKind, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use config::{Config, ValueKind};
use log::{debug, info, trace, warn};
use serde::{
    Deserialize,
    Deserializer,
    Serializer,
    de::{self, MapAccess, Visitor},
};

use crate::{
    ConfigError,
    LOG_TARGET,
    configuration::{
        ConfigOverrideProvider,
        Network,
        bootstrap::prompt,
        loader::{CONFIG_OVERRIDES_KEY, is_valid_config_key},
    },
    network_check::set_network_if_choice_valid,
};

//-------------------------------------           Main API functions         --------------------------------------//

/// Loads the configuration file from the specified path, or creates a new one with the embedded default presets if it
/// does not. This also prompts the user.
pub fn load_configuration<P: AsRef<Path>, TOverride: ConfigOverrideProvider>(
    config_path: P,
    create_if_not_exists: bool,
    non_interactive: bool,
    overrides: &TOverride,
    cli_network: Option<Network>,
) -> Result<Config, ConfigError> {
    if config_path.as_ref().exists() {
        debug!(
            target: LOG_TARGET,
            "Using existing configuration file  {}",
            display_path(config_path.as_ref())
        );
    } else if create_if_not_exists {
        let sources = if non_interactive {
            get_default_config(false)
        } else {
            prompt_default_config()
        };
        write_config_to(&config_path, &sources)
            .map_err(|io| ConfigError::new("Could not create default config", Some(io.to_string())))?;
    } else {
        // Nothing here
    }

    load_configuration_with_overrides(config_path, overrides, cli_network)
}

/// Loads the config at the given path applying all overrides.
///
/// Precedence, lowest to highest:
/// 1. unscoped keys in the config file (e.g. `[base_node]`),
/// 2. `TARI_*` environment variables,
/// 3. `-p` and application-injected overrides,
/// 4. the network-scoped table selected by `<section>.override_from` (e.g. `[mainnet.base_node]`), merged later by
///    [`ConfigPath::merge_subconfig`](crate::ConfigPath::merge_subconfig),
/// 5. `TARI_*` environment variables and `-p`/application overrides again: they are stored in the returned `Config`
///    under the reserved key [`CONFIG_OVERRIDES_KEY`] and re-applied by `merge_subconfig` on top of the scoped table.
///
/// So the scoped table beats the file's unscoped keys, and an explicit env var or `-p` override beats both.
///
/// A `-p` override that sets any `<section>.network` key to a network other than the resolved one is rejected with an
/// error, and so is a `TARI_*` env override for this application's own section (see [`check_network_overrides`]). A
/// contradicting env override for another application's section, or a contradiction coming only from the config file
/// (e.g. a `[miner]` section in a config shared with the node), is only warned about.
///
/// The replayed overrides form a ladder, lowest to highest (see [`build_replay_list`]):
/// 1. `TARI_*` env vars as given (unscoped `<section>.<key>` and scoped `<network>.<section>.<key>`),
/// 2. env vars scoped to the resolved network, copied to `<section>.<key>`, so a scoped env var beats the unscoped one,
/// 3. `-p` and application-injected overrides,
/// 4. `-p`/application overrides scoped to the resolved network, copied to `<section>.<key>`.
///
/// Keys are lowercased for the replay. `-p` keys that start with `__` or contain `[`/`]` are rejected, so the reserved
/// [`CONFIG_OVERRIDES_KEY`] cannot be touched from the command line.
pub fn load_configuration_with_overrides<P: AsRef<Path>, TOverride: ConfigOverrideProvider>(
    config_path: P,
    overrides: &TOverride,
    cli_network: Option<Network>,
) -> Result<Config, ConfigError> {
    check_for_incorrect_env_vars();
    warn_if_untrusted(config_path.as_ref());
    let filename = config_path
        .as_ref()
        .to_str()
        .ok_or_else(|| ConfigError::new("Invalid config file path", None))?;
    let file_cfg = Config::builder()
        .add_source(config::File::with_name(filename))
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;
    warn_if_secrets_readable(config_path.as_ref(), &file_cfg);
    let cfg = Config::builder()
        .add_source(file_cfg)
        .add_source(
            config::Environment::with_prefix("TARI")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;

    let network = match cli_network {
        Some(val) => val,
        None => match cfg.get_string("network") {
            Ok(network) => {
                Network::from_str(&network).map_err(|e| ConfigError::new("Invalid network", Some(e.to_string())))?
            },
            Err(config::ConfigError::NotFound(_)) => {
                debug!(target: LOG_TARGET, "No network configuration found. Using default.");
                Network::default()
            },
            Err(e) => {
                return Err(ConfigError::new(
                    "Could not get network configuration",
                    Some(e.to_string()),
                ));
            },
        },
    };

    info!(target: LOG_TARGET, "Configuration file loaded.");
    let overrides = overrides.get_config_property_overrides(&network);
    for (key, _) in &overrides {
        check_override_key(key)?;
    }
    for (key, value) in &overrides {
        trace!(
            target: LOG_TARGET,
            "Config property override: {}={}",
            sanitize_for_display(key),
            sanitize_for_display(&mask_value(key, value))
        );
    }
    warn_about_network_overrides(&overrides, network);

    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    set_network_if_choice_valid(network)?;

    // Check the explicit overrides themselves: in the merged config an app-injected `<app>.network` would hide a
    // contradicting env var on the same key.
    let env_overrides = tari_env_overrides();
    check_network_overrides(&env_overrides, &overrides, network)?;

    // Store env and -p/app overrides in the config so that `merge_subconfig` can re-apply them on top of the
    // network-scoped tables.
    let override_keys: Vec<String> = env_overrides
        .iter()
        .chain(&overrides)
        .map(|(key, _)| key.to_lowercase())
        .collect();
    let reapply: Vec<String> = build_replay_list(&env_overrides, &overrides, network)
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();

    let mut builder = Config::builder().add_source(cfg);
    for (key, value) in overrides {
        trace!(
            target: LOG_TARGET,
            "Set override: ({}, {})",
            sanitize_for_display(&key),
            sanitize_for_display(&mask_value(&key, &value))
        );
        builder = builder
            .set_override(key.as_str(), value.as_str())
            .map_err(|ce| ConfigError::new("Could not override config property", Some(ce.to_string())))?;
    }
    let cfg = builder
        .set_override(CONFIG_OVERRIDES_KEY, reapply)
        .map_err(|ce| ConfigError::new("Could not override config property", Some(ce.to_string())))?
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;

    warn_about_file_network_keys(&cfg, network, &override_keys)?;

    Ok(cfg)
}

/// Returns the `TARI_*` environment variables as config overrides, using the same key mapping as the
/// `config::Environment` source in [`load_configuration_with_overrides`] (lowercase, `TARI_` stripped, `__` → `.`).
fn tari_env_overrides() -> Vec<(String, String)> {
    let mut result = Vec::new();
    for (name, value) in std::env::vars_os() {
        let (Some(name), Some(value)) = (name.to_str(), value.to_str()) else {
            continue;
        };
        let name = name.to_lowercase();
        let Some(key) = name.strip_prefix("tari_") else {
            continue;
        };
        let key = key.replace("__", ".");
        if !is_valid_config_key(&key) || check_override_key(&key).is_err() {
            warn!(
                target: LOG_TARGET,
                "Ignoring environment variable TARI_{}: '{}' is not a valid config key",
                sanitize_for_display(&name.strip_prefix("tari_").unwrap_or(&name).to_uppercase()),
                sanitize_for_display(&key)
            );
            continue;
        }
        result.push((key, value.to_string()));
    }
    result
}

/// Returns an error if an override key could touch the reserved [`CONFIG_OVERRIDES_KEY`] or index into an array:
/// keys (lowercased, trimmed) that start with `__` or contain `[` or `]`.
fn check_override_key(key: &str) -> Result<(), ConfigError> {
    let key = key.trim().to_lowercase();
    if key.starts_with("__") || key.contains(['[', ']']) {
        return Err(ConfigError::new(
            "Invalid config override key",
            Some(format!(
                "'{}' is reserved or uses array indexing, which overrides do not support",
                sanitize_for_display(&key)
            )),
        ));
    }
    Ok(())
}

/// For each override scoped to the resolved network (`<network>.<section>.<key>`), returns a copy without the network
/// prefix (`<section>.<key>`). `merge_subconfig` only re-applies `<section>.`-prefixed entries, so without the copy a
/// less specific override on `<section>.<key>` would beat the scoped one. Keys are lowercased.
fn unscoped_copies(overrides: &[(String, String)], network: Network) -> Vec<(String, String)> {
    let prefix = format!("{}.", network.as_key_str());
    let mut result = Vec::new();
    for (key, value) in overrides {
        if let Some(rest) = key.to_lowercase().strip_prefix(&prefix) &&
            rest.contains('.')
        {
            result.push((rest.to_string(), value.clone()));
        }
    }
    result
}

/// Builds the list of overrides that `merge_subconfig` re-applies, with lowercased keys, in this order (later wins):
/// env vars as given, env vars scoped to the resolved network copied unscoped, `-p`/application overrides, then
/// `-p`/application overrides scoped to the resolved network copied unscoped.
fn build_replay_list(
    env_overrides: &[(String, String)],
    app_overrides: &[(String, String)],
    network: Network,
) -> Vec<(String, String)> {
    let mut result: Vec<(String, String)> = env_overrides
        .iter()
        .map(|(key, value)| (key.to_lowercase(), value.clone()))
        .collect();
    result.extend(unscoped_copies(env_overrides, network));
    result.extend(
        app_overrides
            .iter()
            .map(|(key, value)| (key.to_lowercase(), value.clone())),
    );
    result.extend(unscoped_copies(app_overrides, network));
    result
}

/// Warns about overrides that do not do what they look like they do.
fn warn_about_network_overrides(overrides: &[(String, String)], network: Network) {
    for (key, value) in overrides {
        if key == "network" {
            warn!(
                target: LOG_TARGET,
                "The config override 'network={}' is ignored. Use --network or TARI_NETWORK to choose the network.",
                sanitize_for_display(value)
            );
        }
        if key == "common.base_path" {
            let last = Path::new(value).file_name().and_then(|n| n.to_str()).unwrap_or("");
            if last != network.as_key_str() {
                warn!(
                    target: LOG_TARGET,
                    "The base path '{}' does not end in the network name '{network}', but the network is \
                     '{network}'. Check --base-path and --network.",
                    sanitize_for_display(value)
                );
            }
        }
    }
}

/// Returns true if `key` is a `<section>.network` key that applies to the resolved `network`: unscoped (e.g.
/// `base_node.network`) or scoped to the resolved network (e.g. `mainnet.base_node.network` when running mainnet).
/// The top-level `network` key (the input to network resolution) and tables scoped to other networks do not apply.
fn is_applicable_network_key(key: &str, network: Network) -> bool {
    if key == "network" || !key.ends_with(".network") {
        return false;
    }
    let first_segment = key.split('.').next().unwrap_or("");
    match Network::from_str(first_segment) {
        Ok(scope) => scope == network,
        Err(_) => true,
    }
}

/// Returns the section part of an applicable `<section>.network` or `<network>.<section>.network` key.
fn network_key_section(key: &str, network: Network) -> &str {
    let key = key.strip_suffix(".network").unwrap_or(key);
    key.strip_prefix(network.as_key_str())
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(key)
}

/// Checks explicit overrides that set an applicable `<section>.network` key (see [`is_applicable_network_key`]) to a
/// network other than the resolved `network`. Every override is checked on its own, so a contradicting env var is
/// caught even when an application-injected override on the same key wins in the merged config.
///
/// A contradicting `-p` (or application-injected) override is always an error. A contradicting env var is an error
/// for the running application's own sections: the `<section>` of every unscoped `<section>.network` key in
/// `app_overrides` (each application injects its own `<section>.network`). A contradicting env var for any other
/// section (e.g. a stale `TARI_MINER__NETWORK` when starting the node) is only warned about. Values that are not a
/// network name are left for the section's own deserialization to report.
fn check_network_overrides(
    env_overrides: &[(String, String)],
    app_overrides: &[(String, String)],
    network: Network,
) -> Result<(), ConfigError> {
    let mut app_sections = Vec::new();
    for (key, _) in app_overrides {
        let key = key.to_lowercase();
        if let Some(section) = key.strip_suffix(".network") &&
            !section.is_empty() &&
            !section.contains('.')
        {
            app_sections.push(section.to_string());
        }
    }

    let explicit = app_overrides.iter().map(|entry| (entry, true));
    for ((key, value), is_explicit) in env_overrides.iter().map(|entry| (entry, false)).chain(explicit) {
        let key = key.to_lowercase();
        if !is_applicable_network_key(&key, network) {
            continue;
        }
        let Ok(configured) = Network::from_str(value) else {
            continue;
        };
        if configured == network {
            continue;
        }
        let section = network_key_section(&key, network);
        if is_explicit || app_sections.iter().any(|s| s == section) {
            return Err(ConfigError::new(
                "Conflicting network configuration",
                Some(format!(
                    "Config key {} is set to {configured} but the network is {network}; use --network or TARI_NETWORK",
                    sanitize_for_display(&key)
                )),
            ));
        }
        warn!(
            target: LOG_TARGET,
            "Override {}={configured} does not match the network {network}. It is not for this application, so it \
             is ignored; use --network or TARI_NETWORK to choose the network.",
            sanitize_for_display(&key)
        );
    }
    Ok(())
}

/// Warns about applicable `<section>.network` keys (see [`is_applicable_network_key`]) in the merged config that differ
/// from the resolved `network` and were not set by an override (`override_keys`), i.e. that come only from the config
/// file. These do not stop the start, so a shared config with, say, `[miner] network = "esmeralda"` still lets a node
/// run with another `--network`. Contradicting overrides are rejected earlier by [`check_network_overrides`].
fn warn_about_file_network_keys(cfg: &Config, network: Network, override_keys: &[String]) -> Result<(), ConfigError> {
    let root = cfg
        .cache
        .clone()
        .into_table()
        .map_err(|ce| ConfigError::new("Could not read config", Some(ce.to_string())))?;
    let mut leaves = Vec::new();
    collect_leaves("", root, &mut leaves);
    for (key, value) in leaves {
        if !is_applicable_network_key(&key, network) || override_keys.iter().any(|k| k.eq_ignore_ascii_case(&key)) {
            continue;
        }
        let Ok(value) = value.into_string() else {
            continue;
        };
        let Ok(configured) = Network::from_str(&value) else {
            continue;
        };
        if configured != network {
            warn!(
                target: LOG_TARGET,
                "Config key {} is set to {configured} in the config file but the network is {network}. The value \
                 is ignored for this run; use --network or TARI_NETWORK to choose the network.",
                sanitize_for_display(&key)
            );
        }
    }
    Ok(())
}

/// Flattens a config table into `(dotted.key, value)` pairs. Tables are descended into; every other value is a leaf.
fn collect_leaves(prefix: &str, table: config::Map<String, config::Value>, out: &mut Vec<(String, config::Value)>) {
    for (key, value) in table {
        let full_key = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        };
        match value.kind {
            ValueKind::Table(table) => collect_leaves(&full_key, table, out),
            _ => out.push((full_key, value)),
        }
    }
}

/// Words that mark the last segment of a config key or env var name as holding a secret. Shared by
/// [`is_secret_key`] (readable-config warning) and [`mask_value`] (masking), so the two cannot drift. `auth` also
/// covers `*_auth`, `*_authentication`, `control_auth` and a bare `auth` (e.g. `socks.auth`). `seed_words` rather than
/// `seed`, so public `peer_seeds`/`dns_seeds` lists are not treated as secrets.
const SECRET_WORDS: &[&str] = &[
    "password",
    "passphrase",
    "secret",
    "seed_words",
    "auth",
    "token",
    "cookie",
    "mnemonic",
    "private",
];

/// Returns true if the last segment of a config key names a secret (contains one of [`SECRET_WORDS`]).
fn is_secret_key(key: &str) -> bool {
    let last = key.rsplit('.').next().unwrap_or(key).to_lowercase();
    SECRET_WORDS.iter().any(|w| last.contains(w))
}

/// Returns the secret-bearing keys in `cfg` that hold a non-empty value. `"none"` and `"auto"` are not secrets.
fn find_secret_keys(cfg: &Config) -> Vec<String> {
    let Ok(root) = cfg.cache.clone().into_table() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    find_secret_keys_in("", root, &mut found);
    found
}

fn find_secret_keys_in(prefix: &str, table: config::Map<String, config::Value>, found: &mut Vec<String>) {
    for (key, value) in table {
        let full_key = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        };
        if is_secret_key(&full_key) {
            let non_empty = match &value.kind {
                // Flags such as `monerod_use_auth = true` are not secrets
                ValueKind::Nil | ValueKind::Boolean(_) => false,
                ValueKind::String(s) => {
                    let s = s.trim().to_lowercase();
                    !(s.is_empty() || s == "none" || s == "auto")
                },
                ValueKind::Table(t) => !t.is_empty(),
                ValueKind::Array(a) => !a.is_empty(),
                _ => true,
            };
            if non_empty {
                found.push(full_key);
            }
        } else if is_url_key(&full_key) && value_contains_at(&value) {
            // Credentials in a URL, e.g. `monerod_url = ["http://user:pass@host"]`
            found.push(full_key);
        } else if let ValueKind::Table(table) = value.kind {
            find_secret_keys_in(&full_key, table, found);
        } else {
            // Not a secret and not a table
        }
    }
}

/// Returns true if the last segment of a config key ends with `_url` or `_address`.
fn is_url_key(key: &str) -> bool {
    let last = key.rsplit('.').next().unwrap_or(key).to_lowercase();
    last.ends_with("_url") || last.ends_with("_address")
}

/// Returns true if a string value, or any string element of an array value, contains `@` (URL userinfo).
fn value_contains_at(value: &config::Value) -> bool {
    match &value.kind {
        ValueKind::String(s) => s.contains('@'),
        ValueKind::Array(items) => items
            .iter()
            .any(|item| matches!(&item.kind, ValueKind::String(s) if s.contains('@'))),
        _ => false,
    }
}

/// Warns if the config file holds a secret but can be read by the group or by other users.
fn warn_if_secrets_readable(path: &Path, file_cfg: &Config) {
    for message in secrets_readable_messages(path, file_cfg) {
        emit_security_warning(&message);
    }
}

/// Returns one warning per secret in `file_cfg` if the config file at `path` is readable by the group or by other
/// users. Keys and paths are sanitized for display.
#[cfg(unix)]
fn secrets_readable_messages(path: &Path, file_cfg: &Config) -> Vec<String> {
    use std::os::unix::fs::MetadataExt;

    let Ok(metadata) = fs::metadata(path) else {
        return Vec::new();
    };
    if metadata.mode() & 0o044 == 0 {
        return Vec::new();
    }
    let shown_path = display_path(path);
    find_secret_keys(file_cfg)
        .into_iter()
        .map(|key| {
            format!(
                "Config file {} is readable by other users (mode {:o}) and contains the secret '{}'. Run `chmod 600 \
                 {}`, or move the secret to an environment variable or command line argument.",
                shown_path,
                metadata.mode() & 0o7777,
                sanitize_for_display(&key),
                shown_path
            )
        })
        .collect()
}

#[cfg(not(unix))]
fn secrets_readable_messages(_path: &Path, _file_cfg: &Config) -> Vec<String> {
    Vec::new()
}

/// Maximum number of characters of one fragment that [`sanitize_for_display`] keeps.
const MAX_DISPLAY_CHARS: usize = 512;

/// Returns true for characters that must not reach a terminal or log as-is: control characters (including ESC, CR,
/// LF, DEL and C1 controls), invisible/bidi format characters that can reorder or hide text, and the Unicode line and
/// paragraph separators.
fn is_unsafe_display_char(c: char) -> bool {
    c.is_control() ||
        matches!(
            c,
            '\u{061C}' |
                '\u{180E}' |
                '\u{200B}'..='\u{200F}' |
                '\u{2028}' |
                '\u{2029}' |
                '\u{202A}'..='\u{202E}' |
                '\u{2060}'..='\u{2064}' |
                '\u{2066}'..='\u{2069}' |
                '\u{FEFF}'
        )
}

/// Returns `s` with every unsafe character (see [`is_unsafe_display_char`]) replaced by its `\u{..}` escape. Not
/// length limited; used as a backstop on whole warning messages.
pub(crate) fn escape_display_chars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        if is_unsafe_display_char(c) {
            result.extend(c.escape_unicode());
        } else {
            result.push(c);
        }
    }
    result
}

/// Makes text from config files, paths or env vars safe to show: control characters, invisible/bidi format
/// characters and line/paragraph separators are replaced by their `\u{..}` escapes, so the text cannot inject
/// terminal escape sequences, reorder a line or fake log lines. Only the first 512 characters are kept; the rest is
/// replaced by `…(N more)`.
pub fn sanitize_for_display(s: &str) -> String {
    let total = s.chars().count();
    if total <= MAX_DISPLAY_CHARS {
        return escape_display_chars(s);
    }
    let kept: String = s.chars().take(MAX_DISPLAY_CHARS).collect();
    format!(
        "{}…({} more)",
        escape_display_chars(&kept),
        total.saturating_sub(MAX_DISPLAY_CHARS)
    )
}

/// [`sanitize_for_display`] for a path.
fn display_path(path: &Path) -> String {
    sanitize_for_display(&path.display().to_string())
}

/// Number of security warnings recorded by this process (see [`print_security_warning`]).
static WARNINGS_EMITTED: AtomicUsize = AtomicUsize::new(0);

/// The sanitized texts of the security warnings recorded by this process, in order.
static SECURITY_WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Returns how many security warnings this process has recorded: trust findings that mean another (non-root) user can
/// control a config or log file (see [`untrusted_severe_reasons`]) and secrets in a config file others can read.
/// Routine findings (files writable only by the user's private group, root-owned files) are printed but not counted.
/// Applications can use this to make sure an interactive user sees them, e.g. before a full-screen UI hides the
/// terminal.
pub fn warnings_emitted() -> usize {
    WARNINGS_EMITTED.load(Ordering::Relaxed)
}

/// Returns the sanitized texts of the security warnings counted by [`warnings_emitted`], so they can be shown again.
pub fn security_warnings() -> Vec<String> {
    SECURITY_WARNINGS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Writes a warning as `WARNING: <message>` to `writer`. The message is sanitized for display.
pub(crate) fn write_warning<W: Write>(writer: &mut W, message: &str) {
    if writeln!(writer, "WARNING: {}", escape_display_chars(message)).is_err() {
        // Nowhere else to report a failed write to stderr
    }
}

/// Prints a warning to stderr. It is not recorded as a security warning.
pub(crate) fn print_warning(message: &str) {
    write_warning(&mut std::io::stderr(), message);
}

/// Prints a security warning to stderr and records it (see [`warnings_emitted`] and [`security_warnings`]).
pub(crate) fn print_security_warning(message: &str) {
    let message = escape_display_chars(message);
    WARNINGS_EMITTED.fetch_add(1, Ordering::Relaxed);
    SECURITY_WARNINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(message.clone());
    write_warning(&mut std::io::stderr(), &message);
}

/// Prints a warning to stderr and also logs it. Printing does not depend on the logger, so the warning is seen even
/// when no logger is set up yet or a (possibly planted) log config silences it. The message is sanitized for display
/// once and the same text goes to both. It is not recorded as a security warning; see [`emit_security_warning`].
pub(crate) fn emit_warning(message: &str) {
    let message = escape_display_chars(message);
    print_warning(&message);
    warn!(target: LOG_TARGET, "⚠️  {message}");
}

/// Like [`emit_warning`], but also records the message as a security warning (see [`warnings_emitted`]).
pub(crate) fn emit_security_warning(message: &str) {
    let message = escape_display_chars(message);
    print_security_warning(&message);
    warn!(target: LOG_TARGET, "⚠️  {message}");
}

/// Warns (on stderr and in the log) if a file that is about to be loaded could have been written by someone else.
/// Severe findings (see [`untrusted_severe_reasons`]) are recorded as security warnings; routine ones are only printed
/// and logged. This only warns; loading always continues.
pub fn warn_if_untrusted(path: &Path) {
    for (severe, reason) in untrusted_findings(path) {
        if severe {
            emit_security_warning(&reason);
        } else {
            emit_warning(&reason);
        }
    }
}

/// Returns why a file that is about to be loaded (config or log config) could have been written by someone else, or
/// an empty list if it looks fine. On Unix the file (following symlinks) and its immediate parent directory should be
/// owned by the effective user and should not be group- or world-writable. If the file is a symlink, the directory of
/// every hop in the symlink chain (up to 32 hops) is checked too, and a symlink whose target does not exist is
/// reported. Paths in the messages are sanitized for display. On other platforms this returns nothing.
///
/// This includes findings that are common and harmless on many systems (files writable by the user's private group
/// under a umask of 002, root-owned read-only files); see [`untrusted_severe_reasons`] for the subset that means
/// another user can control the file.
pub fn untrusted_reasons(path: &Path) -> Vec<String> {
    untrusted_findings(path).into_iter().map(|(_, reason)| reason).collect()
}

/// Returns the subset of [`untrusted_reasons`] that means another (non-root) user can control the file: the file or
/// a checked directory is owned by someone other than the effective user and root, is world-writable, or is
/// group-writable by a group that is not the user's private group (see [`is_private_group`]), or the file is a
/// symlink whose target does not exist. Root-owned files and files writable only by the user's private group are not
/// included.
pub fn untrusted_severe_reasons(path: &Path) -> Vec<String> {
    untrusted_findings(path)
        .into_iter()
        .filter(|(severe, _)| *severe)
        .map(|(_, reason)| reason)
        .collect()
}

/// Returns `(severe, reason)` pairs from a single check of the file, its directory and its symlink chain. This is
/// what [`untrusted_reasons`] and [`untrusted_severe_reasons`] are built from; use it directly to decide from one pass.
#[cfg(unix)]
pub fn untrusted_findings(path: &Path) -> Vec<(bool, String)> {
    let mut findings = Vec::new();
    if let Ok(metadata) = fs::metadata(path) {
        findings.extend(file_findings(path, &metadata));
    }
    findings.extend(untrusted_path_findings(path));
    findings
}

/// Classifies the owner and mode of one file or directory (`kind` is used in the message): owned by someone other
/// than the effective user (severe unless root), world-writable (severe), or group-writable (severe unless the group
/// is the user's private group).
#[cfg(unix)]
fn classify_metadata(kind: &str, path: &Path, metadata: &fs::Metadata) -> Vec<(bool, String)> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    let mut findings = Vec::new();
    let mode = metadata.mode() & 0o7777;
    let shown = display_path(path);
    let owner = metadata.uid();
    if owner != euid {
        // Root can write anything anyway, so a root-owned file is only worth a warning
        findings.push((
            owner != 0,
            format!(
                "The {} {} is owned by uid {} (mode {:o}), not by the current user (uid {}). Another user could \
                 control its contents. Fix with `chown {} {}` or use a base path you own.",
                kind, shown, owner, mode, euid, euid, shown
            ),
        ));
    }
    if mode & 0o002 != 0 {
        findings.push((
            true,
            format!(
                "The {} {} (owner uid {}) is writable by other users (mode {:o}). Another user could control its \
                 contents. Fix with `chmod o-w {}`.",
                kind, shown, owner, mode, shown
            ),
        ));
    } else if mode & 0o020 != 0 {
        let gid = metadata.gid();
        findings.push((
            !is_private_group(gid),
            format!(
                "The {} {} (owner uid {}) is writable by its group (gid {}, mode {:o}). Members of that group could \
                 control its contents. Fix with `chmod g-w {}` if the group is shared.",
                kind, shown, owner, gid, mode, shown
            ),
        ));
    } else {
        // Not writable by anyone else
    }
    findings
}

/// Classifies the file itself from already obtained metadata, e.g. from an open file descriptor.
#[cfg(unix)]
pub(crate) fn file_findings(path: &Path, metadata: &fs::Metadata) -> Vec<(bool, String)> {
    classify_metadata("file", path, metadata)
}

/// The path-based part of [`untrusted_findings`]: the file's directory, the directory of every symlink hop (up to 32)
/// and a dangling symlink. The file itself is not checked here.
#[cfg(unix)]
pub(crate) fn untrusted_path_findings(path: &Path) -> Vec<(bool, String)> {
    use std::os::unix::fs::MetadataExt;

    const MAX_SYMLINK_HOPS: usize = 32;

    let parent = parent_or_current(path);
    let mut findings = Vec::new();
    let mut to_check = vec![("directory", parent.clone())];
    let mut seen_dirs = vec![fs::canonicalize(&parent).unwrap_or(parent)];

    // Walk the symlink chain one hop at a time and check the directory each hop lives in
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        let Ok(link_metadata) = fs::symlink_metadata(&current) else {
            break;
        };
        if !link_metadata.file_type().is_symlink() {
            break;
        }
        let Ok(target) = fs::read_link(&current) else {
            break;
        };
        let next = if target.is_absolute() {
            target
        } else {
            parent_or_current(&current).join(target)
        };
        let next_dir = parent_or_current(&next);
        let canonical_dir = fs::canonicalize(&next_dir).unwrap_or_else(|_| next_dir.clone());
        if !seen_dirs.contains(&canonical_dir) {
            seen_dirs.push(canonical_dir.clone());
            to_check.push(("directory (symlink hop)", canonical_dir.clone()));
        }
        current = next;
    }

    if fs::metadata(path).is_err() &&
        let Ok(link_metadata) = fs::symlink_metadata(path) &&
        link_metadata.file_type().is_symlink()
    {
        findings.push((
            true,
            format!(
                "{} is a symlink whose target does not exist (owner uid {}); remove it or point it at a real file.",
                display_path(path),
                link_metadata.uid()
            ),
        ));
    }

    for (kind, p) in to_check {
        if let Ok(metadata) = fs::metadata(&p) {
            findings.extend(classify_metadata(kind, &p, &metadata));
        }
    }
    findings
}

/// Returns true if `gid` is the current user's private group: it is the effective gid, it is not one of macOS's
/// shared `staff` (20) or `admin` (80) groups, and no user other than the current one is listed as a member. If the
/// group or user cannot be looked up, the group is treated as shared.
#[cfg(unix)]
pub(crate) fn is_private_group(gid: u32) -> bool {
    // SAFETY: getegid has no preconditions and cannot fail.
    let egid = unsafe { libc::getegid() };
    if gid != egid {
        return false;
    }
    if cfg!(target_os = "macos") && (gid == 20 || gid == 80) {
        return false;
    }
    let (Some(user), Some(members)) = (current_user_name(), group_members(gid)) else {
        return false;
    };
    members.iter().all(|member| *member == user)
}

/// Returns the member names of group `gid` (from `getgrgid_r`), or `None` if it cannot be looked up.
#[cfg(unix)]
pub(crate) fn group_members(gid: u32) -> Option<Vec<String>> {
    use std::ffi::CStr;

    let mut buffer: Vec<libc::c_char> = vec![0; 4096];
    loop {
        // SAFETY: `group` is plain old data that getgrgid_r fills in; all-zero is a valid initial value.
        let mut group: libc::group = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::group = std::ptr::null_mut();
        // SAFETY: all pointers are valid for the duration of the call and `buffer.len()` is the buffer's real size.
        let rc = unsafe { libc::getgrgid_r(gid, &mut group, buffer.as_mut_ptr(), buffer.len(), &mut result) };
        if rc == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len().saturating_mul(2), 0);
            continue;
        }
        if rc != 0 || result.is_null() {
            return None;
        }
        let mut members = Vec::new();
        let mut entry = group.gr_mem;
        while !entry.is_null() {
            // SAFETY: gr_mem is a null-terminated array of C strings that live in `buffer`.
            let name = unsafe { *entry };
            if name.is_null() {
                break;
            }
            // SAFETY: `name` is a valid, null-terminated C string in `buffer`.
            members.push(unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned());
            // SAFETY: the array is null-terminated and we stopped at the terminator above.
            entry = unsafe { entry.add(1) };
        }
        return Some(members);
    }
}

/// Returns the effective user's name (from `getpwuid_r`), or `None` if it cannot be looked up.
#[cfg(unix)]
fn current_user_name() -> Option<String> {
    use std::ffi::CStr;

    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    let mut buffer: Vec<libc::c_char> = vec![0; 4096];
    loop {
        // SAFETY: `passwd` is plain old data that getpwuid_r fills in; all-zero is a valid initial value.
        let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: all pointers are valid for the duration of the call and `buffer.len()` is the buffer's real size.
        let rc = unsafe { libc::getpwuid_r(euid, &mut passwd, buffer.as_mut_ptr(), buffer.len(), &mut result) };
        if rc == libc::ERANGE && buffer.len() < 1 << 20 {
            buffer.resize(buffer.len().saturating_mul(2), 0);
            continue;
        }
        if rc != 0 || result.is_null() || passwd.pw_name.is_null() {
            return None;
        }
        // SAFETY: pw_name is a valid, null-terminated C string in `buffer`.
        return Some(unsafe { CStr::from_ptr(passwd.pw_name) }.to_string_lossy().into_owned());
    }
}

/// Returns the parent directory of `path`, or `.` if it has none.
#[cfg(unix)]
fn parent_or_current(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

#[cfg(not(unix))]
pub fn untrusted_findings(_path: &Path) -> Vec<(bool, String)> {
    Vec::new()
}

#[cfg(not(unix))]
pub(crate) fn file_findings(_path: &Path, _metadata: &fs::Metadata) -> Vec<(bool, String)> {
    Vec::new()
}

#[cfg(not(unix))]
pub(crate) fn untrusted_path_findings(_path: &Path) -> Vec<(bool, String)> {
    Vec::new()
}

/// Returns the value to display for a config key or environment variable. Values are masked as `***` unless the last
/// key segment (split on `.` and `__`, case-insensitive, with any `tari_`/`minotari_` prefix removed) is known to be
/// safe to show: `*_address` and `*_url` (masked entirely if the value contains `@`, `?` or `#` anywhere, since
/// credentials, query strings and fragments may contain list separators; otherwise shown with each URL path replaced
/// by `/***`, see [`mask_url_paths`]), `*_port`,
/// `*_enabled`, `*_interval`, `*_timeout`, `*_path`, `*_dir`, `network`, `base_path` or `override_from`. Otherwise
/// boolean and numeric values are shown. Keys that name a secret (see [`SECRET_WORDS`]) are always masked, whatever the
/// value.
pub fn mask_value(key: &str, value: &str) -> String {
    let lower = key.to_lowercase();
    let last = lower.rsplit('.').next().unwrap_or("");
    let last = last.rsplit("__").next().unwrap_or(last);
    let last = last
        .strip_prefix("minotari_")
        .or_else(|| last.strip_prefix("tari_"))
        .unwrap_or(last);

    // Secrets are masked before anything else, so a numeric password or PIN is never shown
    if is_secret_key(last) {
        return "***".to_string();
    }
    let trimmed = value.trim();
    if trimmed.parse::<bool>().is_ok() || trimmed.parse::<f64>().is_ok() {
        return value.to_string();
    }
    if last.ends_with("_url") || last.ends_with("_address") {
        // Credentials, query strings and fragments may themselves contain `,`, `;` or spaces, so any attempt to strip
        // them from a list can leak part of them: mask the whole value instead
        if value.contains(['@', '?', '#']) {
            return "***".to_string();
        }
        return mask_url_paths(value);
    }
    let safe_suffixes = ["_port", "_enabled", "_interval", "_timeout", "_path", "_dir"];
    let safe_names = ["network", "base_path", "override_from"];
    if safe_suffixes.iter().any(|s| last.ends_with(s)) || safe_names.contains(&last) {
        return value.to_string();
    }
    "***".to_string()
}

/// Replaces the path of every URL (an element containing `://`) in a list separated by `,`, `;` or whitespace with
/// `/***`, keeping `scheme://host[:port]` and the separators. URLs with no path or a bare `/`, and elements that are
/// not URLs (e.g. multiaddrs such as `/ip4/1.2.3.4/tcp/18189`), are kept as they are.
fn mask_url_paths(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut element = String::new();
    for c in value.chars() {
        if c == ',' || c == ';' || c.is_whitespace() {
            result.push_str(&mask_url_path(&element));
            element.clear();
            result.push(c);
        } else {
            element.push(c);
        }
    }
    result.push_str(&mask_url_path(&element));
    result
}

/// See [`mask_url_paths`]; handles one element.
fn mask_url_path(element: &str) -> String {
    let Some((scheme, rest)) = element.split_once("://") else {
        return element.to_string();
    };
    match rest.split_once('/') {
        Some((authority, path)) if !path.is_empty() => format!("{scheme}://{authority}/***"),
        _ => element.to_string(),
    }
}

/// Checks for environment variables that look like they are intended to configure Tari applications but use an
/// incorrect prefix or format. Warns users about common mistakes and suggests the correct format.
fn check_for_incorrect_env_vars() {
    // These are patterns that users often try but that won't be picked up by the config loader.
    // The correct prefix for config-level env vars is `TARI_` with `__` as the nested separator.
    let incorrect_patterns: &[(&str, &str)] = &[
        ("MINOTARI_NODE__", "TARI_BASE_NODE__"),
        ("MINOTARI_BASE_NODE__", "TARI_BASE_NODE__"),
        ("MINOTARI_WALLET__", "TARI_WALLET__"),
        ("MINOTARI_MINER__", "TARI_MINER__"),
        ("MINOTARI_MERGE_MINING_PROXY__", "TARI_MERGE_MINING_PROXY__"),
    ];

    for (var_name, var_value) in std::env::vars() {
        for (incorrect_prefix, correct_prefix) in incorrect_patterns {
            if let Some(suffix) = var_name.strip_prefix(incorrect_prefix) {
                warn!(
                    target: LOG_TARGET,
                    "⚠️  Environment variable '{}={}' uses an unrecognised prefix and will be ignored. Did you mean \
                     '{}{}'? Configuration environment variables must use the 'TARI_' prefix with '__' as the nested \
                     key separator.",
                    sanitize_for_display(&var_name),
                    sanitize_for_display(&mask_value(&var_name, &var_value)),
                    correct_prefix,
                    sanitize_for_display(suffix)
                );
            }
        }
    }
}

/// Prints all TARI_* and MINOTARI_* environment variables to stdout, masking sensitive values (see [`mask_value`]).
/// Also prints any config property overrides (`-p` args) if provided.
/// This is useful for debugging configuration issues.
pub fn print_env_vars(config_overrides: &[(String, String)]) {
    let mut env_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("TARI_") || k.starts_with("MINOTARI_"))
        .map(|(k, v)| {
            let display_value = sanitize_for_display(&mask_value(&k, &v));
            (sanitize_for_display(&k), display_value)
        })
        .collect();
    env_vars.sort_by(|(a, _), (b, _)| a.cmp(b));

    if env_vars.is_empty() {
        println!("No TARI_* or MINOTARI_* environment variables are set.");
    } else {
        println!("Tari-related environment variables:");
        for (key, value) in &env_vars {
            println!("  {key}={value}");
        }
    }

    if config_overrides.is_empty() {
        println!("No -p config property overrides are set.");
    } else {
        println!("\nConfig property overrides (-p args):");
        for (key, value) in config_overrides {
            let display_value = sanitize_for_display(&mask_value(key, value));
            println!("  {}={display_value}", sanitize_for_display(key));
        }
    }
}

/// Returns a new configuration file template in parts from the embedded presets. If non_interactive is false, the user
/// is prompted to select if they would like to select a base node configuration that enables mining or not.
/// Also includes the common configuration defined in `config/presets/common.toml`.
pub fn prompt_default_config() -> [&'static str; 12] {
    let mine = prompt(
        "Node config does not exist.\nWould you like to mine (Y/n)?\nNOTE: this will enable additional gRPC methods \
         that could be used to monitor and submit blocks from this node.",
    );
    get_default_config(mine)
}

/// Returns the default configuration file template in parts from the embedded presets. If use_mining_config is true,
/// the base node configuration that enables mining is returned, otherwise the non-mining configuration is returned.
pub fn get_default_config(use_mining_config: bool) -> [&'static str; 12] {
    let base_node_allow_methods = if use_mining_config {
        include_str!("../../config/presets/c_base_node_b_mining_allow_methods.toml")
    } else {
        include_str!("../../config/presets/c_base_node_b_non_mining_allow_methods.toml")
    };

    let common = include_str!("../../config/presets/a_common.toml");
    [
        common,
        include_str!("../../config/presets/b_peer_seeds.toml"),
        include_str!("../../config/presets/c_base_node_a.toml"),
        base_node_allow_methods,
        include_str!("../../config/presets/c_base_node_c.toml"),
        include_str!("../../config/presets/d_console_wallet.toml"),
        include_str!("../../config/presets/g_miner.toml"),
        include_str!("../../config/presets/f_merge_mining_proxy.toml"),
        include_str!("../../config/presets/e_validator_node.toml"),
        include_str!("../../config/presets/h_collectibles.toml"),
        include_str!("../../config/presets/i_indexer.toml"),
        include_str!("../../config/presets/j_dan_wallet_daemon.toml"),
    ]
}

/// Writes a single file concatenating all the provided sources to the specified path. If the parent directory does not
/// exist, it is created. The file is created with `create_new`, so an existing file (or symlink) is never followed or
/// overwritten: if one is already there, it is left untouched and will be loaded (and trust-checked) as an existing
/// config file.
pub fn write_config_to<P: AsRef<Path>>(path: P, sources: &[&str]) -> Result<(), std::io::Error> {
    if let Some(d) = path.as_ref().parent() {
        fs::create_dir_all(d)?
    };
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path.as_ref()) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            warn!(
                target: LOG_TARGET,
                "Configuration file {} already exists, it was not overwritten",
                path.as_ref().display()
            );
            return Ok(());
        },
        Err(e) => return Err(e),
    };
    for source in sources {
        file.write_all(source.as_bytes())?;
        file.write_all(b"\n")?;
    }
    debug!(
        target: LOG_TARGET,
        "Created new configuration file {}",
        path.as_ref().display()
    );
    Ok(())
}

pub fn serialize_string<S, T>(source: &T, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Display,
{
    ser.serialize_str(source.to_string().as_str())
}

pub fn deserialize_string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = anyhow::Error>,
    D: Deserializer<'de>,
{
    struct StringOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrStruct<T>
    where T: Deserialize<'de> + FromStr<Err = anyhow::Error>
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where E: de::Error {
            match FromStr::from_str(value) {
                Ok(val) => Ok(val),
                Err(e) => Err(de::Error::custom(e)),
            }
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where M: MapAccess<'de> {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mask_value_shows_only_safe_keys() {
        // Allow-listed suffixes and names print
        assert_eq!(
            mask_value("base_node.grpc_address", "/ip4/127.0.0.1/tcp/18142"),
            "/ip4/127.0.0.1/tcp/18142"
        );
        assert_eq!(mask_value("TARI_BASE_NODE__GRPC_ADDRESS", "127.0.0.1"), "127.0.0.1");
        assert_eq!(mask_value("wallet.log_path", "/tmp/x"), "/tmp/x");
        assert_eq!(mask_value("TARI_BASE_DIR", "/home/me/.tari"), "/home/me/.tari");
        assert_eq!(mask_value("TARI_NETWORK", "esmeralda"), "esmeralda");
        assert_eq!(mask_value("common.base_path", "/home/me/.tari"), "/home/me/.tari");
        assert_eq!(mask_value("base_node.override_from", "mainnet"), "mainnet");
        // Secrets and unknown keys are masked
        assert_eq!(mask_value("wallet.password", "hunter2"), "***");
        assert_eq!(mask_value("MINOTARI_WALLET_PASSWORD", "hunter2"), "***");
        assert_eq!(
            mask_value("TARI_WALLET__GRPC_AUTHENTICATION", "{ username = \"a\" }"),
            "***"
        );
        assert_eq!(
            mask_value("base_node.p2p.transport.tor.control_auth", "password=x"),
            "***"
        );
        assert_eq!(mask_value("something.unknown", "value"), "***");
        // Secrets are masked even when numeric or boolean
        assert_eq!(mask_value("wallet.password", "12345678"), "***");
        assert_eq!(mask_value("MINOTARI_WALLET_PASSWORD", "123456"), "***");
        assert_eq!(mask_value("TARI_WALLET__PASSPHRASE", "true"), "***");
        assert_eq!(mask_value("some.api_token", "42"), "***");
        assert_eq!(mask_value("base_node.grpc_port", "18142"), "18142");
        // Booleans and numbers print
        assert_eq!(mask_value("something.unknown", "true"), "true");
        assert_eq!(mask_value("something.unknown", "42"), "42");
        assert_eq!(mask_value("something.unknown", "1.5"), "1.5");
        // Any '@' in a URL or address masks the whole value: userinfo may contain list separators
        for value in [
            "http://user:pa;ss@host:18081",
            "http://miner:se,cret@127.0.0.1:18142",
            "http://user:top secret@host",
            "http://user:pass@host",
            "socks5://u:p@host",
            "http://u:pa/ss@host:18081",
            "http://127.0.0.1:18081, http://a:b@node.example:18081",
        ] {
            assert_eq!(mask_value("merge_mining_proxy.monerod_url", value), "***", "{value}");
            assert_eq!(mask_value("x.base_node_grpc_address", value), "***", "{value}");
        }
        // Any '?' or '#' masks the whole value too: a query or fragment may contain list separators
        for value in [
            "http://node:18081/json_rpc?a=1;token=x",
            "https://monero.fail/?a,b",
            "https://monero.fail/?a b",
            "http://host/#frag",
            "https://monero.fail/?all=true",
            "http://node:18081/json_rpc?user=x;token=SECRET",
        ] {
            assert_eq!(mask_value("x.monerod_url", value), "***", "{value}");
            assert_eq!(mask_value("x.listener_address", value), "***", "{value}");
        }
        // Without '@', '?' or '#', values print unchanged, separators included
        for value in [
            "https://node.example",
            "https://node.example/",
            "/ip4/1.2.3.4/tcp/18189",
            "http://127.0.0.1:18081, http://node.example:18081",
            "http://one:1; http://two:2",
        ] {
            assert_eq!(mask_value("x.monerod_url", value), value);
            assert_eq!(mask_value("x.listener_address", value), value);
        }
        // URL paths are masked, host and port kept
        assert_eq!(
            mask_value("x.monerod_url", "http://node:18081/json_rpc"),
            "http://node:18081/***"
        );
        assert_eq!(
            mask_value("x.base_node_grpc_address", "https://node.example/secret-path/x"),
            "https://node.example/***"
        );
        assert_eq!(
            mask_value("x.monerod_url", "http://a:1/x, http://b:2;http://c:3/y"),
            "http://a:1/***, http://b:2;http://c:3/***"
        );
        // A bare `auth` segment is a secret
        assert_eq!(mask_value("p2p.transport.socks.auth", "username_password=u:p"), "***");
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn network_override_mismatch_is_an_error() {
        // -p wallet.network=esmeralda on a mainnet resolve, after the app-injected wallet.network=mainnet was replaced
        let app = pairs(&[("wallet.network", "esmeralda")]);
        let err = check_network_overrides(&[], &app, Network::MainNet).unwrap_err();
        assert!(err.to_string().contains("wallet.network"));
        assert!(err.to_string().contains("esmeralda"));
        assert!(err.to_string().contains("mainnet"));

        // TARI_MINER__NETWORK=mainnet in the env snapshot, the miner injects miner.network=esmeralda on the same key:
        // the app value wins in the merged config, but the env value for the app's own section is still rejected
        let env = pairs(&[("miner.network", "mainnet")]);
        let app = pairs(&[("miner.network", "esmeralda")]);
        let err = check_network_overrides(&env, &app, Network::Esmeralda).unwrap_err();
        assert!(err.to_string().contains("miner.network"));

        // Env and app-injected values equal to the resolved network pass
        let env = pairs(&[("miner.network", "esmeralda")]);
        assert!(check_network_overrides(&env, &app, Network::Esmeralda).is_ok());

        // An env override in the resolved network's scoped table of the app's own section
        let env = pairs(&[("mainnet.wallet.network", "nextnet")]);
        let app = pairs(&[("wallet.network", "mainnet")]);
        let err = check_network_overrides(&env, &app, Network::MainNet).unwrap_err();
        assert!(err.to_string().contains("mainnet.wallet.network"));

        // Other networks' tables and the top-level key are not checked
        let env = pairs(&[
            ("mainnet.base_node.network", "mainnet"),
            ("nextnet.wallet.network", "esmeralda"),
            ("network", "nextnet"),
        ]);
        let app = pairs(&[("base_node.network", "esmeralda")]);
        assert!(check_network_overrides(&env, &app, Network::Esmeralda).is_ok());
    }

    #[test]
    fn network_override_for_another_application_only_warns() {
        // A stale TARI_MINER__NETWORK must not block a node start
        let env = pairs(&[("miner.network", "mainnet"), ("esmeralda.wallet.network", "nextnet")]);
        let app = pairs(&[
            ("base_node.network", "esmeralda"),
            ("base_node.override_from", "esmeralda"),
        ]);
        assert!(check_network_overrides(&env, &app, Network::Esmeralda).is_ok());
        // ... but the node's own section is still checked
        let env = pairs(&[("base_node.network", "mainnet")]);
        assert!(check_network_overrides(&env, &app, Network::Esmeralda).is_err());
    }

    #[test]
    fn scoped_overrides_get_unscoped_copies() {
        let app = pairs(&[
            ("esmeralda.p2p.seeds.peer_seeds", "X"),
            ("mainnet.base_node.x", "other network"),
            ("base_node.y", "unscoped"),
        ]);
        assert_eq!(
            unscoped_copies(&app, Network::Esmeralda),
            pairs(&[("p2p.seeds.peer_seeds", "X")])
        );
    }

    #[derive(Default, serde::Serialize, Deserialize)]
    struct SeedsTestConfig {
        peer_seeds: String,
    }
    impl crate::SubConfigPath for SeedsTestConfig {
        fn main_key_prefix() -> &'static str {
            "seeds_test"
        }
    }

    /// Builds a config the way `load_configuration_with_overrides` does: file keys, then env and -p overrides
    /// applied directly, then the replay list stored under the reserved key. Loads `SeedsTestConfig` from it.
    fn load_seeds_test(
        file: &[(&str, &str)],
        env: &[(String, String)],
        app: &[(String, String)],
        network: Network,
    ) -> SeedsTestConfig {
        use crate::DefaultConfigLoader;

        let mut builder = Config::builder();
        for (key, value) in file {
            builder = builder.set_default(*key, *value).unwrap();
        }
        for (key, value) in env.iter().chain(app) {
            builder = builder.set_override(key.as_str(), value.as_str()).unwrap();
        }
        let reapply: Vec<String> = build_replay_list(env, app, network)
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let cfg = builder
            .set_override(CONFIG_OVERRIDES_KEY, reapply)
            .unwrap()
            .build()
            .unwrap();
        <SeedsTestConfig as DefaultConfigLoader>::load_from(&cfg).unwrap()
    }

    const SEEDS_FILE: &[(&str, &str)] = &[
        ("seeds_test.override_from", "esmeralda"),
        ("seeds_test.peer_seeds", "file"),
        ("esmeralda.seeds_test.peer_seeds", "scoped file"),
    ];

    #[test]
    fn scoped_override_beats_env_on_the_unscoped_key() {
        let env = pairs(&[("seeds_test.peer_seeds", "Y")]);
        let app = pairs(&[("esmeralda.seeds_test.peer_seeds", "X")]);
        assert_eq!(
            load_seeds_test(SEEDS_FILE, &env, &app, Network::Esmeralda).peer_seeds,
            "X"
        );
    }

    #[test]
    fn scoped_env_beats_unscoped_env_and_p_beats_both() {
        let env = pairs(&[
            ("seeds_test.peer_seeds", "unscoped env"),
            ("esmeralda.seeds_test.peer_seeds", "scoped env"),
        ]);
        assert_eq!(
            load_seeds_test(SEEDS_FILE, &env, &[], Network::Esmeralda).peer_seeds,
            "scoped env"
        );
        let app = pairs(&[("seeds_test.peer_seeds", "from -p")]);
        assert_eq!(
            load_seeds_test(SEEDS_FILE, &env, &app, Network::Esmeralda).peer_seeds,
            "from -p"
        );
    }

    #[test]
    fn mixed_case_p_override_is_replayed() {
        let env = pairs(&[("seeds_test.peer_seeds", "Y")]);
        let app = pairs(&[("Seeds_Test.peer_seeds", "X")]);
        assert_eq!(
            load_seeds_test(SEEDS_FILE, &env, &app, Network::Esmeralda).peer_seeds,
            "X"
        );
    }

    #[test]
    fn reserved_and_indexed_override_keys_are_rejected() {
        assert!(check_override_key("__tari_overrides[0]").is_err());
        assert!(check_override_key(" __Tari_Overrides").is_err());
        assert!(check_override_key("__anything").is_err());
        assert!(check_override_key("base_node.x[-9999999999]").is_err());
        assert!(check_override_key("base_node.grpc_address").is_ok());

        // And through the loader
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "").unwrap();
        let network = crate::network_check::is_network_choice_valid(Network::MainNet)
            .or_else(|_| crate::network_check::is_network_choice_valid(Network::NextNet))
            .or_else(|_| crate::network_check::is_network_choice_valid(Network::Esmeralda))
            .unwrap();
        for key in ["__tari_overrides[0]", "__tari_overrides"] {
            let overrides = TestOverrides(vec![(key.to_string(), "x".to_string())]);
            let err = load_configuration_with_overrides(&path, &overrides, Some(network)).unwrap_err();
            assert!(err.to_string().contains("__tari_overrides"), "{err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn group_writable_severity_follows_group_membership() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let file = dir.path().join("config.toml");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o664)).unwrap();
        let gid = fs::metadata(&file).unwrap().gid();

        // Work out the expected answer from the same lookups, so the test is deterministic on any host
        // SAFETY: getegid has no preconditions and cannot fail.
        let egid = unsafe { libc::getegid() };
        let shared_on_macos = cfg!(target_os = "macos") && (gid == 20 || gid == 80);
        let only_us = match (current_user_name(), group_members(gid)) {
            (Some(user), Some(members)) => members.iter().all(|m| *m == user),
            _ => false,
        };
        let expected_private = gid == egid && !shared_on_macos && only_us;
        assert_eq!(is_private_group(gid), expected_private);

        let group_finding = untrusted_findings(&file)
            .into_iter()
            .find(|(_, reason)| reason.contains("writable by its group"))
            .expect("a group-writable finding");
        assert_eq!(group_finding.0, !expected_private);

        // World-writable is always severe
        fs::set_permissions(&file, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(!untrusted_severe_reasons(&file).is_empty());
    }

    #[test]
    fn invisible_and_bidi_characters_are_escaped() {
        assert_eq!(sanitize_for_display("a\u{202E}b"), "a\\u{202e}b");
        assert_eq!(sanitize_for_display("a\u{200B}b"), "a\\u{200b}b");
        assert_eq!(sanitize_for_display("a\u{2028}b"), "a\\u{2028}b");
        assert_eq!(sanitize_for_display("\u{FEFF}x\u{2066}"), "\\u{feff}x\\u{2066}");
        // Ordinary non-ASCII text is kept
        assert_eq!(sanitize_for_display("café ✓"), "café ✓");

        let long = "x".repeat(1000);
        let shown = sanitize_for_display(&long);
        assert_eq!(shown, format!("{}…(488 more)", "x".repeat(512)));
    }

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(
            sanitize_for_display("a\u{1b}c\r\n\u{7f}\u{9b}b"),
            "a\\u{1b}c\\u{d}\\u{a}\\u{7f}\\u{9b}b"
        );
        assert_eq!(sanitize_for_display("plain text"), "plain text");

        let mut out = Vec::new();
        write_warning(&mut out, "key \u{1b}c and \u{1b}[2J");
        let out = String::from_utf8(out).unwrap();
        assert!(!out.contains('\u{1b}'));
        assert!(out.contains("\\u{1b}c"));
        assert!(out.contains("\\u{1b}[2J"));
    }

    #[cfg(unix)]
    #[test]
    fn planted_key_and_path_escapes_are_escaped() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        // A quoted TOML key holding an escape sequence, in a world-readable file
        let path = dir.path().join("config.toml");
        fs::write(&path, "[wallet]\n\"password\\u001bc\" = \"x\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let file_cfg = Config::builder()
            .add_source(config::File::from(path.as_path()))
            .build()
            .unwrap();
        let messages = secrets_readable_messages(&path, &file_cfg);
        assert!(!messages.is_empty());
        for message in &messages {
            assert!(!message.contains('\u{1b}'), "{message}");
            assert!(message.contains("\\u{1b}c"), "{message}");
        }

        // A directory name with an escape sequence, writable by others
        let bad_dir = dir.path().join("x\u{1b}[2Jy");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::set_permissions(&bad_dir, fs::Permissions::from_mode(0o777)).unwrap();
        let file = bad_dir.join("log4rs.yml");
        fs::write(&file, "").unwrap();
        let reasons = untrusted_reasons(&file);
        assert!(!reasons.is_empty());
        for reason in &reasons {
            assert!(!reason.contains('\u{1b}'), "{reason}");
        }
        assert!(reasons.iter().any(|r| r.contains("\\u{1b}[2J")), "{reasons:?}");
    }

    #[test]
    fn url_credentials_in_config_are_secrets() {
        let cfg = Config::builder()
            .add_source(config::File::from_str(
                r#"
[merge_mining_proxy]
monerod_url = ["http://node.example:18081", "http://user:pass@host:18081"]
base_node_grpc_address = "http://u:p@127.0.0.1:18142"
p2pool_node_grpc_address = "http://127.0.0.1:18145"
"#,
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let mut found = find_secret_keys(&cfg);
        found.sort();
        assert_eq!(found, vec![
            "merge_mining_proxy.base_node_grpc_address".to_string(),
            "merge_mining_proxy.monerod_url".to_string(),
        ]);
    }

    #[test]
    fn security_warnings_are_counted_and_stored() {
        let before = warnings_emitted();
        emit_security_warning("unique security warning \u{1b}x");
        assert!(warnings_emitted() > before);
        assert!(security_warnings().contains(&"unique security warning \\u{1b}x".to_string()));

        emit_warning("unique routine warning");
        assert!(!security_warnings().iter().any(|w| w.contains("unique routine warning")));
    }

    /// Returns true if a recorded security warning mentions `needle`. Tests run in parallel and share the record, so
    /// they look for their own unique temp paths instead of comparing counts.
    fn recorded_security_warning_mentions(needle: &str) -> bool {
        security_warnings().iter().any(|w| w.contains(needle))
    }

    #[cfg(unix)]
    #[test]
    fn only_severe_findings_are_recorded() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

        // Group-writable, owned by us: always printed; recorded only if the group is shared
        let group = dir.path().join("group_writable.toml");
        fs::write(&group, "").unwrap();
        fs::set_permissions(&group, fs::Permissions::from_mode(0o664)).unwrap();
        let private = is_private_group(fs::metadata(&group).unwrap().gid());
        assert!(!untrusted_reasons(&group).is_empty());
        assert_eq!(untrusted_severe_reasons(&group).is_empty(), private);
        warn_if_untrusted(&group);
        assert_eq!(
            recorded_security_warning_mentions(&group.display().to_string()),
            !private
        );

        // World-writable: recorded
        let world = dir.path().join("world_writable.toml");
        fs::write(&world, "").unwrap();
        fs::set_permissions(&world, fs::Permissions::from_mode(0o666)).unwrap();
        let before = warnings_emitted();
        warn_if_untrusted(&world);
        assert!(warnings_emitted() > before);
        assert!(recorded_security_warning_mentions(&world.display().to_string()));

        // A readable secret: recorded
        let secret = dir.path().join("secret.toml");
        fs::write(&secret, "[wallet]\npassword = \"x\"\n").unwrap();
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
        let file_cfg = Config::builder()
            .add_source(config::File::from(secret.as_path()))
            .build()
            .unwrap();
        warn_if_secrets_readable(&secret, &file_cfg);
        assert!(recorded_security_warning_mentions(&format!(
            "{} is readable by other users",
            secret.display()
        )));
    }

    #[test]
    fn network_key_mismatch_from_file_only_warns() {
        // Shared config file with `[miner] network = "esmeralda"`, node started with --network localnet
        let cfg = Config::builder()
            .add_source(config::File::from_str(
                "[miner]\nnetwork = \"esmeralda\"\n[localnet.wallet]\nnetwork = \"mainnet\"\n",
                config::FileFormat::Toml,
            ))
            .set_override("base_node.network", "localnet")
            .unwrap()
            .build()
            .unwrap();
        let override_keys = vec!["base_node.network".to_string()];
        assert!(warn_about_file_network_keys(&cfg, Network::LocalNet, &override_keys).is_ok());
    }

    #[test]
    fn applicable_network_keys() {
        assert!(is_applicable_network_key("base_node.network", Network::Esmeralda));
        assert!(is_applicable_network_key(
            "esmeralda.base_node.network",
            Network::Esmeralda
        ));
        assert!(!is_applicable_network_key(
            "mainnet.base_node.network",
            Network::Esmeralda
        ));
        assert!(!is_applicable_network_key("network", Network::Esmeralda));
        assert!(!is_applicable_network_key("base_node.grpc_address", Network::Esmeralda));
    }

    #[test]
    fn contradicting_p_override_is_rejected_on_load() {
        // A -p miner.network=<other> that replaced the app-injected miner.network=<resolved>
        let network = crate::network_check::is_network_choice_valid(Network::MainNet)
            .or_else(|_| crate::network_check::is_network_choice_valid(Network::NextNet))
            .or_else(|_| crate::network_check::is_network_choice_valid(Network::Esmeralda))
            .unwrap();
        let other = if network == Network::MainNet {
            Network::Esmeralda
        } else {
            Network::MainNet
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[miner]\n").unwrap();
        let overrides = TestOverrides(vec![("miner.network".to_string(), other.to_string())]);
        assert!(load_configuration_with_overrides(&path, &overrides, Some(network)).is_err());
        let overrides = TestOverrides(vec![("miner.network".to_string(), network.to_string())]);
        assert!(load_configuration_with_overrides(&path, &overrides, Some(network)).is_ok());
    }

    #[test]
    fn secret_keys_are_found() {
        let cfg = Config::builder()
            .add_source(config::File::from_str(
                r#"
[wallet]
password = "hunter2"
grpc_authentication = { username = "admin", password = "x" }
p2p.transport.tor.control_auth = "auto"
p2p.transport.tor.socks_auth = "none"
p2p.transport.socks.auth = "username_password=u:p"
[merge_mining_proxy]
monerod_password = ""
monerod_use_auth = true
base_node_grpc_authentication = { username = "miner", password = "y" }
[mainnet.wallet]
password = "scoped"
"#,
                config::FileFormat::Toml,
            ))
            .build()
            .unwrap();
        let mut found = find_secret_keys(&cfg);
        found.sort();
        assert_eq!(found, vec![
            "mainnet.wallet.password".to_string(),
            "merge_mining_proxy.base_node_grpc_authentication".to_string(),
            "wallet.grpc_authentication".to_string(),
            "wallet.p2p.transport.socks.auth".to_string(),
            "wallet.password".to_string(),
        ]);
    }

    #[cfg(unix)]
    #[test]
    fn warn_if_untrusted_only_warns() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[wallet]\npassword = \"x\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        warn_if_untrusted(&path);
        assert!(!untrusted_reasons(&path).is_empty());
        warn_if_untrusted(&dir.path().join("does_not_exist.toml"));
        let file_cfg = Config::builder()
            .add_source(config::File::from(path.as_path()))
            .build()
            .unwrap();
        warn_if_secrets_readable(&path, &file_cfg);
    }

    struct TestOverrides(Vec<(String, String)>);
    impl ConfigOverrideProvider for TestOverrides {
        fn get_config_property_overrides(&self, _network: &Network) -> Vec<(String, String)> {
            self.0.clone()
        }
    }

    #[derive(Default, serde::Serialize, Deserialize)]
    struct LoaderTestConfig {
        overridden: String,
        scoped_only: String,
    }
    impl crate::SubConfigPath for LoaderTestConfig {
        fn main_key_prefix() -> &'static str {
            "loader_test"
        }
    }

    #[test]
    fn loaded_config_carries_overrides_past_the_scoped_table() {
        use crate::DefaultConfigLoader;

        // Use the network this build allows, since loading sets the process-wide network
        let network = crate::network_check::is_network_choice_valid(Network::MainNet)
            .or_else(|_| crate::network_check::is_network_choice_valid(Network::NextNet))
            .or_else(|_| crate::network_check::is_network_choice_valid(Network::Esmeralda))
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            format!(
                "[loader_test]\noverride_from = \"{network}\"\noverridden = \"file\"\nscoped_only = \
                 \"file\"\n\n[{network}.loader_test]\noverridden = \"scoped\"\nscoped_only = \"scoped\"\n"
            ),
        )
        .unwrap();
        let overrides = TestOverrides(vec![("loader_test.overridden".to_string(), "from -p".to_string())]);
        let cfg = load_configuration_with_overrides(&path, &overrides, Some(network)).unwrap();
        let loaded = <LoaderTestConfig as DefaultConfigLoader>::load_from(&cfg).unwrap();
        assert_eq!(loaded.overridden, "from -p");
        assert_eq!(loaded.scoped_only, "scoped");
    }

    #[test]
    fn warnings_are_written_with_a_prefix() {
        let mut out = Vec::new();
        write_warning(&mut out, "something is wrong");
        assert_eq!(String::from_utf8(out).unwrap(), "WARNING: something is wrong\n");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_target_directory_is_checked() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let dir = tempfile::tempdir().unwrap();
        let link_dir = dir.path().join("links");
        let target_dir = dir.path().join("targets");
        fs::create_dir_all(&link_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        let target = target_dir.join("config.toml");
        fs::write(&target, "").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&link_dir, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&target_dir, fs::Permissions::from_mode(0o777)).unwrap();
        let link = link_dir.join("config.toml");
        symlink(&target, &link).unwrap();

        let reasons = untrusted_reasons(&link);
        assert!(
            reasons.iter().any(|r| r.contains("directory (symlink hop)")),
            "{reasons:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn every_symlink_hop_directory_is_checked() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        // a/config.toml -> b/hop.toml -> c/config.toml, with only b writable by others
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let c = dir.path().join("c");
        for d in [&a, &b, &c] {
            fs::create_dir_all(d).unwrap();
            fs::set_permissions(d, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::set_permissions(&b, fs::Permissions::from_mode(0o777)).unwrap();
        let file = c.join("config.toml");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&file, b.join("hop.toml")).unwrap();
        // Relative hop, resolved against the link's own directory
        symlink("../b/hop.toml", a.join("config.toml")).unwrap();

        let reasons = untrusted_reasons(&a.join("config.toml"));
        let b_shown = fs::canonicalize(&b).unwrap().display().to_string();
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("directory (symlink hop)") && r.contains(&b_shown) && r.contains("writable")),
            "{reasons:?}"
        );
        // The final target's directory is fine
        let c_shown = fs::canonicalize(&c).unwrap().display().to_string();
        assert!(!reasons.iter().any(|r| r.contains(&c_shown)), "{reasons:?}");
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_is_reported() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("config.toml");
        symlink(dir.path().join("missing.toml"), &link).unwrap();
        let reasons = untrusted_reasons(&link);
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("symlink whose target does not exist")),
            "{reasons:?}"
        );
        // Still fail closed: nothing is created through the link
        write_config_to(&link, &["x"]).unwrap();
        assert!(!dir.path().join("missing.toml").exists());
    }

    #[test]
    fn malformed_env_keys_are_rejected() {
        assert!(!is_valid_config_key("base_node..x"));
        assert!(!is_valid_config_key("base_node.x."));
        assert!(is_valid_config_key("base_node.x"));
    }

    #[test]
    fn write_config_to_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("config.toml");
        write_config_to(&path, &["first"]).unwrap();
        write_config_to(&path, &["second"]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\n");
    }
}
