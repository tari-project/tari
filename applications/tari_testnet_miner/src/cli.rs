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

use clap::clap_app;
use std::path::Path;
use tari_common::{bootstrap_config_from_cli, ConfigBootstrap};

pub const VERSION: &str = "0.0.5";
pub const AUTHOR: &str = "The Tari Community";

/// Prints a pretty banner on the console
pub fn print_banner() {
    println!(
        "\n$ Tari Base Node\n$ Copyright 2019-2020. {}\n$ Version {}\n\nPress Ctrl-C to quit..",
        AUTHOR, VERSION
    );
}

/// Parsed command-line arguments
pub struct Arguments {
    pub bootstrap: ConfigBootstrap,
    pub create_id: bool,
}

/// Parse the command-line args and populate the minimal bootstrap config object
pub fn parse_cli_args() -> Arguments {
    let matches = clap_app!(myapp =>
        (version: VERSION)
        (author: AUTHOR)
        (about: "The reference Tari cryptocurrency base node implementation")
        (@arg config: -c --config +takes_value "A path to the configuration file to use (config.toml)")
        (@arg log_config: -l --log_config +takes_value "A path to the logfile configuration (log4rs.yml))")
        (@arg init: --init "Create a default configuration file if it doesn't exist")
        (@arg create_id: --create_id "Create and save new node identity if one doesn't exist ")
    )
    .get_matches();

    let bootstrap = bootstrap_config_from_cli(&matches);
    let create_id = matches.is_present("create_id");

    Arguments { bootstrap, create_id }
}

fn exists(s: String) -> Result<(), String> {
    let path = Path::new(&s);
    if path.exists() {
        Ok(())
    } else {
        Err(format!("{} does not exist", s))
    }
}
