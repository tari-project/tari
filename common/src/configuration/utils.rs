//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{fmt, fmt::Display, fs, fs::File, io::Write, marker::PhantomData, path::Path, str::FromStr};

use config::Config;
use log::{debug, info};
use multiaddr::{Multiaddr, Protocol};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize,
    Deserializer,
    Serializer,
};

use crate::{ConfigError, LOG_TARGET};

//-------------------------------------           Main API functions         --------------------------------------//

pub fn load_configuration(
    config_path: &Path,
    create_if_not_exists: bool,
    overrides: &[(String, String)],
) -> Result<Config, ConfigError> {
    debug!(
        target: LOG_TARGET,
        "Loading configuration file from  {}",
        config_path.to_str().unwrap_or("[??]")
    );
    if !config_path.exists() && create_if_not_exists {
        write_default_config_to(config_path)
            .map_err(|io| ConfigError::new("Could not create default config", Some(io.to_string())))?;
    }
    let filename = config_path
        .to_str()
        .ok_or_else(|| ConfigError::new("Invalid config file path", None))?;
    let mut cfg = Config::builder()
        .add_source(config::File::with_name(filename))
        .add_source(config::Environment::with_prefix("TARI"));

    for (key, value) in overrides {
        cfg = cfg
            .set_override(key.as_str(), value.as_str())
            .map_err(|ce| ConfigError::new("Could not override config property", Some(ce.to_string())))?;
    }

    let cfg = cfg
        .build()
        .map_err(|ce| ConfigError::new("Could not build config", Some(ce.to_string())))?;
    info!(target: LOG_TARGET, "Configuration file loaded.");

    Ok(cfg)
}

/// Installs a new configuration file template, copied from the application type's preset and written to the given path.
/// Also includes the common configuration defined in `config/presets/common.toml`.
pub fn write_default_config_to(path: &Path) -> Result<(), std::io::Error> {
    // Use the same config file so that all the settings are easier to find, and easier to
    // support users over chat channels
    let common = include_str!("../../config/presets/common.toml");
    let source = [
        common,
        include_str!("../../config/presets/base_node.toml"),
        include_str!("../../config/presets/console_wallet.toml"),
        include_str!("../../config/presets/mining_node.toml"),
        include_str!("../../config/presets/merge_mining_proxy.toml"),
        include_str!("../../config/presets/stratum_transcoder.toml"),
        include_str!("../../config/presets/validator_node.toml"),
        include_str!("../../config/presets/collectibles.toml"),
    ]
    .join("\n");

    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?
    };
    let mut file = File::create(path)?;
    file.write_all(source.as_ref())
}

pub fn get_local_ip() -> Option<Multiaddr> {
    use std::net::IpAddr;

    get_if_addrs::get_if_addrs().ok().and_then(|if_addrs| {
        if_addrs
            .into_iter()
            .find(|if_addr| !if_addr.is_loopback())
            .map(|if_addr| {
                let mut addr = Multiaddr::empty();
                match if_addr.ip() {
                    IpAddr::V4(ip) => {
                        addr.push(Protocol::Ip4(ip));
                    },
                    IpAddr::V6(ip) => {
                        addr.push(Protocol::Ip6(ip));
                    },
                }
                addr
            })
    })
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
