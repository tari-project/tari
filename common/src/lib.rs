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

use dirs;
use std::{
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
};

mod configuration;

pub use configuration::{default_config, ConfigurationError, DatabaseType, Network, NodeBuilderConfig};

/// Determine the path to a log configuration file using the following precedence rules:
/// 1. Use the provided path (usually pulled from a CLI argument)
/// 2. Use the value in the `TARI_LOG_CONFIGURATION` envar
/// 3. The default path (OS-dependent), "~/.tari/log4rs.toml`
/// 4. The current directory
pub fn get_log_configuration_path(cli_path: Option<PathBuf>) -> PathBuf {
    cli_path
        .or_else(|| {
            env::var_os("TARI_LOG_CONFIGURATION")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| dirs::home_dir().map(|path| path.join(".tari/log4rs.yml")))
        .or_else(|| {
            Some(env::current_dir().expect(
                "Could find a suitable path to the log configuration file. Consider setting the \
                 TARI_LOG_CONFIGURATION envar, or check that the current directory exists and that you have \
                 permission to read it",
            ))
        })
        .unwrap()
}

/// Create the default data directory (`~/.tari` on OSx and Linux, for example) if it doesn't already exist
pub fn create_data_directory() -> Result<(), std::io::Error> {
    let mut home = dirs::home_dir().ok_or(std::io::Error::from(ErrorKind::NotFound))?;
    home.push(".tari");
    if !home.exists() {
        std::fs::create_dir(home)
    } else {
        Ok(())
    }
}

/// A convenience function for creating subfolders inside the `~/.tari` default data directory
///
/// # Panics
/// This function panics if the home folder location cannot be found or if the path value is not valid UTF-8.
/// This is a trade-off made in favour of convenience of use.
pub fn default_subdir(path: &str) -> String {
    let mut home = dirs::home_dir().expect("Home folder location failed");
    home.push(".tari");
    home.push(path);
    String::from(home.to_str().expect("Invalid path value"))
}

/// Installs a new configuration file template, copied from `tari_config_sample.toml` to the given path.
/// When bundled as a binary, the config sample file must be bundled in `common/config`.
pub fn install_default_config_file(path: &Path) -> Result<u64, std::io::Error> {
    let mut source = env::current_dir()?;
    source.push(Path::new("common/config/tari_config_sample.toml"));
    std::fs::copy(source, path)
}

/// Installs a new default logfile configuration, copied from `log4rs-sample.yml` to the given path.
/// When bundled as a binary, the config sample file must be bundled in `common/config`.
pub fn install_default_logfile_config(path: &Path) -> Result<u64, std::io::Error> {
    let mut source = env::current_dir()?;
    source.push(Path::new("common/logging/log4rs-sample.yml"));
    std::fs::copy(source, path)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::env;

    #[test]
    fn get_log_configuration_path_cli() {
        let path = get_log_configuration_path(Some(PathBuf::from("~/my-tari")));
        assert_eq!(path.to_str().unwrap(), "~/my-tari");
    }

    #[test]
    fn get_log_configuration_path_by_env_var() {
        env::set_var("TARI_LOG_CONFIGURATION", "~/fake-example");
        let path = get_log_configuration_path(None);
        assert_eq!(path.to_str().unwrap(), "~/fake-example");
        env::set_var("TARI_LOG_CONFIGURATION", "");
    }
}
