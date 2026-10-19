// Copyright 2023 The Tari Project
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

use log::warn;
use serde::{Deserialize, Deserializer, Serialize};

const LOG_TARGET: &str = "comms::peer_validator::config";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerValidatorConfig {
    /// The maximum size of the peer's user agent string. Some unicode characters use more than a single byte
    /// and this specifies the maximum in bytes, as opposed to unicode characters.
    pub max_user_agent_byte_length: usize,
    pub max_permitted_peer_addresses_per_claim: usize,
    pub max_supported_protocols: usize,
    pub max_protocol_id_length: usize,

    /// Set to true to allow peers to send loopback, local-link and other addresses normally not considered valid for
    /// peer-to-peer comms. Default: false
    pub allow_test_addresses: bool,
    /// Removed setting. Accepted and ignored with a warning, so that config files that still set it load.
    #[serde(default, skip_serializing, deserialize_with = "warn_min_peer_version_ignored")]
    pub min_peer_version: (),
}

impl Default for PeerValidatorConfig {
    fn default() -> Self {
        Self {
            max_user_agent_byte_length: 50,
            max_permitted_peer_addresses_per_claim: 5,
            max_supported_protocols: 20,
            max_protocol_id_length: 50,
            #[cfg(not(test))]
            allow_test_addresses: false,
            // This must always be true for internal crate tests
            #[cfg(test)]
            allow_test_addresses: true,
            min_peer_version: (),
        }
    }
}

fn warn_min_peer_version_ignored<'de, D: Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    String::deserialize(deserializer)?;
    warn!(
        target: LOG_TARGET,
        "The `peer_validator_config.min_peer_version` setting is no longer used and is ignored. Remove it from the \
         config file."
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn removed_min_peer_version_setting_still_parses() {
        let mut value = toml::Value::try_from(PeerValidatorConfig::default()).unwrap();
        let table = value.as_table_mut().unwrap();
        assert!(!table.contains_key("min_peer_version"));
        table.insert("min_peer_version".to_string(), "1.0.0".into());

        let config = value.try_into::<PeerValidatorConfig>().unwrap();
        assert_eq!(
            config.max_user_agent_byte_length,
            PeerValidatorConfig::default().max_user_agent_byte_length
        );
    }
}
