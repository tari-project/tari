// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, fmt::Display, fs, fs::File, io::Write, marker::PhantomData, path::Path, str::FromStr};

use config::Config;
use log::{debug, info};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize,
    Deserializer,
    Serializer,
};

use crate::{
    configuration::{bootstrap::prompt, ConfigOverrideProvider, Network},
    network_check::set_network_if_choice_valid,
    ConfigError,
    LOG_TARGET,
};

//-------------------------------------           Main API functions         --------------------------------------//

/// Loads the configuration file from the specified path, or creates a new one with the embedded default presets if it
/// does not. This also prompts the user.
pub fn load_configuration<P: AsRef<Path>, TOverride: ConfigOverrideProvider>(
    config_path: P,
    create_if_not_exists: bool,
    non_interactive: bool,
    overrides: &TOverride,
) -> Result<Config, ConfigError> {
    debug!(
        target: LOG_TARGET,
        "Loading configuration file from  {}",
        config_path.as_ref().display()
    );
    if !config_path.as_ref().exists() && create_if_not_exists {
        let sources = if non_interactive {
            get_default_config(false)
        } else {
            prompt_default_config()
        };
        write_config_to(&config_path, &sources)
            .map_err(|io| ConfigError::new("Could not create default config", Some(io.to_string())))?;
    }

    load_configuration_with_overrides(config_path, overrides)
}

/// Loads the config at the given path applying all overrides.
pub fn load_configuration_with_overrides<P: AsRef<Path>, TOverride: ConfigOverrideProvider>(
    config_path: P,
    overrides: &TOverride,
) -> Result<Config, ConfigError> {
    let filename = config_path
        .as_ref()
        .to_str()
        .ok_or_else(|| ConfigError::new("Invalid config file path", None))?;
    let cfg = Config::builder()
        .add_source(config::File::with_name(filename))
        .add_source(
            config::Environment::with_prefix("TARI")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;

    let mut network = match cfg.get_string("network") {
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
    };

    info!(target: LOG_TARGET, "Configuration file loaded.");
    let overrides = overrides.get_config_property_overrides(&mut network);
    // Set the static network variable according to the user chosen network (for use with
    // `get_current_or_user_setting_or_default()`) -
    set_network_if_choice_valid(network)?;

    if overrides.is_empty() {
        return Ok(cfg);
    }

    let mut cfg = Config::builder().add_source(cfg);
    for (key, value) in overrides {
        cfg = cfg
            .set_override(key.as_str(), value.as_str())
            .map_err(|ce| ConfigError::new("Could not override config property", Some(ce.to_string())))?;
    }
    let cfg = cfg
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;

    Ok(cfg)
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
/// exist, it is created. If the file already exists, it is overwritten.
pub fn write_config_to<P: AsRef<Path>>(path: P, sources: &[&str]) -> Result<(), std::io::Error> {
    if let Some(d) = path.as_ref().parent() {
        fs::create_dir_all(d)?
    };
    let mut file = File::create(path)?;
    for source in sources {
        file.write_all(source.as_bytes())?;
        file.write_all(b"\n")?;
    }
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
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where M: MapAccess<'de> {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}
