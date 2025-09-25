//  Copyright 2021, The Tari Project
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

mod dns;

mod service;
pub use service::{SoftwareUpdaterHandle, SoftwareUpdaterService};

mod error;
use std::{
    fmt,
    fmt::{Display, Formatter},
    str::FromStr,
    time::Duration,
};

pub use error::AutoUpdateError;
use futures::future;
// Re-exports of foreign types used in public interface
pub use semver::Version;
use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::{
        bootstrap::ApplicationType,
        serializers::optional_seconds,
        utils::{deserialize_string_or_struct, serialize_string},
        StringList,
    },
    DnsNameServer,
    SubConfigPath,
};
use tari_utilities::hex::Hex;

use crate::{
    auto_update::dns::UpdateSpec,
    signature_verification::{self, SignedMessageVerifier},
};

const LOG_TARGET: &str = "p2p::auto_update";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoUpdateConfig {
    override_from: Option<String>,
    #[serde(
        deserialize_with = "deserialize_string_or_struct",
        serialize_with = "serialize_string"
    )]
    pub name_server: DnsNameServer,
    pub update_uris: StringList,
    pub use_dnssec: bool,
    pub download_base_url: String,
    pub hashes_url: String,
    pub hashes_sig_url: String,
    #[serde(with = "optional_seconds")]
    pub check_interval: Option<Duration>,
}

impl Default for AutoUpdateConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            name_server: DnsNameServer::from_str("1.1.1.1:53/cloudflare.net").unwrap(),
            update_uris: vec![].into(),
            use_dnssec: false,
            download_base_url: String::new(),
            hashes_url: String::new(),
            hashes_sig_url: String::new(),
            check_interval: None,
        }
    }
}

impl SubConfigPath for AutoUpdateConfig {
    fn main_key_prefix() -> &'static str {
        "auto_update"
    }
}

impl AutoUpdateConfig {
    pub fn is_update_enabled(&self) -> bool {
        !self.update_uris.is_empty()
    }
}

pub async fn check_for_updates(
    app: ApplicationType,
    arch: &str,
    version: &Version,
    config: AutoUpdateConfig,
) -> Result<Option<SoftwareUpdate>, AutoUpdateError> {
    let download_base_url = config.download_base_url.clone();
    let hashes_url = config.hashes_url.clone();
    let hashes_sig_url = config.hashes_sig_url.clone();
    let dns_update = dns::DnsSoftwareUpdate::connect(config)?;

    match dns_update.check_for_updates(app, arch, version).await? {
        Some(update_spec) => {
            log::debug!(
                target: LOG_TARGET,
                "New unverified update found ({update_spec}). Verifying..."

            );
            let (hashes, sig) = future::join(
                signature_verification::download_hashes_file(&hashes_url),
                signature_verification::download_hashes_sig_file(&hashes_sig_url),
            )
            .await;
            let hashes = hashes?;
            let sig = sig?;
            let verifier = SignedMessageVerifier::new(signature_verification::maintainers().collect());
            match verifier.verify_signed_hashes(&sig, &hashes, &update_spec.hash) {
                Ok((_, filename)) => {
                    let download_url = format!("{download_base_url}/{filename}");
                    log::info!(target: LOG_TARGET, "Valid update found at {download_url}");
                    Ok(Some(SoftwareUpdate {
                        spec: update_spec,
                        download_url,
                    }))
                },
                Err(_) => Ok(None),
            }
        },
        None => {
            log::info!("No new updates for {app} ({arch} {version})");
            Ok(None)
        },
    }
}

#[derive(Debug, Clone)]
pub struct SoftwareUpdate {
    spec: UpdateSpec,
    download_url: String,
}

impl SoftwareUpdate {
    pub fn download_url(&self) -> &str {
        &self.download_url
    }

    pub fn hash(&self) -> &[u8] {
        &self.spec.hash
    }

    /// Returns the hex representation of the SHA hash
    pub fn to_hash_hex(&self) -> String {
        self.spec.hash.to_hex()
    }

    pub fn version(&self) -> &Version {
        &self.spec.version
    }

    pub fn app(&self) -> &ApplicationType {
        &self.spec.application
    }
}

impl Display for SoftwareUpdate {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}, url = {}", self.spec, self.download_url)
    }
}

#[cfg(test)]
mod test {
    use tari_common::DefaultConfigLoader;

    use super::*;

    fn get_config(config_name: Option<&str>) -> config::Config {
        let s = match config_name {
            Some(o) => {
                format!(
                    r#"
                    [auto_update]
                    override_from="{o}"
                    check_interval=31
                    name_server="127.0.0.1:80/localtest"
                    update_uris = ["http://none", "http://local"]
                    [config_a.auto_update]
                    check_interval=33
                    name_server="127.0.0.1:80/localtest2"
                    use_dnssec=true
                    [config_b.auto_update]
                    # spelling error in name
                    use_dns_sec=true
                    "#
                )
            },
            None => r#"
[auto_update]
check_interval=31
name_server="127.0.0.1:80/localtest"
download_base_url ="http://test.com"
"#
            .to_string(),
        };

        config::Config::builder()
            .add_source(config::File::from_str(s.as_str(), config::FileFormat::Toml))
            .build()
            .unwrap()
    }

    #[test]
    fn test_no_overrides_config() {
        let cfg = get_config(None);
        let config = AutoUpdateConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.check_interval, Some(Duration::from_secs(31)));
        assert_eq!(
            config.name_server,
            DnsNameServer::from_str("127.0.0.1:80/localtest").unwrap(),
        );
        assert_eq!(config.update_uris.into_vec(), Vec::<String>::new());
        assert_eq!(config.download_base_url, "http://test.com");
        // update_uris =
        // pub update_uris: Vec<String>,
        // pub download_base_url: String,
        // pub hashes_url: String,
        // pub hashes_sig_url: String,
        // #[serde(with = "optional_seconds")]
        // pub check_interval: Option<Duration>,
    }

    #[test]
    fn test_with_overrides() {
        let cfg = get_config(Some("config_a"));
        let config = AutoUpdateConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.check_interval, Some(Duration::from_secs(33)));
        assert_eq!(
            config.name_server,
            DnsNameServer::from_str("127.0.0.1:80/localtest2").unwrap(),
        );
        assert_eq!(config.update_uris.into_vec(), vec!["http://none", "http://local"]);
        assert!(config.use_dnssec);
    }
    #[test]
    fn test_wit() {
        let cfg = get_config(Some("config_a"));
        let config = AutoUpdateConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.check_interval, Some(Duration::from_secs(33)));
        assert_eq!(
            config.name_server,
            DnsNameServer::from_str("127.0.0.1:80/localtest2").unwrap(),
        );
        assert_eq!(config.update_uris.into_vec(), vec!["http://none", "http://local"]);
        assert!(config.use_dnssec);
    }

    #[test]
    fn test_incorrect_spelling() {
        let cfg = get_config(Some("config_b"));
        assert!(AutoUpdateConfig::load_from(&cfg).is_err());
    }
}
