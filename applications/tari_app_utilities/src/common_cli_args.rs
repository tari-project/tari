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
use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug)]
pub struct CommonCliArgs {
    /// A path to a directory to store your files
    #[clap(
    short,
    long,
    aliases = &["base_path", "base_dir", "base-dir"],
    default_value_t= Defaults::base_path()
    )]
    base_path: String,
    /// A path to the configuration file to use (config.toml)
    #[clap(short, long, default_value_t= Defaults::config())]
    config: String,
    /// The path to the log configuration file
    #[clap(short, long, alias = "log_config")]
    pub log_config: Option<PathBuf>,
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
}

mod Defaults {
    use std::env;

    use tari_common::dir_utils;

    const DEFAULT_CONFIG: &str = "config/config.toml";

    pub(super) fn base_path() -> String {
        dir_utils::default_path("", None).to_string_lossy().to_string()
    }

    pub(super) fn config() -> String {
        DEFAULT_CONFIG.to_string()
    }
}
