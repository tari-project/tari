// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt,
    fmt::Display,
    fs,
    fs::OpenOptions,
    io::{ErrorKind, Write},
    marker::PhantomData,
    path::Path,
    str::FromStr,
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
    configuration::{ConfigOverrideProvider, Network, bootstrap::prompt, loader::CONFIG_OVERRIDES_KEY},
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
            config_path.as_ref().display()
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
/// A `-p`, `TARI_*` env or application override that sets a `<section>.network` key to a network other than the
/// resolved one is rejected with an error. The same contradiction coming only from the config file (e.g. a `[miner]`
/// section in a config shared with the node) is only warned about.
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
    for (key, value) in &overrides {
        trace!(target: LOG_TARGET, "Config property override: {key}={}", mask_value(key, value));
    }
    warn_about_network_overrides(&overrides, network);

    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    set_network_if_choice_valid(network)?;

    // Store env and -p/app overrides in the config so that `merge_subconfig` can re-apply them on top of the
    // network-scoped tables. Env first, so that a -p/app override on the same key wins.
    let mut all_overrides = tari_env_overrides();
    all_overrides.extend(overrides.iter().cloned());
    let override_keys: Vec<String> = all_overrides.iter().map(|(key, _)| key.clone()).collect();
    let reapply: Vec<String> = all_overrides
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();

    let mut builder = Config::builder().add_source(cfg);
    for (key, value) in overrides {
        trace!(target: LOG_TARGET, "Set override: ({key}, {})", mask_value(&key, &value));
        builder = builder
            .set_override(key.as_str(), value.as_str())
            .map_err(|ce| ConfigError::new("Could not override config property", Some(ce.to_string())))?;
    }
    let cfg = builder
        .set_override(CONFIG_OVERRIDES_KEY, reapply)
        .map_err(|ce| ConfigError::new("Could not override config property", Some(ce.to_string())))?
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;

    check_network_keys(&cfg, network, &override_keys)?;

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
        if let Some(key) = name.strip_prefix("tari_") {
            result.push((key.replace("__", "."), value.to_string()));
        }
    }
    result
}

/// Warns about overrides that do not do what they look like they do.
fn warn_about_network_overrides(overrides: &[(String, String)], network: Network) {
    for (key, value) in overrides {
        if key == "network" {
            warn!(
                target: LOG_TARGET,
                "The config override 'network={value}' is ignored. Use --network or TARI_NETWORK to choose the network."
            );
        }
        if key == "common.base_path" {
            let last = Path::new(value).file_name().and_then(|n| n.to_str()).unwrap_or("");
            if last != network.as_key_str() {
                warn!(
                    target: LOG_TARGET,
                    "The base path '{value}' does not end in the network name '{network}', but the network is \
                     '{network}'. Check --base-path and --network."
                );
            }
        }
    }
}

/// Checks unscoped `<section>.network` keys (e.g. `base_node.network`) and keys scoped to the resolved network (e.g.
/// `mainnet.base_node.network` when running mainnet) against the resolved `network`. Tables scoped to other networks
/// are ignored. The top-level `network` key is the input to network resolution and is not checked here. Values that
/// are not a network name are left for the section's own deserialization to report.
///
/// A mismatch is an error only if the key was set by an explicit override (`override_keys`: `-p`, `TARI_*` env or
/// application-injected). A mismatch that comes only from the config file is logged as a warning, so a shared config
/// with, say, `[miner] network = "esmeralda"` does not stop a node started with another `--network`.
fn check_network_keys(cfg: &Config, network: Network, override_keys: &[String]) -> Result<(), ConfigError> {
    let root = cfg
        .cache
        .clone()
        .into_table()
        .map_err(|ce| ConfigError::new("Could not read config", Some(ce.to_string())))?;
    let mut leaves = Vec::new();
    collect_leaves("", root, &mut leaves);
    for (key, value) in leaves {
        if key == "network" || !key.ends_with(".network") {
            continue;
        }
        // A table scoped to another network (e.g. `[mainnet.base_node]` while running esmeralda) is never used, so
        // its `network` key does not matter.
        let first_segment = key.split('.').next().unwrap_or("");
        if let Ok(scope) = Network::from_str(first_segment) &&
            scope != network
        {
            continue;
        }
        let Ok(value) = value.into_string() else {
            continue;
        };
        let Ok(configured) = Network::from_str(&value) else {
            continue;
        };
        if configured == network {
            continue;
        }
        if override_keys.iter().any(|k| k.eq_ignore_ascii_case(&key)) {
            return Err(ConfigError::new(
                "Conflicting network configuration",
                Some(format!(
                    "Config key {key} is set to {configured} but the network is {network}; use --network or \
                     TARI_NETWORK"
                )),
            ));
        }
        warn!(
            target: LOG_TARGET,
            "Config key {key} is set to {configured} in the config file but the network is {network}. The value is \
             ignored for this run; use --network or TARI_NETWORK to choose the network."
        );
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

/// Returns true if the last segment of a config key names a secret.
fn is_secret_key(key: &str) -> bool {
    let last = key.rsplit('.').next().unwrap_or(key).to_lowercase();
    last == "password" ||
        last.ends_with("_password") ||
        last.ends_with("_authentication") ||
        last.ends_with("control_auth") ||
        last.ends_with("socks_auth")
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
                ValueKind::Nil => false,
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
        } else if let ValueKind::Table(table) = value.kind {
            find_secret_keys_in(&full_key, table, found);
        } else {
            // Not a secret and not a table
        }
    }
}

/// Warns if the config file holds a secret but can be read by the group or by other users.
#[cfg(unix)]
fn warn_if_secrets_readable(path: &Path, file_cfg: &Config) {
    use std::os::unix::fs::MetadataExt;

    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.mode() & 0o044 == 0 {
        return;
    }
    for key in find_secret_keys(file_cfg) {
        warn!(
            target: LOG_TARGET,
            "⚠️  Config file {} is readable by other users (mode {:o}) and contains the secret '{}'. Run `chmod 600 {}`, \
             or move the secret to an environment variable or command line argument.",
            path.display(),
            metadata.mode() & 0o7777,
            key,
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn warn_if_secrets_readable(_path: &Path, _file_cfg: &Config) {}

/// Warns if a file that is about to be loaded could have been written by someone else. See [`untrusted_reasons`].
/// This only warns; loading always continues.
pub fn warn_if_untrusted(path: &Path) {
    for reason in untrusted_reasons(path) {
        warn!(target: LOG_TARGET, "⚠️  {reason}");
    }
}

/// Returns why a file that is about to be loaded (config or log config) could have been written by someone else, or
/// an empty list if it looks fine. On Unix the file (following symlinks) and its immediate parent directory must be
/// owned by the effective user and must not be group- or world-writable. On other platforms this returns nothing.
#[cfg(unix)]
pub fn untrusted_reasons(path: &Path) -> Vec<String> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let mut reasons = Vec::new();
    for (kind, p) in [("file", path), ("directory", parent)] {
        let Ok(metadata) = fs::metadata(p) else {
            continue;
        };
        let mode = metadata.mode() & 0o7777;
        if metadata.uid() != euid {
            reasons.push(format!(
                "The {} {} is owned by uid {} (mode {:o}), not by the current user (uid {}). Another user could \
                 control its contents. Fix with `chown {} {}` or use a base path you own.",
                kind,
                p.display(),
                metadata.uid(),
                mode,
                euid,
                euid,
                p.display()
            ));
        }
        if mode & 0o022 != 0 {
            reasons.push(format!(
                "The {} {} (owner uid {}) is writable by other users (mode {:o}). Another user could control its \
                 contents. Fix with `chmod go-w {}`.",
                kind,
                p.display(),
                metadata.uid(),
                mode,
                p.display()
            ));
        }
    }
    reasons
}

#[cfg(not(unix))]
pub fn untrusted_reasons(_path: &Path) -> Vec<String> {
    Vec::new()
}

/// Returns the value to display for a config key or environment variable. Values are masked as `***` unless the last
/// key segment (split on `.` and `__`, case-insensitive, with any `tari_`/`minotari_` prefix removed) is known to be
/// safe to show: `*_address`, `*_port`, `*_enabled`, `*_interval`, `*_timeout`, `*_path`, `*_dir`, `*_url` (with any
/// `user:pass@` removed), `network`, `base_path` or `override_from`. Otherwise boolean and numeric values are shown.
/// Keys that name a secret (password, passphrase, secret, seed, auth, token, cookie, mnemonic, private, ...) are
/// always masked, whatever the value.
pub fn mask_value(key: &str, value: &str) -> String {
    let lower = key.to_lowercase();
    let last = lower.rsplit('.').next().unwrap_or("");
    let last = last.rsplit("__").next().unwrap_or(last);
    let last = last
        .strip_prefix("minotari_")
        .or_else(|| last.strip_prefix("tari_"))
        .unwrap_or(last);

    // Secrets are masked before anything else, so a numeric password or PIN is never shown
    let secret_words = [
        "password",
        "passphrase",
        "secret",
        "seed",
        "auth",
        "token",
        "cookie",
        "mnemonic",
        "private",
    ];
    if is_secret_key(last) || secret_words.iter().any(|w| last.contains(w)) {
        return "***".to_string();
    }
    let trimmed = value.trim();
    if trimmed.parse::<bool>().is_ok() || trimmed.parse::<f64>().is_ok() {
        return value.to_string();
    }
    if last.ends_with("_url") {
        return value
            .split(',')
            .map(strip_url_credentials)
            .collect::<Vec<_>>()
            .join(",");
    }
    let safe_suffixes = [
        "_address",
        "_port",
        "_enabled",
        "_interval",
        "_timeout",
        "_path",
        "_dir",
    ];
    let safe_names = ["network", "base_path", "override_from"];
    if safe_suffixes.iter().any(|s| last.ends_with(s)) || safe_names.contains(&last) {
        return value.to_string();
    }
    "***".to_string()
}

/// Removes a `user:pass@` part from the authority of a URL, e.g. `http://u:p@host:18081` → `http://host:18081`.
fn strip_url_credentials(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (format!("{scheme}://"), rest),
        None => (String::new(), url),
    };
    let (authority, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    match authority.rsplit_once('@') {
        Some((_credentials, host)) => format!("{scheme}{host}{path}"),
        None => url.to_string(),
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
                    var_name,
                    mask_value(&var_name, &var_value),
                    correct_prefix,
                    suffix
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
            let display_value = mask_value(&k, &v);
            (k, display_value)
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
            let display_value = mask_value(key, value);
            println!("  {key}={display_value}");
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
        // URL credentials are stripped
        assert_eq!(
            mask_value(
                "merge_mining_proxy.monerod_url",
                "http://user:pass@node.example:18081/json_rpc"
            ),
            "http://node.example:18081/json_rpc"
        );
        assert_eq!(
            mask_value("merge_mining_proxy.monerod_url", "http://a:b@one:1,https://two:2"),
            "http://one:1,https://two:2"
        );
        assert_eq!(mask_value("x.some_url", "https://node.example"), "https://node.example");
    }

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn network_key_mismatch_from_override_is_an_error() {
        // -p wallet.network=esmeralda on a mainnet resolve
        let cfg = Config::builder()
            .set_override("wallet.network", "esmeralda")
            .unwrap()
            .build()
            .unwrap();
        let err = check_network_keys(&cfg, Network::MainNet, &keys(&["wallet.network"])).unwrap_err();
        assert!(err.to_string().contains("wallet.network"));
        assert!(check_network_keys(&cfg, Network::Esmeralda, &keys(&["wallet.network"])).is_ok());

        // TARI_MINER__NETWORK=esmeralda, as it appears in the env-override list
        let cfg = Config::builder()
            .set_override("miner.network", "esmeralda")
            .unwrap()
            .build()
            .unwrap();
        assert!(check_network_keys(&cfg, Network::MainNet, &keys(&["miner.network"])).is_err());

        // An override in the resolved network's scoped table
        let cfg = Config::builder()
            .set_override("mainnet.wallet.network", "nextnet")
            .unwrap()
            .build()
            .unwrap();
        let err = check_network_keys(&cfg, Network::MainNet, &keys(&["mainnet.wallet.network"])).unwrap_err();
        assert!(err.to_string().contains("mainnet.wallet.network"));

        // App-injected override equal to the resolved network
        let cfg = Config::builder()
            .set_override("base_node.network", "localnet")
            .unwrap()
            .build()
            .unwrap();
        assert!(check_network_keys(&cfg, Network::LocalNet, &keys(&["base_node.network"])).is_ok());
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
        assert!(check_network_keys(&cfg, Network::LocalNet, &keys(&["base_node.network"])).is_ok());
    }

    #[test]
    fn network_keys_in_other_network_tables_are_ignored() {
        let cfg = Config::builder()
            .set_override("mainnet.base_node.network", "mainnet")
            .unwrap()
            .set_override("nextnet.wallet.network", "esmeralda")
            .unwrap()
            .build()
            .unwrap();
        let override_keys = keys(&["mainnet.base_node.network", "nextnet.wallet.network"]);
        assert!(check_network_keys(&cfg, Network::Esmeralda, &override_keys).is_ok());

        // The top-level key is not checked here
        let cfg = Config::builder()
            .set_override("network", "nextnet")
            .unwrap()
            .build()
            .unwrap();
        assert!(check_network_keys(&cfg, Network::MainNet, &keys(&["network"])).is_ok());
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
[merge_mining_proxy]
monerod_password = ""
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
    fn write_config_to_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("config.toml");
        write_config_to(&path, &["first"]).unwrap();
        write_config_to(&path, &["second"]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first\n");
    }
}
