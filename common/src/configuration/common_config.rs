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

use serde::{Deserialize, Serialize};

use crate::SubConfigPath;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommonConfig {
    override_from: Option<String>,
    pub base_path: PathBuf,
}

impl Default for CommonConfig {
    fn default() -> Self {
        let base_path = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(PathBuf::from(".tari"));
        Self {
            override_from: None,
            base_path,
        }
    }
}

impl SubConfigPath for CommonConfig {
    fn main_key_prefix() -> &'static str {
        "common"
    }
}

impl CommonConfig {
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default_common_config() {
        let default_common_config = CommonConfig::default();

        assert!(default_common_config.override_from.is_none());
        assert_eq!(
            *default_common_config.base_path(),
            dirs_next::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(PathBuf::from(".tari"))
        );
    }

    #[test]
    fn main_key_prefix_test() {
        assert_eq!(CommonConfig::main_key_prefix(), "common");
    }
}
