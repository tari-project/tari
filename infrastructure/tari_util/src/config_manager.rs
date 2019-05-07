// Copyright 2019 The Tari Project
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

use config::*;
use derive_error::Error;
use serde::de::Deserialize;
use std::fs;

// TODO add clap

struct ConfigManager {
    fileconfig: Config,
}

#[derive(Debug, Error)]
pub enum ConfigManagerError {
    // Could not load file
    FileLoadError(ConfigError),
    // Could not find the file
    FileNotFound,
    // Could not be translated into object
    FileDeseriliseError,
}

impl ConfigManager {
    /// This creates a new ConfigManager
    pub fn new() -> ConfigManager {
        let c = Config::default();
        ConfigManager { fileconfig: c }
    }

    /// This function adds a configuration file be loaded and searched for settings
    pub fn add_file(&mut self, filename: String) -> Result<(), ConfigManagerError> {
        if fs::metadata(&filename).is_err() {
            return Err(ConfigManagerError::FileNotFound);
        }
        self.fileconfig
            .merge(File::new(&filename, FileFormat::Toml))
            .map_err(|e| ConfigManagerError::FileLoadError(e))?;
        Ok(())
    }

    /// Attempt to deserialize the entire configuration into the requested type.
    pub fn try_into<'de, T: Deserialize<'de>>(self) -> Result<T, ConfigManagerError> {
        T::deserialize(self.fileconfig).map_err(|_| ConfigManagerError::FileDeseriliseError)
    }
}
