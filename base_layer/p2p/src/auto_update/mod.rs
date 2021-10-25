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
mod signature;

mod service;
pub use service::{SoftwareUpdaterHandle, SoftwareUpdaterService};

mod error;
pub use error::AutoUpdateError;

// Re-exports of foreign types used in public interface
pub use semver::Version;
pub use trust_dns_client::rr::dnssec::TrustAnchor;

use crate::auto_update::{dns::UpdateSpec, signature::SignedMessageVerifier};
use futures::future;
use pgp::Deserializable;
use reqwest::IntoUrl;
use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    net::SocketAddr,
};
use tari_common::configuration::bootstrap::ApplicationType;
use tari_utilities::hex::Hex;

const LOG_TARGET: &str = "p2p::auto_update";

#[derive(Debug, Clone)]
pub struct AutoUpdateConfig {
    pub name_server: SocketAddr,
    pub update_uris: Vec<String>,
    pub use_dnssec: bool,
    pub download_base_url: String,
    pub hashes_url: String,
    pub hashes_sig_url: String,
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
    let dns_update = dns::DnsSoftwareUpdate::connect(config).await?;

    match dns_update.check_for_updates(app, arch, version).await? {
        Some(update_spec) => {
            log::debug!(
                target: LOG_TARGET,
                "New unverified update found ({}). Verifying...",
                update_spec
            );
            let (hashes, sig) = future::join(
                download_hashes_file(&hashes_url),
                download_hashes_sig_file(&hashes_sig_url),
            )
            .await;
            let hashes = hashes?;
            let sig = sig?;
            let verifier = SignedMessageVerifier::new(maintainers().collect());
            verifier
                .verify_signed_update(&sig, &hashes, &update_spec)
                .map(|(_, filename)| {
                    let download_url = format!("{}/{}", download_base_url, filename);
                    log::info!(target: LOG_TARGET, "Valid update found at {}", download_url);
                    Ok(SoftwareUpdate {
                        spec: update_spec,
                        download_url,
                    })
                })
                .transpose()
        },
        None => {
            log::info!("No new updates for {} ({} {})", app, arch, version);
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

async fn download_hashes_file<T: IntoUrl>(url: T) -> Result<String, AutoUpdateError> {
    let resp = http_download(url).await?;
    let txt = resp.text().await?;
    Ok(txt)
}

async fn download_hashes_sig_file<T: IntoUrl>(url: T) -> Result<pgp::StandaloneSignature, AutoUpdateError> {
    let resp = http_download(url).await?;
    let sig_bytes = resp.bytes().await?;
    let cursor = io::Cursor::new(&sig_bytes);
    let sig = pgp::StandaloneSignature::from_bytes(cursor).map_err(AutoUpdateError::SignatureError)?;
    Ok(sig)
}

async fn http_download<T: IntoUrl>(url: T) -> Result<reqwest::Response, AutoUpdateError> {
    let resp = reqwest::get(url).await?.error_for_status()?;
    Ok(resp)
}

const MAINTAINERS: &[&str] = &[
    include_str!("../../../../meta/gpg_keys/CjS77.asc"),
    include_str!("../../../../meta/gpg_keys/hansieodendaal.asc"),
    include_str!("../../../../meta/gpg_keys/philipr-za.asc"),
    include_str!("../../../../meta/gpg_keys/sdbondi.asc"),
    include_str!("../../../../meta/gpg_keys/swvheerden.asc"),
    include_str!("../../../../meta/gpg_keys/delta1.asc"),
];

fn maintainers() -> impl Iterator<Item = pgp::SignedPublicKey> {
    MAINTAINERS.iter().map(|s| {
        let (pk, _) = pgp::SignedPublicKey::from_string(s).expect("Malformed maintainer PGP signature");
        pk
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn all_maintainers_well_formed() {
        assert_eq!(maintainers().count(), MAINTAINERS.len());
    }
}
