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
    ConfigError,
    LOG_TARGET,
};

//-------------------------------------           Main API functions         --------------------------------------//

pub fn load_configuration<P: AsRef<Path>, TOverride: ConfigOverrideProvider>(
    config_path: P,
    create_if_not_exists: bool,
    overrides: &TOverride,
) -> Result<Config, ConfigError> {
    debug!(
        target: LOG_TARGET,
        "Loading configuration file from  {}",
        config_path.as_ref().to_str().unwrap_or("[??]")
    );
    if !config_path.as_ref().exists() && create_if_not_exists {
        write_default_config_to(&config_path)
            .map_err(|io| ConfigError::new("Could not create default config", Some(io.to_string())))?;
    }
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

    let network = match cfg.get_string("network") {
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
    let overrides = overrides.get_config_property_overrides(network);
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

/// Installs a new configuration file template, copied from the application type's preset and written to the given path.
/// Also includes the common configuration defined in `config/presets/common.toml`.
pub fn write_default_config_to<P: AsRef<Path>>(path: P) -> Result<(), std::io::Error> {
    // Use the same config file so that all the settings are easier to find, and easier to
    // support users over chat channels
    let mine = prompt(
        "Node config does not exist.\nWould you like to mine (Y/n)?\nNOTE: this will enable gprc methods that can \
         leak private info on the node",
    );
    let base_node_deny_methods = if mine {
        include_str!("../../config/presets/c_base_node_mining_deny_methods.toml")
    } else {
        include_str!("../../config/presets/c_base_node_non_mining_deny_methods.toml")
    };

    let common = include_str!("../../config/presets/a_common.toml");
    let source = [
        common,
        include_str!("../../config/presets/b_peer_seeds.toml"),
        base_node_deny_methods,
        include_str!("../../config/presets/c_base_node.toml"),
        include_str!("../../config/presets/d_console_wallet.toml"),
        include_str!("../../config/presets/g_miner.toml"),
        include_str!("../../config/presets/f_merge_mining_proxy.toml"),
        include_str!("../../config/presets/e_validator_node.toml"),
        include_str!("../../config/presets/h_collectibles.toml"),
        include_str!("../../config/presets/i_indexer.toml"),
        include_str!("../../config/presets/j_dan_wallet_daemon.toml"),
    ]
    .join("\n");

    if let Some(d) = path.as_ref().parent() {
        fs::create_dir_all(d)?
    };
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
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
