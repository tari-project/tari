// Copyright 2021. The Tari Project
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

use std::{env::JoinPathsError, path::PathBuf};

const TARI_FOLDER: &str = "tari";
const TARI_HIDDEN_FOLDER: &str = ".tari";

pub enum SourceLocation {
    SourceCode(SourceCodeOptions),
    Docker(DockerOptions),
}

pub struct SourceCodeOptions {}

pub struct DockerOptions {}

pub struct InstallLocation {
    config_folder: PathBuf,
    executable_folder: PathBuf,
    data_folder: PathBuf,
}

impl Default for InstallLocation {
    fn default() -> Self {
        let mut home = dirs::home_dir().expect("No default home folder");
        let mut bin: PathBuf;
        let data = dirs::data_dir().expect("No default data folder");
        #[cfg(target_os = "windows")]
        {
            home.push(TARI_FOLDER);
            bin = PathBuf::from("C:\\Program Files");
        }
        #[cfg(any(target_os = "macos", target_os = "unix"))]
        {
            bin = dirs::home_dir().expect("No default home folder");
            bin.push("bin");
            home.push(TARI_HIDDEN_FOLDER);
        }

        Self {
            config_folder: home,
            executable_folder: bin,
            data_folder: data,
        }
    }
}

pub enum TariConfig {
    Default,
    Supplied(PathBuf),
    Inline(String),
}

pub enum Network {
    Stibbons,
    Weatherwax,
    Mainnet,
}
