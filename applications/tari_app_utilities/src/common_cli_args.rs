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
use std::{error::Error, path::PathBuf};

use clap::Args;
use log::Level;

#[derive(Args, Debug)]
pub struct CommonCliArgs {
    /// A path to a directory to store your files
    #[clap(
        short,
        long,
        aliases = &["base_path", "base_dir", "base-dir"],
        default_value_t= defaults::base_path()
    )]
    base_path: String,
    /// A path to the configuration file to use (config.toml)
    #[clap(short, long, default_value_t= defaults::config())]
    config: String,
    /// The path to the log configuration file
    #[clap(short, long, alias = "log_config")]
    pub log_config: Option<PathBuf>,

    #[clap()]
    pub log_level: Option<Level>,

    /// Overrides for properties in the config file, e.g. -p base_node.netwok=dibbler
    #[clap(short = 'p', parse(try_from_str = parse_key_val), multiple_occurrences(true))]
    pub config_property_overrides: Vec<(String, String)>,
}

// Taken from clap examples
/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let mut parts = s.split("=").map(|s| s.trim());
    let k = parts.next().ok_or_else(|| "invalid override: string empty`")?;
    let v = parts
        .next()
        .ok_or_else(|| format!("invalid override: expected key=value: no `=` found in `{}`", s))?;
    Ok((k.parse()?, v.parse()?))
}

impl CommonCliArgs {
    pub fn config_path(&self) -> PathBuf {
        let config_path = PathBuf::from(&self.config);
        if config_path.is_absolute() {
            config_path
        } else {
            let mut base_path = PathBuf::from(&self.base_path);
            base_path.push(config_path);
            base_path
        }
    }

    pub fn get_base_path(&self) -> PathBuf {
        PathBuf::from(&self.base_path)
    }

    pub fn log_config_path(&self, application_name: &str) -> PathBuf {
        if let Some(ref log_config) = self.log_config {
            let path = PathBuf::from(log_config);
            if path.is_absolute() {
                log_config.clone()
            } else {
                let mut base_path = PathBuf::from(&self.base_path);
                base_path.push(log_config);
                base_path
            }
        } else {
            let mut path = PathBuf::from(&self.base_path);
            path.push("config");
            path.push(application_name);
            path.push("log4rs.yml");
            path
        }
    }

    pub fn config_property_overrides(&self) -> Vec<(String, String)> {
        let mut overrides = self.config_property_overrides.clone();
        overrides.push(("common.base_path".to_string(), self.base_path.clone()));
        overrides
    }
}

mod defaults {
    use tari_common::dir_utils;

    const DEFAULT_CONFIG: &str = "config/config.toml";

    pub(super) fn base_path() -> String {
        dir_utils::default_path("", None).to_string_lossy().to_string()
    }

    pub(super) fn config() -> String {
        DEFAULT_CONFIG.to_string()
    }
}
