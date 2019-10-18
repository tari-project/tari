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

use crate::consts;
use clap::clap_app;
use dirs;
use std::path::{Path, PathBuf};

/// A minimal parsed configuration object that's used to bootstrap the main Configuration.
pub struct ConfigBootstrap {
    pub config: PathBuf,
    /// The path to the log configuration file. It is set using the following precedence set:
    ///   1. from the command-line parameter,
    ///   2. from the `TARI_LOG_CONFIGURATION` environment variable,
    ///   3. from a default value, usually `~/.tari/log4rs.yml` (or OS equivalent).
    pub log_config: PathBuf,
}

/// Parse the command-line args and populate the minimal bootstrap config object
pub fn parse_cli_args() -> ConfigBootstrap {
    let matches = clap_app!(myapp =>
        (version: consts::VERSION)
        (author: consts::AUTHOR)
        (about: "The reference Tari cryptocurrency base node implementation")
        (@arg config: -c --config +takes_value {exists} "A path to the configuration file to use (tari_conf.toml)")
        (@arg log_config: -l --log_config +takes_value {exists} "A path to the logfile configuration (log4rs.yml))")
        (@arg init: --init "Create a default configuration file if it doesn't exist")
    )
    .get_matches();

    let config = matches
        .value_of("config")
        .map(PathBuf::from)
        .unwrap_or(default_path(consts::DEFAULT_CONFIG));
    let log_config = matches.value_of("log_config").map(PathBuf::from);
    let log_config = tari_common::get_log_configuration_path(log_config);

    if !config.exists() && matches.is_present("init") {
        println!("Installing new config file at {}", config.to_str().unwrap_or("[??]"));
        install_configuration(&config, tari_common::install_default_config_file);
    }

    if !log_config.exists() && matches.is_present("init") {
        println!(
            "Installing new logfile configuration at {}",
            log_config.to_str().unwrap_or("[??]")
        );
        install_configuration(&log_config, tari_common::install_default_logfile_config);
    }
    ConfigBootstrap { config, log_config }
}

fn exists(s: String) -> Result<(), String> {
    let path = Path::new(&s);
    if path.exists() {
        Ok(())
    } else {
        Err(format!("{} does not exist", s))
    }
}

fn default_path(filename: &str) -> PathBuf {
    let mut home = dirs::home_dir().unwrap_or(PathBuf::from("."));
    home.push(".tari");
    home.push(filename);
    home
}

fn install_configuration<F>(path: &Path, installer: F)
where F: Fn(&Path) -> Result<u64, std::io::Error> {
    if let Err(e) = installer(path) {
        println!(
            "We could not install a new configuration file in {}: {}",
            path.to_str().unwrap_or("?"),
            e.to_string()
        )
    }
}
