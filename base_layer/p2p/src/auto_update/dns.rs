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

use std::{
    fmt,
    fmt::{Display, Formatter},
    str::FromStr,
};

use anyhow::anyhow;
use futures::future;
use tari_common::configuration::bootstrap::ApplicationType;
use tari_utilities::hex::{from_hex, Hex, HexError};
use thiserror::Error;

use super::{error::AutoUpdateError, AutoUpdateConfig, Version};
use crate::dns::{default_trust_anchor, DnsClient};

const LOG_TARGET: &str = "p2p::auto_update::dns";

pub struct DnsSoftwareUpdate {
    client: DnsClient,
    config: AutoUpdateConfig,
}

impl DnsSoftwareUpdate {
    /// Connect to DNS host according to the given config
    pub async fn connect(config: AutoUpdateConfig) -> Result<Self, AutoUpdateError> {
        let name_server = config.name_server.clone();
        let client = if config.use_dnssec {
            DnsClient::connect_secure(name_server, default_trust_anchor()).await?
        } else {
            DnsClient::connect(name_server).await?
        };

        Ok(Self { client, config })
    }

    pub async fn check_for_updates(
        &self,
        app: ApplicationType,
        arch: &str,
        current_version: &Version,
    ) -> Result<Option<UpdateSpec>, AutoUpdateError> {
        let records = self.config.update_uris.iter().map(|addr| {
            let mut client = self.client.clone();
            async move {
                log::debug!(target: LOG_TARGET, "Checking {} for updates...", addr);
                match client.query_txt(addr.as_str()).await {
                    Ok(recs) => recs
                        .iter()
                        .filter_map(|s| UpdateSpec::from_str(s).ok())
                        .map(|update| {
                            log::trace!(target: LOG_TARGET, "Update: {}", update);
                            update
                        })
                        .filter(|u| u.application == app)
                        .filter(|u| u.arch.as_str() == arch)
                        .filter(|u| u.version > *current_version)
                        .collect::<Vec<_>>(),
                    Err(err) => {
                        log::warn!(target: LOG_TARGET, "Failed to retrieve TXT records: {}", err);
                        Vec::new()
                    },
                }
            }
        });

        let records = future::join_all(records).await;

        let best_update = records
            .iter()
            .flatten()
            .fold(Option::<&UpdateSpec>::None, |best_update, update| match best_update {
                Some(u) if u.version < update.version => Some(update),
                Some(u) => Some(u),
                None => Some(update),
            });

        match best_update {
            Some(best_update) => {
                // Check that a majority of URLs agree
                let majority = self.config.update_uris.len() / 2 + 1;
                let count = records
                    .iter()
                    .flatten()
                    .filter(|u| u.version == best_update.version)
                    .count();

                if count < majority {
                    log::warn!(
                        target: LOG_TARGET,
                        "Too few update URLs have the update to version {}. {} out of {}. {} are required",
                        best_update.version,
                        count,
                        self.config.update_uris.len(),
                        majority
                    );
                    return Ok(None);
                }

                log::debug!(target: LOG_TARGET, "Update found! {}", best_update);
                Ok(Some(best_update.clone()))
            },
            None => {
                log::debug!(
                    target: LOG_TARGET,
                    "No new updates found. Current version {}",
                    current_version
                );
                Ok(None)
            },
        }
    }
}

#[derive(Debug, Error, PartialEq)]
enum DnsError {
    #[error("Could not convert into hex: `{0}`")]
    HexError(String),
}

impl From<HexError> for DnsError {
    fn from(e: HexError) -> Self {
        DnsError::HexError(e.to_string())
    }
}

/// Software update records
#[derive(Debug, Clone)]
pub struct UpdateSpec {
    pub application: ApplicationType,
    pub arch: String,
    pub version: Version,
    pub hash: Vec<u8>,
}

impl FromStr for UpdateSpec {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(':');
        let application = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("No application in TXT record"))?;
        let arch = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("No arch in TXT record"))?;
        let version = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("No version in TXT record"))?;
        let hash = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("No hash in TXT record"))?;
        let hash = from_hex(hash).map_err(|e| DnsError::HexError(format!("{}", e)))?;
        if parts.next().is_some() {
            return Err(anyhow!("String contained too many parts"));
        }

        Ok(UpdateSpec {
            application: application.parse()?,
            arch: arch.to_string(),
            version: version.parse()?,
            hash,
        })
    }
}

impl Display for UpdateSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "app = {}, arch = {}, version = {}, hash = {}",
            self.application,
            self.arch,
            self.version,
            self.hash.to_hex()
        )
    }
}

#[cfg(test)]
mod test {
    use hickory_client::{
        op::Query,
        proto::{
            rr::{rdata, Name, RData, RecordType},
            xfer::DnsResponse,
        },
        rr::Record,
    };

    use super::*;
    use crate::dns::mock;

    fn create_txt_record(contents: Vec<&str>) -> DnsResponse {
        let resp_query = Query::query(Name::from_str("test.local.").unwrap(), RecordType::A);

        let origin = Name::parse("example.com.", None).unwrap();
        let record = Record::from_rdata(
            origin,
            86300,
            RData::TXT(rdata::TXT::new(contents.into_iter().map(ToString::to_string).collect())),
        );
        DnsResponse::from_message(mock::message(resp_query, vec![record], vec![], vec![])).unwrap()
    }

    mod update_spec {
        use super::*;

        #[test]
        fn it_parses_update_spec_string() {
            let update_spec = UpdateSpec::from_str("base-node:linux-x64:1.0.0:bada55").unwrap();
            assert_eq!(update_spec.application, ApplicationType::BaseNode);
            assert_eq!(update_spec.arch, "linux-x64");
            assert_eq!(update_spec.version.to_string(), "1.0.0");
            assert_eq!(update_spec.hash, [0xBA, 0xDA, 0x55]);
        }
    }

    mod dns_software_update {
        use std::time::Duration;

        use super::*;

        impl AutoUpdateConfig {
            fn get_test_defaults() -> Self {
                Self {
                    override_from: None,
                    name_server: Default::default(),
                    update_uris: vec!["test.local".to_string()].into(),
                    use_dnssec: true,
                    download_base_url: "https://tari-binaries.s3.amazonaws.com/latest".to_string(),
                    hashes_url: "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt"
                        .to_string(),
                    hashes_sig_url:
                        "https://raw.githubusercontent.com/tari-project/tari/development/meta/hashes.txt.sig"
                            .to_string(),
                    check_interval: Some(Duration::from_secs(30)),
                }
            }
        }

        #[tokio::test]
        async fn it_ignores_non_conforming_txt_entries() {
            let records = vec![
                Ok(create_txt_record(vec![":::"])),
                Ok(create_txt_record(vec!["base-node:::"])),
                Ok(create_txt_record(vec!["base-node::1.0:"])),
                Ok(create_txt_record(vec!["base-node:android-armv7:0.1.0:abcdef"])),
                Ok(create_txt_record(vec!["base-node:linux-x86_64:1.0.0:bada55"])),
            ];
            let updater = DnsSoftwareUpdate {
                client: DnsClient::connect_mock(records).await.unwrap(),
                config: AutoUpdateConfig::get_test_defaults(),
            };
            let spec = updater
                .check_for_updates(ApplicationType::BaseNode, "linux-x86_64", &"1.0.0".parse().unwrap())
                .await
                .unwrap();
            assert!(spec.is_none());
        }

        #[tokio::test]
        async fn it_returns_best_update() {
            let records = vec![
                Ok(create_txt_record(vec!["base-node:linux-x86_64:1.0.0:abcdef"])),
                Ok(create_txt_record(vec!["base-node:linux-x86_64:1.0.1:abcdef01"])),
            ];
            let updater = DnsSoftwareUpdate {
                client: DnsClient::connect_mock(records).await.unwrap(),
                config: AutoUpdateConfig::get_test_defaults(),
            };
            let spec = updater
                .check_for_updates(ApplicationType::BaseNode, "linux-x86_64", &"1.0.0".parse().unwrap())
                .await
                .unwrap()
                .unwrap();

            assert_eq!(spec.version.to_string(), "1.0.1");
            assert_eq!(spec.hash, [0xab, 0xcd, 0xef, 0x01]);
        }
    }
}
